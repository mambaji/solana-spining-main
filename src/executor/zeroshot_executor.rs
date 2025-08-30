use async_trait::async_trait;
use base64::prelude::*;
use reqwest::Client;
use solana_sdk::{
    signature::{Keypair, Signature, Signer},
    transaction::VersionedTransaction,
    pubkey::Pubkey,
    system_instruction,
};
use serde_json::{json, Value};
use std::time::{Duration, Instant};
use std::sync::Arc;
use log::{info, warn};

use crate::executor::{
    traits::{TransactionExecutor, ExecutionStrategy, ExecutionResult, TradeParams},
    errors::ExecutionError,
    config::ZeroShotConfig,
    transaction_builder::TransactionBuilder,
    blockhash_cache::BlockhashCache,
};

/// ZeroSlot交易执行器
pub struct ZeroShotExecutor {
    config: ZeroShotConfig,
    client: Client,
    wallet: Keypair,
    transaction_builder: TransactionBuilder,
    blockhash_cache: Arc<BlockhashCache>,
}

impl ZeroShotExecutor {
    /// 创建新的ZeroSlot执行器
    pub fn new(config: ZeroShotConfig, wallet: Keypair, blockhash_cache: Arc<BlockhashCache>) -> Result<Self, ExecutionError> {
        let client = Client::builder()
            .timeout(Duration::from_secs(config.timeout_seconds))
            .build()
            .map_err(|e| ExecutionError::Configuration(format!("Failed to create HTTP client: {}", e)))?;

        Ok(Self {
            config,
            client,
            wallet,
            transaction_builder: TransactionBuilder::new(),
            blockhash_cache,
        })
    }

    /// 获取区域化端点
    fn get_endpoint(&self, region: Option<&str>) -> String {
        let region = region.unwrap_or(&self.config.default_region);
        
        self.config.regional_endpoints
            .get(region)
            .cloned()
            .unwrap_or_else(|| {
                warn!("Region {} not found, using base endpoint", region);
                self.config.base_endpoint.clone()
            })
    }

    /// 提交交易到ZeroSlot
    async fn submit_transaction_to_zeroshot(
        &self,
        transaction: &VersionedTransaction,
        region: Option<&str>,
    ) -> Result<Signature, ExecutionError> {
        let endpoint = self.get_endpoint(region);
        let url = format!("{}?api-key={}", endpoint, self.config.api_key);
        info!("submit_transaction_to_zeroshot: {}", url);
        // 序列化交易
        let serialized_tx = bincode::serialize(transaction)
            .map_err(|e| ExecutionError::Serialization(format!("Failed to serialize transaction: {}", e)))?;
        
        let tx_base64 = base64::prelude::BASE64_STANDARD.encode(serialized_tx);

        // 使用标准JSON-RPC 2.0格式
        let request_body = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "sendTransaction",
            "params": [
                tx_base64,
                {
                    "encoding": "base64",
                    "skipPreflight": true
                }
            ]
        });

        let response = self.client
            .post(&url)
            .header("Content-Type", "application/json")
            .json(&request_body)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
            return Err(ExecutionError::ServiceUnavailable {
                service: "ZeroSlot".to_string(),
                reason: format!("HTTP {}: {}", status, error_text),
            });
        }

        let result: Value = response.json().await?;
        
        // 处理JSON-RPC响应
        if let Some(signature_str) = result.get("result").and_then(|v| v.as_str()) {
            signature_str.parse::<Signature>()
                .map_err(|e| ExecutionError::Serialization(format!("Invalid signature: {}", e)))
        } else if let Some(error) = result.get("error") {
            Err(ExecutionError::TransactionFailed {
                message: format!("RPC Error: {}", error),
                signature: None,
            })
        } else {
            Err(ExecutionError::ServiceUnavailable {
                service: "ZeroSlot".to_string(),
                reason: "Unexpected response format".to_string(),
            })
        }
    }

    /// 构建带tip的交易
    async fn build_trade_with_tip(
        &self,
        trade_params: &TradeParams,
        tip_lamports: u64,
        _region: Option<&str>,
    ) -> Result<VersionedTransaction, ExecutionError> {
        // 从缓存获取最新区块哈希
        let recent_blockhash = self.blockhash_cache.get_cached_blockhash()
            .or_else(|_| {
                warn!("📋 Failed to get cached blockhash, attempting fresh fetch");
                // 如果缓存失败，尝试同步获取
                futures::executor::block_on(self.blockhash_cache.get_fresh_blockhash())
            })?;

        // 构建tip指令 (转账到ZeroSlot tip地址)
        let tip_recipient = self.get_zeroshot_tip_address(None)?;
        let tip_instruction = system_instruction::transfer(
            &self.wallet.pubkey(),
            &tip_recipient,
            tip_lamports,
        );

        // 使用transaction_builder的统一方法构建交易
        if trade_params.is_buy {
            if let Some(creator) = &trade_params.creator {
                // 使用简化版本，避免重复创建账户
                self.transaction_builder.build_complete_pumpfun_buy_transaction_with_tip_and_manual_account(
                    &trade_params.mint,
                    &self.wallet,
                    trade_params.sol_amount,
                    trade_params.min_tokens_out,
                    creator,
                    tip_instruction,
                    recent_blockhash,
                )
            } else {
                return Err(ExecutionError::InvalidParams(
                    "Creator address is required for PumpFun buy transactions".to_string()
                ));
            }
        } else {
            return Err(ExecutionError::InvalidParams("Sell transactions not implemented yet".to_string()));
        }
    }

    /// 获取ZeroSlot tip地址 (从配置中随机选择)
    fn get_zeroshot_tip_address(&self, _region: Option<&str>) -> Result<Pubkey, ExecutionError> {
        if self.config.tip_accounts.is_empty() {
            return Err(ExecutionError::Configuration("No tip addresses configured".to_string()));
        }
        
        // 随机选择一个tip地址（简单使用第一个）
        // TODO: 可以改为随机选择或负载均衡
        let tip_address = &self.config.tip_accounts[0];
        
        Pubkey::try_from(tip_address.as_str())
            .map_err(|e| ExecutionError::Configuration(format!("Invalid tip address '{}': {}", tip_address, e)))
    }
}

#[async_trait]
impl TransactionExecutor for ZeroShotExecutor {
    /// 执行交易
    async fn execute_trade(
        &self,
        trade_params: TradeParams,
        strategy: ExecutionStrategy,
    ) -> Result<ExecutionResult, ExecutionError> {
        let start_time = Instant::now();

        let (tip_lamports, region) = match &strategy {
            ExecutionStrategy::ZeroSlot { tip_lamports, region } => (*tip_lamports, region.as_str()),
            _ => return Err(ExecutionError::InvalidParams("Invalid strategy for ZeroSlot executor".to_string())),
        };

        info!("🚀 [ZeroSlot] 开始执行交易，tip: {} lamports, region: {}", 
              tip_lamports, region);

        // 验证参数
        self.validate_params(&trade_params)?;

        // 构建交易 (跳过余额检查，直接构建交易)
        let transaction = self.build_trade_with_tip(&trade_params, tip_lamports, Some(region)).await?;
        
        // 提交交易
        let signature = self.submit_transaction_to_zeroshot(&transaction, Some(region)).await?;
        
        info!("✅ [ZeroSlot] 交易已提交，签名: {}", signature);

        let execution_latency = start_time.elapsed().as_millis() as u64;
        
        info!("🎉 [ZeroSlot] 交易执行完成，延迟: {}ms", execution_latency);

        // 构建执行结果 (不等待确认，ZeroSlot承诺立即确认)
        let mut metadata = std::collections::HashMap::new();
        metadata.insert("region".to_string(), region.to_string());
        metadata.insert("tip_lamports".to_string(), tip_lamports.to_string());
        metadata.insert("service".to_string(), "ZeroSlot".to_string());

        Ok(ExecutionResult {
            signature,
            strategy_used: strategy,
            actual_fee_paid: tip_lamports + 5000, // tip + base transaction fee
            execution_latency_ms: execution_latency,
            confirmation_status: "submitted".to_string(), // ZeroSlot承诺立即确认
            success: true,
            metadata,
        })
    }

    /// 获取钱包余额 (简化版本，不调用0slot API)
    async fn get_balance(&self) -> Result<u64, ExecutionError> {
        // 使用Shyft或本地RPC获取余额，而不是0slot API
        Err(ExecutionError::ServiceUnavailable {
            service: "Balance".to_string(),
            reason: "Balance check disabled for 0slot integration".to_string(),
        })
    }

    /// 验证交易参数
    fn validate_params(&self, params: &TradeParams) -> Result<(), ExecutionError> {
        if params.is_buy {
            if params.sol_amount == 0 {
                return Err(ExecutionError::InvalidParams("SOL amount cannot be zero for buy transactions".to_string()));
            }
        } else {
            // 卖出交易验证
            if params.token_amount.is_none() || params.token_amount.unwrap() == 0 {
                return Err(ExecutionError::InvalidParams("Token amount is required and cannot be zero for sell transactions".to_string()));
            }
        }

        if params.max_slippage_bps > 5000 { // 50% max slippage
            return Err(ExecutionError::InvalidParams("Slippage too high".to_string()));
        }

        Ok(())
    }

    /// 检查服务健康状态
    async fn health_check(&self) -> Result<bool, ExecutionError> {
        // 检查区块哈希缓存是否正常工作
        Ok(self.blockhash_cache.is_running())
    }
}