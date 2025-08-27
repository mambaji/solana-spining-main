use anyhow::Result;
use log::{info, warn, debug};
use serde_json::{json, Value};
use solana_sdk::{
    pubkey::Pubkey,
    signature::Signature,
};
use std::str::FromStr;
use tokio::time::{sleep, Duration};

/// 代币余额查询客户端 - 用于从链上获取准确的代币数量
/// 
/// 使用 Shyft RPC API 获取交易详情和代币余额
pub struct TokenBalanceClient {
    rpc_endpoint: String,
    client: reqwest::Client,
}

/// 代币余额变化信息
#[derive(Debug, Clone)]
pub struct TokenBalanceChange {
    pub mint: Pubkey,
    pub owner: Pubkey,
    pub amount_before: u64,
    pub amount_after: u64,
    pub amount_changed: i64, // 正数表示增加，负数表示减少
}

impl TokenBalanceClient {
    /// 创建新的代币余额查询客户端
    pub fn new(rpc_endpoint: String, _api_key: String) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .unwrap_or_default();

        Self {
            rpc_endpoint,
            client,
        }
    }

    /// 从环境变量创建客户端
    pub fn from_env() -> Result<Self> {
        // 优先使用 SHYFT_RPC_API_KEY，如果没有则回退到 SHYFT_API_KEY
        let api_key = std::env::var("SHYFT_RPC_API_KEY")
            .or_else(|_| std::env::var("SHYFT_API_KEY"))
            .map_err(|_| anyhow::anyhow!("SHYFT_RPC_API_KEY or SHYFT_API_KEY environment variable is required"))?;

        let rpc_endpoint = std::env::var("SHYFT_RPC_ENDPOINT")
            .unwrap_or_else(|_| {
                // 从区域配置获取端点，默认使用ny区域
                std::env::var("SHYFT_RPC_REGION_NY")
                    .unwrap_or_else(|_| "https://rpc.ny.shyft.to".to_string())
            });

        Ok(Self::new(rpc_endpoint, api_key))
    }

    /// 从交易签名获取代币余额变化
    /// 
    /// 这个方法分析交易前后的代币余额，计算每个账户的代币数量变化
    pub async fn get_token_balance_changes_from_transaction(
        &self,
        signature: &Signature,
        target_mint: &Pubkey,
        target_owner: &Pubkey,
    ) -> Result<Option<TokenBalanceChange>> {
        info!("🔍 查询交易代币余额变化: 签名={}, 代币={}, 所有者={}", 
            signature, target_mint, target_owner);

        // 获取交易详情，包括代币余额变化
        let transaction_details = self.get_transaction_with_retries(signature, 3).await?;

        // 解析代币余额变化
        let balance_change = self.parse_token_balance_changes(
            &transaction_details,
            target_mint,
            target_owner,
        ).await?;

        if let Some(change) = &balance_change {
            info!("✅ 代币余额变化: {} tokens ({}→{})", 
                change.amount_changed.abs(), 
                change.amount_before, 
                change.amount_after);
        } else {
            warn!("⚠️ 未找到目标代币的余额变化");
        }

        Ok(balance_change)
    }

    /// 获取用户特定代币的当前余额
    pub async fn get_token_balance(
        &self,
        owner: &Pubkey,
        mint: &Pubkey,
    ) -> Result<u64> {
        info!("🔍 查询代币余额: 所有者={}, 代币={}", owner, mint);

        let request_body = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "getTokenAccountsByOwner",
            "params": [
                owner.to_string(),
                {
                    "mint": mint.to_string()
                },
                {
                    "encoding": "jsonParsed"
                }
            ]
        });

        let response = self.make_rpc_request(request_body).await?;
        
        if let Some(result) = response.get("result") {
            if let Some(value) = result.get("value").and_then(|v| v.as_array()) {
                if let Some(account) = value.first() {
                    if let Some(account_info) = account.get("account")
                        .and_then(|a| a.get("data"))
                        .and_then(|d| d.get("parsed"))
                        .and_then(|p| p.get("info"))
                    {
                        if let Some(amount_str) = account_info.get("tokenAmount")
                            .and_then(|ta| ta.get("amount"))
                            .and_then(|a| a.as_str())
                        {
                            let amount = amount_str.parse::<u64>()
                                .unwrap_or(0);
                            info!("✅ 查询到代币余额: {} tokens", amount);
                            return Ok(amount);
                        }
                    }
                }
            }
        }

        info!("ℹ️ 未找到代币账户或余额为0");
        Ok(0)
    }

    /// 带重试的获取交易详情
    async fn get_transaction_with_retries(
        &self,
        signature: &Signature,
        max_retries: u32,
    ) -> Result<Value> {
        let mut attempts = 0;
        let mut last_error = None;

        while attempts < max_retries {
            attempts += 1;
            
            match self.get_transaction(signature).await {
                Ok(transaction) => {
                    info!("✅ 获取交易详情成功 (第{}次尝试)", attempts);
                    return Ok(transaction);
                }
                Err(e) => {
                    warn!("⚠️ 获取交易详情失败 (第{}次尝试): {}", attempts, e);
                    last_error = Some(e);
                    
                    if attempts < max_retries {
                        let delay = Duration::from_millis(1000 * attempts as u64);
                        debug!("等待 {}ms 后重试...", delay.as_millis());
                        sleep(delay).await;
                    }
                }
            }
        }

        Err(last_error.unwrap_or_else(|| anyhow::anyhow!("获取交易详情失败")))
    }

    /// 获取交易详情
    async fn get_transaction(&self, signature: &Signature) -> Result<Value> {
        let request_body = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "getTransaction",
            "params": [
                signature.to_string(),
                {
                    "encoding": "jsonParsed",
                    "maxSupportedTransactionVersion": 0
                }
            ]
        });

        let response = self.make_rpc_request(request_body).await?;
        
        if let Some(result) = response.get("result") {
            if result.is_null() {
                return Err(anyhow::anyhow!("交易未找到或尚未确认"));
            }
            Ok(result.clone())
        } else if let Some(error) = response.get("error") {
            Err(anyhow::anyhow!("RPC错误: {}", error))
        } else {
            Err(anyhow::anyhow!("无效的RPC响应"))
        }
    }

    /// 解析交易中的代币余额变化
    async fn parse_token_balance_changes(
        &self,
        transaction: &Value,
        target_mint: &Pubkey,
        target_owner: &Pubkey,
    ) -> Result<Option<TokenBalanceChange>> {
        // 获取交易前后的代币余额
        let pre_balances = transaction.get("meta")
            .and_then(|m| m.get("preTokenBalances"))
            .and_then(|b| b.as_array());

        let post_balances = transaction.get("meta")
            .and_then(|m| m.get("postTokenBalances"))
            .and_then(|b| b.as_array());

        if let (Some(pre_balances), Some(post_balances)) = (pre_balances, post_balances) {
            debug!("📊 分析代币余额变化: pre={}, post={}", pre_balances.len(), post_balances.len());

            // 查找目标代币和所有者的余额变化
            let mut pre_amount = 0u64;
            let mut post_amount = 0u64;
            let mut found_target = false;

            // 检查交易前的余额
            for balance in pre_balances {
                if let (Some(mint_str), Some(owner_str), Some(amount_str)) = (
                    balance.get("mint").and_then(|m| m.as_str()),
                    balance.get("owner").and_then(|o| o.as_str()),
                    balance.get("uiTokenAmount").and_then(|a| a.get("amount")).and_then(|amt| amt.as_str())
                ) {
                    if let (Ok(mint), Ok(owner)) = (
                        Pubkey::from_str(mint_str),
                        Pubkey::from_str(owner_str)
                    ) {
                        if mint == *target_mint && owner == *target_owner {
                            pre_amount = amount_str.parse().unwrap_or(0);
                            found_target = true;
                            debug!("🔍 找到交易前余额: {} tokens", pre_amount);
                        }
                    }
                }
            }

            // 检查交易后的余额
            for balance in post_balances {
                if let (Some(mint_str), Some(owner_str), Some(amount_str)) = (
                    balance.get("mint").and_then(|m| m.as_str()),
                    balance.get("owner").and_then(|o| o.as_str()),
                    balance.get("uiTokenAmount").and_then(|a| a.get("amount")).and_then(|amt| amt.as_str())
                ) {
                    if let (Ok(mint), Ok(owner)) = (
                        Pubkey::from_str(mint_str),
                        Pubkey::from_str(owner_str)
                    ) {
                        if mint == *target_mint && owner == *target_owner {
                            post_amount = amount_str.parse().unwrap_or(0);
                            found_target = true;
                            debug!("🔍 找到交易后余额: {} tokens", post_amount);
                        }
                    }
                }
            }

            if found_target {
                let amount_changed = post_amount as i64 - pre_amount as i64;
                return Ok(Some(TokenBalanceChange {
                    mint: *target_mint,
                    owner: *target_owner,
                    amount_before: pre_amount,
                    amount_after: post_amount,
                    amount_changed,
                }));
            }
        }

        Ok(None)
    }

    /// 执行RPC请求
    async fn make_rpc_request(&self, request_body: Value) -> Result<Value> {
        debug!("📡 发送RPC请求: {}", serde_json::to_string_pretty(&request_body)?);

        let response = self.client
            .post(&self.rpc_endpoint)
            .header("Content-Type", "application/json")
            .json(&request_body)
            .send()
            .await?;

        let status = response.status();
        let response_text = response.text().await?;

        debug!("📡 RPC响应状态: {}", status);
        debug!("📡 RPC响应内容: {}", response_text);

        if !status.is_success() {
            return Err(anyhow::anyhow!("RPC请求失败: {} - {}", status, response_text));
        }

        let json_response: Value = serde_json::from_str(&response_text)?;
        Ok(json_response)
    }

    /// 从买入交易获取实际获得的代币数量
    /// 
    /// 这是一个便利方法，专门用于买入交易后获取代币数量
    pub async fn get_tokens_acquired_from_buy_transaction(
        &self,
        signature: &Signature,
        mint: &Pubkey,
        buyer: &Pubkey,
    ) -> Result<u64> {
        info!("💰 获取买入交易的代币数量: 签名={}, 代币={}, 买方={}", 
            signature, mint, buyer);

        if let Some(balance_change) = self.get_token_balance_changes_from_transaction(
            signature, mint, buyer
        ).await? {
            if balance_change.amount_changed > 0 {
                let tokens_acquired = balance_change.amount_changed as u64;
                info!("✅ 买入获得代币: {} tokens", tokens_acquired);
                return Ok(tokens_acquired);
            } else {
                warn!("⚠️ 代币数量未增加，可能交易失败");
            }
        }

        Err(anyhow::anyhow!("无法从交易中获取代币数量变化"))
    }
}
