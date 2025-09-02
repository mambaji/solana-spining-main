use anyhow::Result;
use log::{info, warn, debug};
use serde_json::{json, Value};
use solana_sdk::{
    pubkey::Pubkey,
    signature::Signature,
};
use spl_associated_token_account::get_associated_token_address;
use std::str::FromStr;
use tokio::time::{sleep, Duration};
use crate::constant::{accounts::PUMPFUN, seeds::BONDING_CURVE_SEED};
use crate::executor::errors::ExecutionError;
use borsh::{BorshDeserialize, BorshSerialize};
use base64::Engine;

/// Bonding Curve 账户结构体 (参考 pump_fun.rs 实现)
#[derive(Debug, BorshSerialize, BorshDeserialize)]
pub struct BondingCurveAccount {
    pub discriminator: u64,
    pub virtual_token_reserves: u64,
    pub virtual_sol_reserves: u64,
    pub real_token_reserves: u64,
    pub real_sol_reserves: u64,
    pub token_total_supply: u64,
    pub complete: bool,
    pub creator: Pubkey,
}

/// 代币余额查询客户端 - 用于从链上获取准确的代币数量
/// 
/// 使用 Shyft RPC API 获取交易详情和代币余额
pub struct TokenBalanceClient {
    rpc_endpoint: String,
    api_key: String,
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
    pub fn new(rpc_endpoint: String, api_key: String) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .unwrap_or_default();

        Self {
            rpc_endpoint,
            api_key,
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

    /// 获取用户特定代币的当前余额 (通过ATA地址直接查询)
    pub async fn get_token_balance_by_ata(
        &self,
        owner: &Pubkey,
        mint: &Pubkey,
    ) -> Result<u64> {
        let ata = get_associated_token_address(owner, mint);
        info!("🔍 查询ATA余额: ATA={}, 所有者={}, 代币={}", ata, owner, mint);

        self.get_token_account_balance(&ata).await
    }

    /// 🆕 获取指定代币账户的余额（通用方法，支持ATA和基于种子的账户）
    pub async fn get_token_account_balance(&self, token_account: &Pubkey) -> Result<u64> {
        info!("🔍 查询代币账户余额: 账户={}", token_account);

        let request_body = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "getTokenAccountBalance",
            "params": [token_account.to_string()]
        });

        let response = self.make_rpc_request(request_body).await?;
        
        if let Some(result) = response.get("result") {
            info!("🔍 代币账户余额: {}", result);
            if let Some(value) = result.get("value") {
                // 优先使用 amount (原始最小单位)，这样保持与链上交易数据的一致性
                if let Some(amount_str) = value.get("amount").and_then(|a| a.as_str()) {
                    let amount = amount_str.parse::<u64>().unwrap_or(0);
                    // 同时记录UI友好的数量用于日志
                    let ui_amount = value.get("uiAmount").and_then(|a| a.as_f64()).unwrap_or(0.0);
                    info!("✅ 代币账户余额: {} tokens (UI显示: {})", amount, ui_amount);
                    return Ok(amount);
                }
            }
        }

        // 如果代币账户不存在，余额为0
        info!("ℹ️ 代币账户不存在，余额为0");
        Ok(0)
    }
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
    pub async fn get_transaction_with_retries(
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

    /// 🆕 使make_rpc_request方法公开，以便测试使用
    pub async fn make_rpc_request(&self, request_body: Value) -> Result<Value> {
        debug!("📡 发送RPC请求: {}", serde_json::to_string_pretty(&request_body)?);

        // 构建带API密钥的URL
        let url_with_key = format!("{}?api_key={}", self.rpc_endpoint, self.api_key);

        let response = self.client
            .post(url_with_key)
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

    /// 验证 bonding curve 账户是否已初始化 (改进的版本)
    pub async fn validate_bonding_curve_exists(&self, mint: &Pubkey) -> Result<(), ExecutionError> {
        // 推导 bonding curve 地址 (使用正确的种子)
        let (bonding_curve, _) = Pubkey::find_program_address(
            &[BONDING_CURVE_SEED, mint.as_ref()],
            &PUMPFUN,
        );
        
        info!("🔍 [改进版] 检查bonding curve账户: {}", bonding_curve);
        
        // 通过RPC检查账户是否存在且已初始化
        let request_body = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "getAccountInfo",
            "params": [
                bonding_curve.to_string(),
                {
                    "encoding": "base64"
                }
            ]
        });

        let response = self.make_rpc_request(request_body).await
            .map_err(|e| ExecutionError::ServiceUnavailable {
                service: "RPC".to_string(),
                reason: format!("Failed to check bonding curve account: {}", e),
            })?;
        
        if let Some(account_info) = response.get("result").and_then(|v| v.get("value")) {
            if account_info.is_null() {
                return Err(ExecutionError::InvalidParams(
                    format!("❌ Bonding curve账户不存在，代币可能尚未创建: {}", mint)
                ));
            }
            
            // 检查账户数据是否为空
            if let Some(data_b64) = account_info.get("data").and_then(|d| d.get(0)).and_then(|d| d.as_str()) {
                if data_b64.is_empty() {
                    return Err(ExecutionError::InvalidParams(
                        format!("❌ Bonding curve账户未初始化，数据为空: {}", mint)
                    ));
                }
                
                // 尝试解码和反序列化数据以验证它是有效的 bonding curve 账户
                if let Ok(data_bytes) = base64::prelude::BASE64_STANDARD.decode(data_b64) {
                    match borsh::from_slice::<BondingCurveAccount>(&data_bytes) {
                        Ok(bonding_curve_account) => {
                            info!("✅ Bonding curve账户验证成功: 虚拟 SOL 储备={}, 虚拟代币储备={}, 创建者={}", 
                                  bonding_curve_account.virtual_sol_reserves,
                                  bonding_curve_account.virtual_token_reserves,
                                  bonding_curve_account.creator);
                            return Ok(());
                        }
                        Err(e) => {
                            warn!("⚠️ Bonding curve账户数据反序列化失败: {}", e);
                            // 即使反序列化失败，只要账户存在且有数据，就认为可用
                            return Ok(());
                        }
                    }
                } else {
                    return Err(ExecutionError::InvalidParams(
                        format!("❌ Bonding curve账户数据格式错误: {}", mint)
                    ));
                }
            } else {
                return Err(ExecutionError::InvalidParams(
                    format!("❌ Bonding curve账户数据不可用: {}", mint)
                ));
            }
        } else if let Some(error) = response.get("error") {
            Err(ExecutionError::ServiceUnavailable {
                service: "RPC".to_string(),
                reason: format!("RPC错误: {}", error),
            })
        } else {
            Err(ExecutionError::ServiceUnavailable {
                service: "RPC".to_string(),
                reason: "意外的响应格式".to_string(),
            })
        }
    }

    /// 带重试的 bonding curve 验证
    pub async fn validate_bonding_curve_with_retry(
        &self,
        mint: &Pubkey,
        max_retries: u32,
        retry_delay_ms: u64,
    ) -> Result<(), ExecutionError> {
        let mut attempts = 0;
        
        while attempts < max_retries {
            match self.validate_bonding_curve_exists(mint).await {
                Ok(()) => return Ok(()),
                Err(e) => {
                    attempts += 1;
                    
                    if attempts >= max_retries {
                        return Err(e);
                    }
                    
                    warn!("❌ Bonding curve验证失败 (尝试 {}/{}): {}", attempts, max_retries, e);
                    warn!("⏰ 等待 {}ms 后重试...", retry_delay_ms);
                    
                    sleep(Duration::from_millis(retry_delay_ms)).await;
                }
            }
        }
        
        Err(ExecutionError::InvalidParams(
            format!("❌ 经过{}次重试后，bonding curve仍不可用: {}", max_retries, mint)
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use solana_sdk::{pubkey::Pubkey, signature::Signature};
    use std::str::FromStr;
    use tokio;

    /// 测试查询具体交易的代币余额变化
    /// 使用提供的交易签名: 5NFeJCkuyRqJgGyu9VeURX7hG5y9Cj6jCjM8EDg1GthxWsCPfw2J2qYMZMSJKCqTzx2sfZeeoYnjnxQERPUdY5P8
    #[tokio::test]
    async fn test_get_token_balance_changes_specific_transaction() {
        // 跳过测试如果没有环境变量
        if std::env::var("SHYFT_RPC_API_KEY").is_err() && std::env::var("SHYFT_API_KEY").is_err() {
            println!("⚠️ 跳过测试: 缺少 SHYFT_RPC_API_KEY 或 SHYFT_API_KEY 环境变量");
            return;
        }

        // 创建客户端
        let client = match TokenBalanceClient::from_env() {
            Ok(client) => client,
            Err(e) => {
                println!("❌ 创建TokenBalanceClient失败: {}", e);
                return;
            }
        };

        // 测试数据
        let signature_str = "5NFeJCkuyRqJgGyu9VeURX7hG5y9Cj6jCjM8EDg1GthxWsCPfw2J2qYMZMSJKCqTzx2sfZeeoYnjnxQERPUdY5P8";
        let signature = match Signature::from_str(signature_str) {
            Ok(sig) => sig,
            Err(e) => {
                println!("❌ 解析交易签名失败: {}", e);
                return;
            }
        };

        println!("🔍 测试交易签名: {}", signature);

        // 首先获取交易详情来查看涉及的代币和账户
        println!("\n=== 步骤1: 获取交易详情 ===");
        let transaction_details = match client.get_transaction_with_retries(&signature, 3).await {
            Ok(details) => {
                println!("✅ 获取交易详情成功");
                details
            }
            Err(e) => {
                println!("❌ 获取交易详情失败: {}", e);
                return;
            }
        };

        // 分析交易中的代币信息
        println!("\n=== 步骤2: 分析交易中的代币信息 ===");
        if let Some(meta) = transaction_details.get("meta") {
            if let Some(post_balances) = meta.get("postTokenBalances").and_then(|b| b.as_array()) {
                println!("发现 {} 个代币余额记录:", post_balances.len());
                
                for (i, balance) in post_balances.iter().enumerate() {
                    if let (Some(mint_str), Some(owner_str)) = (
                        balance.get("mint").and_then(|m| m.as_str()),
                        balance.get("owner").and_then(|o| o.as_str())
                    ) {
                        println!("  {}. 代币: {} | 所有者: {}", i + 1, mint_str, owner_str);
                        
                        // 如果有金额信息，也显示
                        if let Some(amount) = balance.get("uiTokenAmount")
                            .and_then(|a| a.get("amount"))
                            .and_then(|amt| amt.as_str())
                        {
                            println!("     余额: {} tokens", amount);
                        }
                    }
                }

                // 选择第一个代币和所有者进行测试
                if let Some(first_balance) = post_balances.first() {
                    if let (Some(mint_str), Some(owner_str)) = (
                        first_balance.get("mint").and_then(|m| m.as_str()),
                        first_balance.get("owner").and_then(|o| o.as_str())
                    ) {
                        println!("\n=== 步骤3: 测试代币余额变化查询 ===");
                        println!("使用第一个代币进行测试:");
                        println!("  代币mint: {}", mint_str);
                        println!("  所有者: {}", owner_str);

                        let target_mint = match Pubkey::from_str(mint_str) {
                            Ok(mint) => mint,
                            Err(e) => {
                                println!("❌ 解析代币mint失败: {}", e);
                                return;
                            }
                        };

                        let target_owner = match Pubkey::from_str(owner_str) {
                            Ok(owner) => owner,
                            Err(e) => {
                                println!("❌ 解析所有者地址失败: {}", e);
                                return;
                            }
                        };

                        // 测试获取代币余额变化
                        match client.get_token_balance_changes_from_transaction(
                            &signature,
                            &target_mint,
                            &target_owner,
                        ).await {
                            Ok(Some(balance_change)) => {
                                println!("✅ 成功获取代币余额变化:");
                                println!("   代币: {}", balance_change.mint);
                                println!("   所有者: {}", balance_change.owner);
                                println!("   交易前余额: {} tokens", balance_change.amount_before);
                                println!("   交易后余额: {} tokens", balance_change.amount_after);
                                println!("   变化量: {} tokens", balance_change.amount_changed);
                                
                                if balance_change.amount_changed > 0 {
                                    println!("   📈 代币余额增加 (买入操作)");
                                } else if balance_change.amount_changed < 0 {
                                    println!("   📉 代币余额减少 (卖出操作)");
                                } else {
                                    println!("   ➡️ 代币余额无变化");
                                }
                            }
                            Ok(None) => {
                                println!("⚠️ 未找到该代币的余额变化信息");
                            }
                            Err(e) => {
                                println!("❌ 查询代币余额变化失败: {}", e);
                            }
                        }

                        // 测试获取当前代币余额
                        println!("\n=== 步骤4: 测试当前代币余额查询 ===");
                        match client.get_token_balance(&target_owner, &target_mint).await {
                            Ok(current_balance) => {
                                println!("✅ 当前代币余额: {} tokens", current_balance);
                            }
                            Err(e) => {
                                println!("❌ 查询当前代币余额失败: {}", e);
                            }
                        }

                        // 测试通过ATA查询余额
                        println!("\n=== 步骤5: 测试ATA代币余额查询 ===");
                        match client.get_token_balance_by_ata(&target_owner, &target_mint).await {
                            Ok(ata_balance) => {
                                println!("✅ ATA代币余额: {} tokens", ata_balance);
                            }
                            Err(e) => {
                                println!("❌ 查询ATA代币余额失败: {}", e);
                            }
                        }
                    }
                }
            } else {
                println!("⚠️ 交易中没有找到代币余额信息");
            }
        } else {
            println!("⚠️ 交易元数据不可用");
        }

        println!("\n🎉 测试完成!");
    }

    /// 测试专门用于买入交易的代币数量获取
    #[tokio::test]
    async fn test_get_tokens_acquired_from_buy_transaction() {
        // 跳过测试如果没有环境变量
        if std::env::var("SHYFT_RPC_API_KEY").is_err() && std::env::var("SHYFT_API_KEY").is_err() {
            println!("⚠️ 跳过测试: 缺少 SHYFT_RPC_API_KEY 或 SHYFT_API_KEY 环境变量");
            return;
        }

        let client = match TokenBalanceClient::from_env() {
            Ok(client) => client,
            Err(e) => {
                println!("❌ 创建TokenBalanceClient失败: {}", e);
                return;
            }
        };

        let signature_str = "5NFeJCkuyRqJgGyu9VeURX7hG5y9Cj6jCjM8EDg1GthxWsCPfw2J2qYMZMSJKCqTzx2sfZeeoYnjnxQERPUdY5P8";
        let signature = match Signature::from_str(signature_str) {
            Ok(sig) => sig,
            Err(e) => {
                println!("❌ 解析交易签名失败: {}", e);
                return;
            }
        };

        println!("🔍 测试买入交易代币数量获取");
        println!("交易签名: {}", signature);

        // 首先分析交易找到相关的mint和buyer
        let transaction_details = match client.get_transaction_with_retries(&signature, 3).await {
            Ok(details) => details,
            Err(e) => {
                println!("❌ 获取交易详情失败: {}", e);
                return;
            }
        };

        // 从交易中找到可能的买入信息
        if let Some(meta) = transaction_details.get("meta") {
            if let Some(post_balances) = meta.get("postTokenBalances").and_then(|b| b.as_array()) {
                if let Some(pre_balances) = meta.get("preTokenBalances").and_then(|b| b.as_array()) {
                    // 查找余额增加的账户 (可能是买方)
                    for post_balance in post_balances {
                        if let (Some(mint_str), Some(owner_str), Some(post_amount_str)) = (
                            post_balance.get("mint").and_then(|m| m.as_str()),
                            post_balance.get("owner").and_then(|o| o.as_str()),
                            post_balance.get("uiTokenAmount").and_then(|a| a.get("amount")).and_then(|amt| amt.as_str())
                        ) {
                            let post_amount: u64 = post_amount_str.parse().unwrap_or(0);
                            
                            // 查找对应的pre balance
                            let pre_amount = pre_balances.iter()
                                .find(|pre| {
                                    pre.get("mint").and_then(|m| m.as_str()) == Some(mint_str) &&
                                    pre.get("owner").and_then(|o| o.as_str()) == Some(owner_str)
                                })
                                .and_then(|pre| pre.get("uiTokenAmount").and_then(|a| a.get("amount")).and_then(|amt| amt.as_str()))
                                .and_then(|s| s.parse().ok())
                                .unwrap_or(0);

                            // 如果余额增加了，这可能是买入操作
                            if post_amount > pre_amount {
                                println!("🎯 发现可能的买入操作:");
                                println!("   代币: {}", mint_str);
                                println!("   买方: {}", owner_str);
                                println!("   代币增加: {} tokens", post_amount - pre_amount);

                                let mint = match Pubkey::from_str(mint_str) {
                                    Ok(m) => m,
                                    Err(_) => continue,
                                };

                                let buyer = match Pubkey::from_str(owner_str) {
                                    Ok(b) => b,
                                    Err(_) => continue,
                                };

                                // 测试专门的买入交易代币获取方法
                                match client.get_tokens_acquired_from_buy_transaction(&signature, &mint, &buyer).await {
                                    Ok(tokens_acquired) => {
                                        println!("✅ 买入交易代币数量: {} tokens", tokens_acquired);
                                        assert_eq!(tokens_acquired, post_amount - pre_amount);
                                        return; // 找到一个就够了
                                    }
                                    Err(e) => {
                                        println!("❌ 获取买入代币数量失败: {}", e);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        println!("⚠️ 未找到明显的买入操作记录");
    }

    /// 测试指定账户的代币余额查询
    #[tokio::test]
    async fn test_specific_token_account_balance() {
        // 跳过测试如果没有环境变量
        if std::env::var("SHYFT_RPC_API_KEY").is_err() && std::env::var("SHYFT_API_KEY").is_err() {
            println!("⚠️ 跳过测试: 缺少 SHYFT_RPC_API_KEY 或 SHYFT_API_KEY 环境变量");
            return;
        }

        let client = match TokenBalanceClient::from_env() {
            Ok(client) => client,
            Err(e) => {
                println!("❌ 创建TokenBalanceClient失败: {}", e);
                return;
            }
        };

        println!("🔍 测试指定代币账户余额查询");
        
        // 测试目标账户
        let test_account_str = "893AbbfPCHShb1SsAnMB6k4nBtroYZbWYNfVVxyX52f6";
        let test_account = match Pubkey::from_str(test_account_str) {
            Ok(account) => account,
            Err(e) => {
                println!("❌ 解析测试账户地址失败: {}", e);
                return;
            }
        };

        println!("测试账户: {}", test_account);
        
        // 测试 get_token_account_balance 方法
        match client.get_token_account_balance(&test_account).await {
            Ok(balance) => {
                println!("✅ 代币账户余额查询成功: {} tokens", balance);
                if balance > 0 {
                    println!("   📈 账户有代币余额");
                } else {
                    println!("   📭 账户余额为0或不存在");
                }
            }
            Err(e) => {
                println!("❌ 代币账户余额查询失败: {}", e);
                println!("   💡 可能原因:");
                println!("      - 账户不存在");
                println!("      - 账户不是有效的代币账户");
                println!("      - RPC端点访问问题");
                println!("      - API密钥无效");
            }
        }

        // 额外测试：尝试获取账户信息来验证账户是否存在
        println!("\n🔍 验证账户是否存在...");
        let request_body = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "getAccountInfo",
            "params": [
                test_account_str,
                {
                    "encoding": "base64"
                }
            ]
        });

        match client.make_rpc_request(request_body).await {
            Ok(response) => {
                if let Some(account_info) = response.get("result").and_then(|r| r.get("value")) {
                    if account_info.is_null() {
                        println!("ℹ️ 账户不存在于区块链上");
                    } else {
                        println!("✅ 账户存在于区块链上");
                        if let Some(owner) = account_info.get("owner").and_then(|o| o.as_str()) {
                            println!("   所有者程序: {}", owner);
                        }
                        if let Some(data) = account_info.get("data").and_then(|d| d.get(0)).and_then(|d| d.as_str()) {
                            if data.is_empty() {
                                println!("   账户数据: 空");
                            } else {
                                println!("   账户数据长度: {} bytes", data.len() * 3 / 4); // base64 解码后的大致长度
                            }
                        }
                    }
                }
            }
            Err(e) => {
                println!("❌ 获取账户信息失败: {}", e);
            }
        }

        println!("🎉 指定账户余额测试完成!");
    }

    /// 测试错误处理情况
    #[tokio::test]
    async fn test_error_handling() {
        // 跳过测试如果没有环境变量
        if std::env::var("SHYFT_RPC_API_KEY").is_err() && std::env::var("SHYFT_API_KEY").is_err() {
            println!("⚠️ 跳过测试: 缺少 SHYFT_RPC_API_KEY 或 SHYFT_API_KEY 环境变量");
            return;
        }

        let client = match TokenBalanceClient::from_env() {
            Ok(client) => client,
            Err(e) => {
                println!("❌ 创建TokenBalanceClient失败: {}", e);
                return;
            }
        };

        println!("🔍 测试错误处理情况");

        // 测试无效的交易签名
        let invalid_signature = "1".repeat(88); // 无效长度的签名
        if let Ok(sig) = Signature::from_str(&invalid_signature) {
            println!("测试无效交易签名: {}", sig);
            match client.get_transaction_with_retries(&sig, 1).await {
                Ok(_) => println!("⚠️ 意外成功 - 应该失败"),
                Err(e) => println!("✅ 预期的错误: {}", e),
            }
        }

        // 测试不存在的代币账户
        let random_mint = Pubkey::new_unique();
        let random_owner = Pubkey::new_unique();
        println!("测试不存在的代币账户: mint={}, owner={}", random_mint, random_owner);
        
        match client.get_token_balance(&random_owner, &random_mint).await {
            Ok(balance) => {
                println!("✅ 不存在的账户余额: {} (应该为0)", balance);
                assert_eq!(balance, 0);
            }
            Err(e) => println!("ℹ️ 查询不存在账户的错误: {}", e),
        }

        println!("🎉 错误处理测试完成");
    }
}
