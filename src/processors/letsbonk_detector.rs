use anyhow::Result;
use log::debug;
use solana_sdk::pubkey::Pubkey;
use std::str::FromStr;
use yellowstone_grpc_proto::geyser::SubscribeUpdateTransactionInfo;

use crate::processors::{TokenEvent, TransactionType};

#[cfg(feature = "letsbonk")]
use raydium_launchpad_interface::RaydiumLaunchpadProgramIx;

// Raydium Launchpad Program ID
const RAYDIUM_LAUNCHPAD_PROGRAM_ID: &str = "LanMV9sAd7wArD4vJFi2qDdfnVhFxYSUg6eADduJ3uj";

#[derive(Debug, Clone)]
pub struct LetsbonkTokenCreationEvent {
    pub mint: String,
    pub slot: u64,
    pub signature: String,
    pub token_name: Option<String>,
    pub token_symbol: Option<String>,
    pub token_uri: Option<String>,
}

#[derive(Debug, Clone)]
pub struct LetsbonkCreationWithBuyInfo {
    pub mint: String,
    pub slot: u64,
    pub signature: String,
    pub token_name: Option<String>,
    pub token_symbol: Option<String>,
    pub token_uri: Option<String>,
    // 买入信息（如果存在）
    pub buy_amount: Option<u64>,           // SOL买入金额
    pub minimum_token_out: Option<u64>,    // 最小代币输出
}

pub struct LetsbonkDetector {
    raydium_launchpad_program_id: Pubkey,
}

impl LetsbonkDetector {
    pub fn new() -> Result<Self> {
        let raydium_launchpad_program_id = Pubkey::from_str(RAYDIUM_LAUNCHPAD_PROGRAM_ID)?;
        
        Ok(Self {
            raydium_launchpad_program_id,
        })
    }

    /// 检测Raydium Launchpad代币创建事件
    pub async fn detect_bonk_token_creation(
        &self,
        txn_info: &SubscribeUpdateTransactionInfo,
        slot: u64,
    ) -> Option<TokenEvent> {
        let signature = if !txn_info.signature.is_empty() {
            bs58::encode(&txn_info.signature).into_string()
        } else {
            "unknown".to_string()
        };

        debug!("🔍 检查Raydium Launchpad交易: {}", signature);

        // 检查交易是否包含Raydium Launchpad程序
        let transaction = txn_info.transaction.as_ref()?;
        let message = transaction.message.as_ref()?;
        
        let account_keys: Vec<String> = message.account_keys.iter()
            .map(|key| bs58::encode(key).into_string())
            .collect();
        
        let has_raydium_launchpad = account_keys.iter().any(|key| key == RAYDIUM_LAUNCHPAD_PROGRAM_ID);
        
        if !has_raydium_launchpad {
            return None;
        }

        // 分析整个交易的所有指令
        if let Some(creation_info) = self.analyze_transaction_instructions(txn_info, slot).await {
            // 检查mint地址是否以"bonk"结尾（letsbonk池的特征）
            if creation_info.mint.to_lowercase().ends_with("bonk") {                
                // 构建包含创建和可能的买入信息的事件
                return Some(TokenEvent {
                    signature: creation_info.signature,
                    slot: creation_info.slot,
                    mint: Some(creation_info.mint),
                    transaction_type: TransactionType::TokenCreation,
                    detection_method: if creation_info.buy_amount.is_some() { 
                        "Raydium Launchpad letsbonk Filter (含买入)"
                    } else {
                        "Raydium Launchpad letsbonk Filter"
                    }.to_string(),
                    program_logs: self.extract_program_logs(txn_info),
                    account_keys: account_keys.clone(),
                    sol_amount: creation_info.buy_amount, // 如果有买入，记录买入金额
                    token_amount: creation_info.minimum_token_out, // 如果有买入，记录最小代币输出
                    creator_wallet: Some(account_keys.get(0).cloned().unwrap_or("unknown".to_string())), // 第一个账户通常是创建者
                    timestamp: Some(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() as i64),
                    raw_data: None,
                });
            }
        }

        // 检查买卖交易（对已存在的letsbonk池代币）
        if let Some(trade_event) = self.parse_trade_instructions(txn_info, slot).await {
            if let Some(ref mint) = trade_event.mint {
                if mint.to_lowercase().ends_with("bonk") {
                    return Some(trade_event);
                }
            }
        }

        None
    }


    /// 解析交易指令（买/卖）
    async fn parse_trade_instructions(
        &self,
        txn_info: &SubscribeUpdateTransactionInfo,
        slot: u64,
    ) -> Option<TokenEvent> {
        let signature = if !txn_info.signature.is_empty() {
            bs58::encode(&txn_info.signature).into_string()
        } else {
            "unknown".to_string()
        };

        let transaction = txn_info.transaction.as_ref()?;
        let message = transaction.message.as_ref()?;
        
        let account_keys: Vec<String> = message.account_keys.iter()
            .map(|key| bs58::encode(key).into_string())
            .collect();

        let account_pubkeys: Vec<Pubkey> = message.account_keys.iter()
            .filter_map(|key| {
                if key.len() == 32 {
                    Some(Pubkey::try_from(key.as_slice()).ok()?)
                } else {
                    None
                }
            })
            .collect();

        // 查找买卖交易指令
        for instruction in &message.instructions {
            if let Some(program_key) = account_pubkeys.get(instruction.program_id_index as usize) {
                if *program_key == self.raydium_launchpad_program_id {
                    if instruction.data.len() >= 8 {
                        let discriminator = &instruction.data[..8];
                        
                        #[cfg(feature = "letsbonk")]
                        {
                            debug!("尝试解析买卖指令，数据长度: {}, 判别器: {:?}", 
                                instruction.data.len(), discriminator);
                                
                            match RaydiumLaunchpadProgramIx::deserialize(&instruction.data) {
                                Ok(RaydiumLaunchpadProgramIx::BuyExactIn(args)) => {
                                    // 提取mint地址并检查是否符合bonk条件
                                    if let Some(mint) = self.extract_mint_from_accounts(&instruction.accounts, &account_keys) {
                                        if mint.to_lowercase().ends_with("bonk") {
                                            return Some(TokenEvent {
                                                signature: signature.clone(),
                                                slot,
                                                mint: Some(mint),
                                                transaction_type: TransactionType::Buy,
                                                detection_method: "Raydium Launchpad Buy".to_string(),
                                                program_logs: self.extract_program_logs(txn_info),
                                                account_keys: account_keys.clone(),
                                                sol_amount: Some(args.amount_in), // SOL输入金额
                                                token_amount: Some(args.minimum_amount_out), // 最小代币输出
                                                creator_wallet: None,
                                                timestamp: Some(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() as i64),
                                                raw_data: None,
                                            });
                                        } else {
                                            debug!("❌ mint地址不是bonk类型: {}", mint);
                                        }
                                    }
                                }
                                Ok(RaydiumLaunchpadProgramIx::BuyExactOut(args)) => {
                                    debug!("✅ 发现BuyExactOut指令: amount_out={}, maximum_amount_in={}", 
                                        args.amount_out, args.maximum_amount_in);
                                    
                                    // 提取mint地址并检查是否符合bonk条件
                                    if let Some(mint) = self.extract_mint_from_accounts(&instruction.accounts, &account_keys) {
                                        debug!("检查mint地址是否为bonk类型: {}", mint);
                                        if mint.to_lowercase().ends_with("bonk") {
                                            return Some(TokenEvent {
                                                signature: signature.clone(),
                                                slot,
                                                mint: Some(mint),
                                                transaction_type: TransactionType::Buy,
                                                detection_method: "Raydium Launchpad Buy Exact Out".to_string(),
                                                program_logs: self.extract_program_logs(txn_info),
                                                account_keys: account_keys.clone(),
                                                sol_amount: Some(args.maximum_amount_in), // 最大SOL输入
                                                token_amount: Some(args.amount_out), // 确切代币输出
                                                creator_wallet: None,
                                                timestamp: Some(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() as i64),
                                                raw_data: None,
                                            });
                                        } else {
                                            debug!("❌ mint地址不是bonk类型: {}", mint);
                                        }
                                    }
                                }
                                Ok(RaydiumLaunchpadProgramIx::SellExactIn(args)) => {
                                    debug!("✅ 发现SellExactIn指令: amount_in={}, minimum_amount_out={}", 
                                        args.amount_in, args.minimum_amount_out);
                                    
                                    // 提取mint地址并检查是否符合bonk条件
                                    if let Some(mint) = self.extract_mint_from_accounts(&instruction.accounts, &account_keys) {
                                        debug!("检查mint地址是否为bonk类型: {}", mint);
                                        if mint.to_lowercase().ends_with("bonk") {
                                            return Some(TokenEvent {
                                                signature: signature.clone(),
                                                slot,
                                                mint: Some(mint),
                                                transaction_type: TransactionType::Sell,
                                                detection_method: "Raydium Launchpad Sell".to_string(),
                                                program_logs: self.extract_program_logs(txn_info),
                                                account_keys: account_keys.clone(),
                                                sol_amount: Some(args.minimum_amount_out), // 最小SOL输出
                                                token_amount: Some(args.amount_in), // 代币输入金额
                                                creator_wallet: None,
                                                timestamp: Some(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() as i64),
                                                raw_data: None,
                                            });
                                        } else {
                                            debug!("❌ mint地址不是bonk类型: {}", mint);
                                        }
                                    }
                                }
                                Ok(RaydiumLaunchpadProgramIx::SellExactOut(args)) => {
                                    debug!("✅ 发现SellExactOut指令: amount_out={}, maximum_amount_in={}", 
                                        args.amount_out, args.maximum_amount_in);
                                    
                                    // 提取mint地址并检查是否符合bonk条件
                                    if let Some(mint) = self.extract_mint_from_accounts(&instruction.accounts, &account_keys) {
                                        debug!("检查mint地址是否为bonk类型: {}", mint);
                                        if mint.to_lowercase().ends_with("bonk") {
                                            return Some(TokenEvent {
                                                signature: signature.clone(),
                                                slot,
                                                mint: Some(mint),
                                                transaction_type: TransactionType::Sell,
                                                detection_method: "Raydium Launchpad Sell Exact Out".to_string(),
                                                program_logs: self.extract_program_logs(txn_info),
                                                account_keys: account_keys.clone(),
                                                sol_amount: Some(args.amount_out), // 确切SOL输出
                                                token_amount: Some(args.maximum_amount_in), // 最大代币输入
                                                creator_wallet: None,
                                                timestamp: Some(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() as i64),
                                                raw_data: None,
                                            });
                                        } else {
                                            debug!("❌ mint地址不是bonk类型: {}", mint);
                                        }
                                    }
                                }
                                Err(e) => {
                                    debug!("❌ 买卖指令解析失败: {:?}", e);
                                    debug!("指令数据 (前32字节): {}", hex::encode(&instruction.data[..std::cmp::min(32, instruction.data.len())]));
                                }
                                Ok(other) => {
                                    debug!("🔍 其他买卖指令类型: {:?}", other);
                                }
                            }
                        }
                    }
                }
            }
        }

        None
    }

    /// 从账户列表中提取mint地址
    fn extract_mint_from_accounts(&self, account_indices: &[u8], account_keys: &[String]) -> Option<String> {
        debug!("🔍 账户索引: {:?}", account_indices);
        debug!("🔍 可用账户 ({} 个):", account_keys.len());
        for (i, key) in account_keys.iter().enumerate() {
            debug!("   [{}]: {}", i, key);
        }
        
        // 对于 Raydium Launchpad，我们需要找到实际的 mint 地址
        // 通常交易的各个账户的含义：
        // [0]: 用户/交易发起者
        // [1]: 用户的相关账户 (ATA等)
        // [2]: 可能是mint地址，也可能是其他账户
        // 我们需要尝试多个位置找到正确的mint地址
        
        // 方法1: 查找以 mint 结尾的账户 (BONK 特征)
        for &index in account_indices.iter() {
            if let Some(account) = account_keys.get(index as usize) {
                debug!("检查账户索引 {}: {}", index, account);
                if account.to_lowercase().ends_with("bonk") {
                    debug!("✅ 找到 BONK mint 地址: {}", account);
                    return Some(account.clone());
                }
            }
        }
        
        // 方法2: 如果没找到明确的BONK地址，尝试找到看起来像mint的地址
        // Mint地址通常不是已知的程序地址
        let known_programs = [
            "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA", // Token Program
            "ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL", // Associated Token Program  
            "LanMV9sAd7wArD4vJFi2qDdfnVhFxYSUg6eADduJ3uj", // Raydium Launchpad Program
            "ComputeBudget111111111111111111111111111111", // Compute Budget Program
            "11111111111111111111111111111111", // System Program
            "So11111111111111111111111111111111111111112", // WSOL
            "metaqbxxUerdq28cj1RbAWkYQm3ybzjb6a8bt518x1s", // Metaplex Program
            "SysvarRent111111111111111111111111111111111", // Sysvar Rent
        ];
        
        for &index in account_indices.iter() {
            if let Some(account) = account_keys.get(index as usize) {
                if !known_programs.contains(&account.as_str()) {
                    debug!("💡 可能的mint地址候选: {}", account);
                    // 返回第一个不是已知程序的账户
                    return Some(account.clone());
                }
            }
        }
        
        // 方法3: 退化到使用第二个账户（原来的逻辑）
        if account_indices.len() > 1 {
            if let Some(mint_key) = account_keys.get(account_indices[1] as usize) {
                debug!("⚠️ 使用退化逻辑，返回第二个账户: {}", mint_key);
                return Some(mint_key.clone());
            }
        }
        
        debug!("❌ 无法提取mint地址");
        None
    }

    /// 提取程序日志
    fn extract_program_logs(&self, txn_info: &SubscribeUpdateTransactionInfo) -> Vec<String> {
        if let Some(meta) = &txn_info.meta {
            meta.log_messages.clone()
        } else {
            vec![]
        }
    }

    /// 分析整个交易的所有指令，提取创建和买入信息
    async fn analyze_transaction_instructions(
        &self,
        txn_info: &SubscribeUpdateTransactionInfo,
        slot: u64,
    ) -> Option<LetsbonkCreationWithBuyInfo> {
        let signature = if !txn_info.signature.is_empty() {
            bs58::encode(&txn_info.signature).into_string()
        } else {
            "unknown".to_string()
        };

        let transaction = txn_info.transaction.as_ref()?;
        let message = transaction.message.as_ref()?;
        
        let account_keys: Vec<Pubkey> = message.account_keys.iter()
            .filter_map(|key| {
                if key.len() == 32 {
                    Some(Pubkey::try_from(key.as_slice()).ok()?)
                } else {
                    None
                }
            })
            .collect();

        let _account_keys_str: Vec<String> = message.account_keys.iter()
            .map(|key| bs58::encode(key).into_string())
            .collect();

        // 存储找到的创建和买入信息
        let mut creation_info: Option<(String, String, String, String)> = None; // (mint, name, symbol, uri)
        let mut buy_info: Option<(u64, u64)> = None; // (amount_in, minimum_amount_out)

        debug!("🔍 分析交易中的 {} 条指令", message.instructions.len());

        // 分析所有指令
        for (i, instruction) in message.instructions.iter().enumerate() {
            if let Some(program_key) = account_keys.get(instruction.program_id_index as usize) {
                if *program_key == self.raydium_launchpad_program_id {
                    debug!("📝 指令 {}: Raydium Launchpad 指令，数据长度: {}", i, instruction.data.len());
                    
                    if instruction.data.len() >= 8 {
                        let _discriminator = &instruction.data[..8];
                        
                        #[cfg(feature = "letsbonk")]
                        {
                            match RaydiumLaunchpadProgramIx::deserialize(&instruction.data) {
                                Ok(RaydiumLaunchpadProgramIx::Initialize(args)) => {
                                    debug!("✅ 找到Initialize指令");
                                    // 提取mint地址（根据Raydium Launchpad IDL，mint地址在账户索引6）
                                    if let Some(&mint_index) = instruction.accounts.get(6) {
                                        if let Some(mint_pubkey) = account_keys.get(mint_index as usize) {
                                            debug!("🪙 mint地址: {}", mint_pubkey);
                                            creation_info = Some((
                                                mint_pubkey.to_string(),
                                                args.base_mint_param.name.clone(),
                                                args.base_mint_param.symbol.clone(),
                                                args.base_mint_param.uri.clone(),
                                            ));
                                        }
                                    }
                                }
                                Ok(RaydiumLaunchpadProgramIx::BuyExactIn(args)) => {
                                    debug!("💰 找到BuyExactIn指令: amount_in={}, minimum_amount_out={}", 
                                        args.amount_in, args.minimum_amount_out);
                                    buy_info = Some((args.amount_in, args.minimum_amount_out));
                                }
                                Ok(RaydiumLaunchpadProgramIx::BuyExactOut(args)) => {
                                    debug!("💰 找到BuyExactOut指令: amount_out={}, maximum_amount_in={}", 
                                        args.amount_out, args.maximum_amount_in);
                                    buy_info = Some((args.maximum_amount_in, args.amount_out));
                                }
                                Ok(other) => {
                                    debug!("🔍 其他指令类型: {:?}", other);
                                }
                                Err(e) => {
                                    debug!("❌ 指令解析失败: {:?}", e);
                                }
                            }
                        }
                    }
                }
            }
        }

        // 如果找到了创建信息，构建完整的创建事件
        if let Some((mint, name, symbol, uri)) = creation_info {
            debug!("✅ 构建创建事件信息，包含买入信息: {:?}", buy_info.is_some());
            Some(LetsbonkCreationWithBuyInfo {
                mint,
                slot,
                signature,
                token_name: Some(name),
                token_symbol: Some(symbol),
                token_uri: Some(uri),
                buy_amount: buy_info.map(|(amount_in, _)| amount_in),
                minimum_token_out: buy_info.map(|(_, min_out)| min_out),
            })
        } else {
            debug!("❌ 未找到Initialize指令");
            None
        }
    }
}

impl Default for LetsbonkDetector {
    fn default() -> Self {
        Self::new().expect("Failed to create LetsbonkDetector")
    }
}

/// 处理letsbonk交易的便利函数
pub async fn process_letsbonk_transaction(
    txn_info: &SubscribeUpdateTransactionInfo,
    slot: u64,
) -> Result<Option<TokenEvent>> {
    let detector = LetsbonkDetector::new()?;
    Ok(detector.detect_bonk_token_creation(txn_info, slot).await)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detector_creation() {
        let detector = LetsbonkDetector::new();
        assert!(detector.is_ok());
    }
}