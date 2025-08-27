use anyhow::Result;
use log::{debug, info, warn, error};
use serde::Serialize;
use solana_sdk::{
    instruction::{AccountMeta, CompiledInstruction, Instruction},
    message::{v0::LoadedAddresses, VersionedMessage},
    pubkey::Pubkey,
    signature::Signature,
    transaction::VersionedTransaction,
};
use solana_transaction_status::{
    VersionedTransactionWithStatusMeta, InnerInstructions, InnerInstruction,
    TransactionStatusMeta as SolanaTransactionStatusMeta
};
use std::{fs, str::FromStr};
use yellowstone_grpc_proto::geyser::SubscribeUpdateTransactionInfo;
use yellowstone_grpc_proto::prelude::TransactionStatusMeta;

use crate::processors::{instruction_account_mapper::{AccountMetadata, Idl, InstructionAccountMapper}, TokenEvent, TransactionType};
use crate::serialization::serialize_pubkey;

// Program IDs
const PUMPFUN_PROGRAM_ID: &str = "6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P";
const TOKEN_PROGRAM_ID: &str = "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA";

#[derive(Debug, Serialize)]
pub struct TransactionInstructionWithParent {
    pub instruction: Instruction,
    pub parent_program_id: Option<Pubkey>,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct DecodedInstruction {
    pub name: String,
    pub accounts: Vec<AccountMetadata>,
    pub data: serde_json::Value,
    #[serde(serialize_with = "serialize_pubkey")]
    pub program_id: Pubkey,
    #[serde(serialize_with = "crate::serialization::serialize_option_pubkey")]
    pub parent_program_id: Option<Pubkey>,
}

#[derive(Debug, Clone)]
pub struct IdlTransactionProcessor {
    pub pumpfun_program_id: Pubkey,
    pub token_program_id: Pubkey,
    pub pumpfun_idl: Option<Idl>,
    pub token_idl: Option<Idl>,
}

impl IdlTransactionProcessor {
    pub fn new() -> Result<Self> {
        let mut processor = Self {
            pumpfun_program_id: Pubkey::from_str(PUMPFUN_PROGRAM_ID)?,
            token_program_id: Pubkey::from_str(TOKEN_PROGRAM_ID)?,
            pumpfun_idl: None,
            token_idl: None,
        };

        // 加载IDL文件
        processor.load_idls()?;
        
        Ok(processor)
    }

    fn load_idls(&mut self) -> Result<()> {
        // 加载PumpFun IDL
        if let Ok(idl_content) = fs::read_to_string("idls/pumpfun_0.1.0.json") {
            match serde_json::from_str::<Idl>(&idl_content) {
                Ok(idl) => {
                    info!("✅ 成功加载PumpFun IDL");
                    self.pumpfun_idl = Some(idl);
                }
                Err(e) => {
                    error!("解析PumpFun IDL失败: {}", e);
                }
            }
        } else {
            info!("⚠️  未找到PumpFun IDL文件，将使用基础解析");
        }

        // 加载Token程序IDL
        if let Ok(idl_content) = fs::read_to_string("idls/token_program_idl.json") {
            match serde_json::from_str::<Idl>(&idl_content) {
                Ok(idl) => {
                    info!("✅ 成功加载Token程序IDL");
                    self.token_idl = Some(idl);
                }
                Err(e) => {
                    error!("解析Token程序IDL失败: {}", e);
                }
            }
        } else {
            info!("⚠️  未找到Token程序IDL文件，将使用基础解析");
        }

        Ok(())
    }

    /// 使用IDL解析交易
    pub async fn process_transaction_with_idl(
        &self,
        txn_info: &SubscribeUpdateTransactionInfo,
        slot: u64,
    ) -> Option<TokenEvent> {
        let signature = if !txn_info.signature.is_empty() {
            bs58::encode(&txn_info.signature).into_string()
        } else {
            "unknown".to_string()
        };

        debug!("开始IDL解析交易: {}", signature);

        // 构建完整的交易结构用于解析
        if let Some(parsed_transaction) = self.build_parsed_transaction(txn_info, slot).await {
            // 检查是否包含代币创建
            if let Some(token_event) = self.detect_token_creation(&parsed_transaction, &signature, slot) {
                return Some(token_event);
            }

            // 检查是否包含买卖交易
            if let Some(token_event) = self.detect_buy_sell_transactions(&parsed_transaction, &signature, slot) {
                return Some(token_event);
            }
        }

        None
    }

    /// 构建解析后的交易结构
    async fn build_parsed_transaction(
        &self,
        txn_info: &SubscribeUpdateTransactionInfo,
        slot: u64,
    ) -> Option<ParsedConfirmedTransaction> {
        let transaction = txn_info.transaction.as_ref()?;
        let _message = transaction.message.as_ref()?;
        let meta = txn_info.meta.as_ref()?;

        // 构建VersionedTransactionWithStatusMeta用于指令展开
        let versioned_tx = self.build_versioned_transaction(txn_info)?;

        // 展开编译指令和内部指令
        let compiled_instructions = self.flatten_compiled_instructions(&versioned_tx);
        let inner_instructions = self.flatten_inner_instructions(&versioned_tx);

        // 使用IDL解码指令
        let mut decoded_compiled_instructions = Vec::new();
        let mut decoded_inner_instructions = Vec::new();

        // 解码编译指令
        for instruction_with_parent in compiled_instructions {
            if let Some(decoded) = self.decode_instruction(&instruction_with_parent) {
                decoded_compiled_instructions.push(decoded);
            }
        }

        // 解码内部指令
        for instruction_with_parent in inner_instructions {
            if let Some(decoded) = self.decode_instruction(&instruction_with_parent) {
                decoded_inner_instructions.push(decoded);
            }
        }

        Some(ParsedConfirmedTransaction {
            slot,
            signature: bs58::encode(&txn_info.signature).into_string(),
            compiled_instructions: decoded_compiled_instructions,
            inner_instructions: decoded_inner_instructions,
            meta: meta.clone(),
        })
    }

    /// 构建VersionedTransactionWithStatusMeta - 改进版本
    fn build_versioned_transaction(
        &self,
        txn_info: &SubscribeUpdateTransactionInfo,
    ) -> Option<VersionedTransactionWithStatusMeta> {
        let transaction = txn_info.transaction.as_ref()?;
        let message = transaction.message.as_ref()?;
        let meta = txn_info.meta.as_ref()?;
        let header = message.header.as_ref()?;

        // 构建签名
        let signature_bytes: [u8; 64] = txn_info.signature.clone().try_into().ok()?;
        let signature = Signature::from(signature_bytes);

        // 构建账户密钥
        let static_account_keys: Vec<Pubkey> = message.account_keys.iter()
            .map(|key| Pubkey::new_from_array(key.clone().try_into().unwrap_or_default()))
            .collect();

        // 添加从地址表加载的账户
        let mut all_account_keys = static_account_keys.clone();
        all_account_keys.extend(meta.loaded_writable_addresses.iter()
            .map(|key| Pubkey::new_from_array(key.clone().try_into().unwrap_or_default())));
        all_account_keys.extend(meta.loaded_readonly_addresses.iter()
            .map(|key| Pubkey::new_from_array(key.clone().try_into().unwrap_or_default())));

        debug!("Transaction accounts: static={}, loaded_writable={}, loaded_readonly={}, total={}", 
               static_account_keys.len(),
               meta.loaded_writable_addresses.len(), 
               meta.loaded_readonly_addresses.len(),
               all_account_keys.len());

        // 构建指令
        let instructions: Vec<CompiledInstruction> = message.instructions.iter()
            .map(|ix| CompiledInstruction {
                program_id_index: ix.program_id_index as u8,
                accounts: ix.accounts.clone(),
                data: ix.data.clone(),
            })
            .collect();

        // 构建VersionedMessage
        let versioned_message = VersionedMessage::V0(solana_sdk::message::v0::Message {
            header: solana_sdk::message::MessageHeader {
                num_required_signatures: header.num_required_signatures as u8,
                num_readonly_signed_accounts: header.num_readonly_signed_accounts as u8,
                num_readonly_unsigned_accounts: header.num_readonly_unsigned_accounts as u8,
            },
            account_keys: static_account_keys,
            recent_blockhash: solana_sdk::hash::Hash::new_from_array(
                message.recent_blockhash.clone().try_into().unwrap_or_default()
            ),
            instructions,
            address_table_lookups: message.address_table_lookups.iter().map(|lookup| {
                solana_sdk::message::v0::MessageAddressTableLookup {
                    account_key: Pubkey::new_from_array(lookup.account_key.clone().try_into().unwrap_or_default()),
                    writable_indexes: lookup.writable_indexes.clone(),
                    readonly_indexes: lookup.readonly_indexes.clone(),
                }
            }).collect(),
        });

        // 构建TransactionStatusMeta
        let transaction_meta = SolanaTransactionStatusMeta {
            status: Ok(()),
            fee: meta.fee,
            pre_balances: meta.pre_balances.clone(),
            post_balances: meta.post_balances.clone(),
            inner_instructions: Some(
                meta.inner_instructions.iter().map(|inner| {
                    InnerInstructions {
                        index: inner.index as u8,
                        instructions: inner.instructions.iter().map(|ix| {
                            InnerInstruction {
                                instruction: CompiledInstruction {
                                    program_id_index: ix.program_id_index as u8,
                                    accounts: ix.accounts.clone(),
                                    data: ix.data.clone(),
                                },
                                stack_height: ix.stack_height,
                            }
                        }).collect(),
                    }
                }).collect()
            ),
            log_messages: Some(meta.log_messages.clone()),
            pre_token_balances: None,  // 简化处理
            post_token_balances: None, // 简化处理
            rewards: None,             // 简化处理
            loaded_addresses: LoadedAddresses {
                writable: meta.loaded_writable_addresses.iter()
                    .map(|addr| Pubkey::new_from_array(addr.clone().try_into().unwrap_or_default()))
                    .collect(),
                readonly: meta.loaded_readonly_addresses.iter()
                    .map(|addr| Pubkey::new_from_array(addr.clone().try_into().unwrap_or_default()))
                    .collect(),
            },
            return_data: None,
            compute_units_consumed: meta.compute_units_consumed,
        };

        Some(VersionedTransactionWithStatusMeta {
            transaction: VersionedTransaction {
                signatures: vec![signature],
                message: versioned_message,
            },
            meta: transaction_meta,
        })
    }

    /// 使用IDL解码指令
    fn decode_instruction(&self, instruction_with_parent: &TransactionInstructionWithParent) -> Option<DecodedInstruction> {
        let instruction = &instruction_with_parent.instruction;

        // PumpFun程序指令解析
        if instruction.program_id == self.pumpfun_program_id {
            if let Some(ref idl) = self.pumpfun_idl {
                return self.decode_pumpfun_instruction(instruction, instruction_with_parent.parent_program_id, idl);
            }
        }

        // Token程序指令解析
        if instruction.program_id == self.token_program_id {
            if let Some(ref idl) = self.token_idl {
                return self.decode_token_instruction(instruction, instruction_with_parent.parent_program_id, idl);
            }
        }

        None
    }

    /// 解码PumpFun指令 - 改进版本
    fn decode_pumpfun_instruction(
        &self,
        instruction: &Instruction,
        parent_program_id: Option<Pubkey>,
        idl: &Idl,
    ) -> Option<DecodedInstruction> {
        // PumpFun指令判别器（8字节）
        if instruction.data.len() < 8 {
            warn!("PumpFun instruction data too short: {} bytes", instruction.data.len());
            return None;
        }

        let discriminator = &instruction.data[0..8];
        
        // 使用IDL中的正确判别器进行匹配
        let instruction_name = match discriminator {
            // 从IDL文件中获取的完整判别器列表
            [102, 6, 61, 18, 1, 218, 235, 234] => "buy",
            [20, 22, 86, 123, 198, 28, 219, 132] => "collect_creator_fee",  
            [24, 30, 200, 40, 5, 28, 7, 119] => "create",
            [234, 102, 194, 203, 150, 72, 62, 229] => "extend_account",
            [175, 175, 109, 31, 13, 152, 155, 237] => "initialize",
            [155, 234, 231, 146, 236, 158, 162, 30] => "migrate",
            [51, 230, 133, 164, 1, 127, 131, 173] => "sell",
            [254, 148, 255, 112, 207, 142, 170, 165] => "set_creator",
            [138, 96, 174, 217, 48, 85, 197, 246] => "set_metaplex_creator",
            [27, 234, 178, 52, 147, 2, 187, 141] => "set_params",
            [227, 181, 74, 196, 208, 21, 97, 213] => "update_global_authority",
            _ => {
                // 打印未知判别器的详细信息用于调试
                debug!("Unknown PumpFun instruction discriminator: {:?} (hex: {})", 
                     discriminator, 
                     hex::encode(discriminator));
                debug!("This might be a different program's instruction or a new PumpFun instruction");
                debug!("Instruction data length: {} bytes", instruction.data.len());
                debug!("Full instruction data (first 32 bytes): {}", 
                     hex::encode(&instruction.data[..std::cmp::min(32, instruction.data.len())]));
                return None; // 直接返回None而不是使用"unknown"
            }
        };

        debug!("Decoded PumpFun instruction: {}", instruction_name);

        // 使用IDL映射账户
        match idl.map_accounts(&instruction.accounts, instruction_name) {
            Ok(mapped_accounts) => {
                // 解析指令数据
                let instruction_data = match instruction_name {
                    "buy" => {
                        if instruction.data.len() >= 24 {
                            // buy指令数据：8字节判别器 + 8字节amount + 8字节max_sol_cost
                            let amount = u64::from_le_bytes(
                                instruction.data[8..16].try_into().unwrap_or([0; 8])
                            );
                            let max_sol_cost = u64::from_le_bytes(
                                instruction.data[16..24].try_into().unwrap_or([0; 8])
                            );
                            serde_json::json!({
                                "instruction": instruction_name,
                                "amount": amount,
                                "max_sol_cost": max_sol_cost
                            })
                        } else {
                            serde_json::json!({
                                "instruction": instruction_name,
                                "raw_data": hex::encode(&instruction.data)
                            })
                        }
                    }
                    "sell" => {
                        if instruction.data.len() >= 24 {
                            // sell指令数据：8字节判别器 + 8字节amount + 8字节min_sol_output
                            let amount = u64::from_le_bytes(
                                instruction.data[8..16].try_into().unwrap_or([0; 8])
                            );
                            let min_sol_output = u64::from_le_bytes(
                                instruction.data[16..24].try_into().unwrap_or([0; 8])
                            );
                            serde_json::json!({
                                "instruction": instruction_name,
                                "amount": amount,
                                "min_sol_output": min_sol_output
                            })
                        } else {
                            serde_json::json!({
                                "instruction": instruction_name,
                                "raw_data": hex::encode(&instruction.data)
                            })
                        }
                    }
                    "create" => {
                        // create指令包含代币名称、符号、URI等信息
                        serde_json::json!({
                            "instruction": instruction_name,
                            "raw_data": hex::encode(&instruction.data),
                            "description": "Token creation instruction"
                        })
                    }
                    "collect_creator_fee" => {
                        serde_json::json!({
                            "instruction": instruction_name,
                            "raw_data": hex::encode(&instruction.data),
                            "description": "Collect creator fees"
                        })
                    }
                    "migrate" => {
                        serde_json::json!({
                            "instruction": instruction_name,
                            "raw_data": hex::encode(&instruction.data),
                            "description": "Migrate to PumpAMM"
                        })
                    }
                    _ => {
                        // 其他指令的通用处理
                        serde_json::json!({
                            "instruction": instruction_name,
                            "discriminator": hex::encode(discriminator),
                            "raw_data": hex::encode(&instruction.data),
                            "description": format!("PumpFun {} instruction", instruction_name)
                        })
                    }
                };

                Some(DecodedInstruction {
                    name: instruction_name.to_string(),
                    accounts: mapped_accounts,
                    data: instruction_data,
                    program_id: instruction.program_id,
                    parent_program_id,
                })
            }
            Err(err) => {
                error!("Failed to map accounts for PumpFun instruction '{}': {:?}", instruction_name, err);
                error!("Instruction has {} accounts", instruction.accounts.len());
                for (i, account) in instruction.accounts.iter().enumerate() {
                    debug!("  Account[{}]: {} (signer: {}, writable: {})", 
                           i, account.pubkey, account.is_signer, account.is_writable);
                }
                None
            }
        }
    }

    /// 解码Token程序指令
    fn decode_token_instruction(
        &self,
        instruction: &Instruction,
        parent_program_id: Option<Pubkey>,
        idl: &Idl,
    ) -> Option<DecodedInstruction> {
        // 使用spl-token库解析指令
        if let Ok(token_instruction) = spl_token::instruction::TokenInstruction::unpack(&instruction.data) {
            let instruction_name = self.get_token_instruction_name(&token_instruction);
            
            if let Ok(mapped_accounts) = idl.map_accounts(&instruction.accounts, &instruction_name) {
                return Some(DecodedInstruction {
                    name: instruction_name,
                    accounts: mapped_accounts,
                    data: serde_json::json!({
                        "instruction_type": format!("{:?}", token_instruction),
                        "raw_data": bs58::encode(&instruction.data).into_string()
                    }),
                    program_id: instruction.program_id,
                    parent_program_id,
                });
            }
        }

        None
    }

    /// 获取Token指令名称
    fn get_token_instruction_name(&self, instruction: &spl_token::instruction::TokenInstruction) -> String {
        match instruction {
            spl_token::instruction::TokenInstruction::InitializeMint { .. } => "initializeMint".to_string(),
            spl_token::instruction::TokenInstruction::InitializeMint2 { .. } => "initializeMint2".to_string(),
            spl_token::instruction::TokenInstruction::InitializeAccount => "initializeAccount".to_string(),
            spl_token::instruction::TokenInstruction::InitializeAccount2 { .. } => "initializeAccount2".to_string(),
            spl_token::instruction::TokenInstruction::InitializeAccount3 { .. } => "initializeAccount3".to_string(),
            spl_token::instruction::TokenInstruction::Transfer { .. } => "transfer".to_string(),
            spl_token::instruction::TokenInstruction::Approve { .. } => "approve".to_string(),
            spl_token::instruction::TokenInstruction::Revoke => "revoke".to_string(),
            spl_token::instruction::TokenInstruction::SetAuthority { .. } => "setAuthority".to_string(),
            spl_token::instruction::TokenInstruction::MintTo { .. } => "mintTo".to_string(),
            spl_token::instruction::TokenInstruction::Burn { .. } => "burn".to_string(),
            spl_token::instruction::TokenInstruction::CloseAccount => "closeAccount".to_string(),
            spl_token::instruction::TokenInstruction::FreezeAccount => "freezeAccount".to_string(),
            spl_token::instruction::TokenInstruction::ThawAccount => "thawAccount".to_string(),
            spl_token::instruction::TokenInstruction::TransferChecked { .. } => "transferChecked".to_string(),
            spl_token::instruction::TokenInstruction::ApproveChecked { .. } => "approveChecked".to_string(),
            spl_token::instruction::TokenInstruction::MintToChecked { .. } => "mintToChecked".to_string(),
            spl_token::instruction::TokenInstruction::BurnChecked { .. } => "burnChecked".to_string(),
            spl_token::instruction::TokenInstruction::SyncNative => "syncNative".to_string(),
            _ => "unknown".to_string(),
        }
    }

    /// 检测代币创建事件 - 简化版本
    fn detect_token_creation(&self, parsed_tx: &ParsedConfirmedTransaction, signature: &str, slot: u64) -> Option<TokenEvent> {
        // 查找initializeMint2指令 - 这是PumpFun代币创建的关键指令
        let has_mint_init = parsed_tx.compiled_instructions.iter()
            .chain(parsed_tx.inner_instructions.iter())
            .any(|instr| instr.name == "initializeMint2");

        // 同时检查是否包含PumpFun程序的调用
        let has_pumpfun_call = parsed_tx.compiled_instructions.iter()
            .any(|instr| instr.program_id == self.pumpfun_program_id);

        if has_mint_init && has_pumpfun_call {
            debug!("🚀 检测到PumpFun代币创建");
            
            // 提取mint地址和基本信息
            let mint = self.extract_mint_from_instructions(&parsed_tx.compiled_instructions, &parsed_tx.inner_instructions);
            let (sol_amount, token_amount) = self.extract_token_creation_amounts(parsed_tx);
            
            let creator_wallet = self.extract_pumpfun_creator(parsed_tx);
            debug!("🔍 PumpFun创建交易 - 提取的创建者钱包: {:?}", creator_wallet);
            debug!("🔍 PumpFun创建交易 - 提取的代币地址: {:?}", mint);
            let timestamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() as i64;
            let raw_data = self.build_raw_event_data(parsed_tx, signature, slot);
            
            return Some(TokenEvent {
                signature: signature.to_string(),
                slot,
                mint,
                transaction_type: TransactionType::TokenCreation,
                detection_method: "pumpfun_create".to_string(),
                program_logs: parsed_tx.meta.log_messages.clone(),
                account_keys: self.extract_account_keys(&parsed_tx.compiled_instructions),
                sol_amount,
                token_amount,
                creator_wallet,
                timestamp: Some(timestamp),
                raw_data: Some(raw_data),
            });
        }

        None
    }

    /// 提取代币创建相关的金额信息
    fn extract_token_creation_amounts(&self, parsed_tx: &ParsedConfirmedTransaction) -> (Option<u64>, Option<u64>) {
        // 从交易的余额变化中推断SOL和代币数量
        let pre_balances = &parsed_tx.meta.pre_balances;
        let post_balances = &parsed_tx.meta.post_balances;
        
        if pre_balances.len() == post_balances.len() && pre_balances.len() > 0 {
            // 计算第一个账户的余额变化（通常是发起者账户）
            let balance_diff = if post_balances[0] < pre_balances[0] {
                Some(pre_balances[0] - post_balances[0])
            } else {
                None
            };
            
            return (balance_diff, Some(1000000000)); // PumpFun通常创建10亿代币
        }
        
        (None, None)
    }

    /// 提取账户密钥
    fn extract_account_keys(&self, instructions: &[DecodedInstruction]) -> Vec<String> {
        let mut account_keys = Vec::new();
        for instruction in instructions {
            for account in &instruction.accounts {
                account_keys.push(account.pubkey.to_string());
            }
        }
        account_keys.sort();
        account_keys.dedup();
        account_keys
    }

    /// 提取创建者/交易者钱包地址 (通用方法，保留作为后备)
    fn extract_creator_wallet(&self, parsed_tx: &ParsedConfirmedTransaction) -> Option<String> {
        // 通用的创建者地址提取逻辑（后备方案）
        for instruction in &parsed_tx.compiled_instructions {
            for account in &instruction.accounts {
                if account.is_signer {
                    return Some(account.pubkey.to_string());
                }
            }
        }
        None
    }

    /// 提取PumpFun代币创建交易中的创建者钱包地址
    fn extract_pumpfun_creator(&self, parsed_tx: &ParsedConfirmedTransaction) -> Option<String> {
        debug!("🔍 extract_pumpfun_creator: 开始提取创建者地址");
        
        // 在PumpFun创建交易中，创建者是交易级别的签名者（fee payer）
        // 不是某个特定指令的账户，而是所有指令中标记为 is_signer: true 且不是 mint 的账户
        
        // 检查是否有 initializeMint2 指令（代币创建的标志）
        let has_mint_init = parsed_tx.compiled_instructions.iter()
            .chain(parsed_tx.inner_instructions.iter())
            .any(|instr| instr.name == "initializeMint2");
            
        debug!("🔍 extract_pumpfun_creator: has_mint_init = {}", has_mint_init);
            
        if !has_mint_init {
            debug!("🔍 extract_pumpfun_creator: 没有initializeMint2指令，回退到通用逻辑");
            // 如果不是代币创建交易，回退到通用逻辑
            return self.extract_creator_wallet(parsed_tx);
        }
        
        // 首先获取mint地址
        let mint_address = self.extract_mint_from_instructions(&parsed_tx.compiled_instructions, &parsed_tx.inner_instructions);
        debug!("🔍 extract_pumpfun_creator: mint地址 = {:?}", mint_address);
        
        // 从所有指令中找到签名者，但排除mint地址本身
        for (idx, instruction) in parsed_tx.compiled_instructions.iter().enumerate() {
            debug!("🔍 extract_pumpfun_creator: 检查指令 {} - {}", idx, instruction.name);
            for (acc_idx, account) in instruction.accounts.iter().enumerate() {
                debug!("🔍 extract_pumpfun_creator: 账户 {} - {} (is_signer: {})", acc_idx, account.pubkey, account.is_signer);
                
                if account.is_signer {
                    let account_addr = account.pubkey.to_string();
                    
                    // 检查这个签名者是否是mint地址，如果是则跳过
                    if let Some(ref mint_addr) = mint_address {
                        if account_addr == *mint_addr {
                            debug!("🔍 extract_pumpfun_creator: 跳过mint地址签名者: {}", account_addr);
                            continue;
                        }
                    }
                    
                    debug!("🔍 extract_pumpfun_creator: 找到真实创建者: {}", account_addr);
                    return Some(account_addr);
                }
            }
        }
        
        debug!("🔍 extract_pumpfun_creator: 没有找到非mint签名者，返回None");
        None
    }

    /// 提取PumpFun买卖交易中的交易者钱包地址  
    fn extract_pumpfun_trader(&self, parsed_tx: &ParsedConfirmedTransaction) -> Option<String> {
        // 在PumpFun买卖交易中，交易者是buy/sell指令的签名者
        for instruction in &parsed_tx.compiled_instructions {
            if instruction.program_id == self.pumpfun_program_id && 
               (instruction.name == "buy" || instruction.name == "sell") {
                // 在买卖指令中查找签名者（交易者）
                for account in &instruction.accounts {
                    if account.is_signer {
                        return Some(account.pubkey.to_string());
                    }
                }
            }
        }
        
        // 后备方案
        self.extract_creator_wallet(parsed_tx)
    }

    /// 构建完整的原始数据用于JSON记录
    fn build_raw_event_data(&self, parsed_tx: &ParsedConfirmedTransaction, signature: &str, slot: u64) -> serde_json::Value {
        serde_json::json!({
            "signature": signature,
            "slot": slot,
            "timestamp": std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() as i64,
            "compiled_instructions_count": parsed_tx.compiled_instructions.len(),
            "inner_instructions_count": parsed_tx.inner_instructions.len(),
            "meta": {
                "log_messages": parsed_tx.meta.log_messages,
                "pre_balances": parsed_tx.meta.pre_balances,
                "post_balances": parsed_tx.meta.post_balances,
                "fee": parsed_tx.meta.fee
            }
        })
    }

    /// 检测买卖交易 - 简化版本，只关注核心交易类型
    fn detect_buy_sell_transactions(&self, parsed_tx: &ParsedConfirmedTransaction, signature: &str, slot: u64) -> Option<TokenEvent> {
        // 只查找编译指令中的buy/sell，忽略其他指令类型
        for instruction in &parsed_tx.compiled_instructions {
            if instruction.program_id == self.pumpfun_program_id {
                match instruction.name.as_str() {
                    "buy" => {
                        debug!("💰 检测到BUY交易");
                        let mint = self.extract_mint_from_accounts(&instruction.accounts);
                        let (sol_amount, token_amount) = self.extract_buy_sell_amounts(&instruction.data, true);
                        
                        let creator_wallet = self.extract_pumpfun_trader(parsed_tx);
                        let timestamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() as i64;
                        let raw_data = self.build_raw_event_data(parsed_tx, signature, slot);
                        
                        return Some(TokenEvent {
                            signature: signature.to_string(),
                            slot,
                            mint,
                            transaction_type: TransactionType::Buy,
                            detection_method: "pumpfun_buy".to_string(),
                            program_logs: parsed_tx.meta.log_messages.clone(),
                            account_keys: self.extract_account_keys(&parsed_tx.compiled_instructions),
                            sol_amount,
                            token_amount,
                            creator_wallet,
                            timestamp: Some(timestamp),
                            raw_data: Some(raw_data),
                        });
                    }
                    "sell" => {
                        debug!("💸 检测到SELL交易");
                        let mint = self.extract_mint_from_accounts(&instruction.accounts);
                        let (sol_amount, token_amount) = self.extract_buy_sell_amounts(&instruction.data, false);
                        
                        let creator_wallet = self.extract_pumpfun_trader(parsed_tx);
                        let timestamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() as i64;
                        let raw_data = self.build_raw_event_data(parsed_tx, signature, slot);
                        
                        return Some(TokenEvent {
                            signature: signature.to_string(),
                            slot,
                            mint,
                            transaction_type: TransactionType::Sell,
                            detection_method: "pumpfun_sell".to_string(),
                            program_logs: parsed_tx.meta.log_messages.clone(),
                            account_keys: self.extract_account_keys(&parsed_tx.compiled_instructions),
                            sol_amount,
                            token_amount,
                            creator_wallet,
                            timestamp: Some(timestamp),
                            raw_data: Some(raw_data),
                        });
                    }
                    _ => {
                        // 静默忽略其他PumpFun指令类型
                    }
                }
            }
        }

        None
    }

    /// 从指令数据中提取买卖金额
    fn extract_buy_sell_amounts(&self, data: &serde_json::Value, is_buy: bool) -> (Option<u64>, Option<u64>) {
        if is_buy {
            // buy指令：返回max_sol_cost和amount
            if let (Some(amount), Some(max_sol_cost)) = (
                data.get("amount").and_then(|v| v.as_u64()),
                data.get("max_sol_cost").and_then(|v| v.as_u64())
            ) {
                return (Some(max_sol_cost), Some(amount));
            }
        } else {
            // sell指令：返回min_sol_output和amount
            if let (Some(amount), Some(min_sol_output)) = (
                data.get("amount").and_then(|v| v.as_u64()),
                data.get("min_sol_output").and_then(|v| v.as_u64())
            ) {
                return (Some(min_sol_output), Some(amount));
            }
        }
        
        (None, None)
    }

    /// 从指令中提取mint地址
    fn extract_mint_from_instructions(&self, compiled: &[DecodedInstruction], inner: &[DecodedInstruction]) -> Option<String> {
        debug!("🔍 extract_mint_from_instructions: 开始提取mint地址");
        
        for (idx, instruction) in compiled.iter().chain(inner.iter()).enumerate() {
            debug!("🔍 extract_mint_from_instructions: 检查指令 {} - {}", idx, instruction.name);
            
            if instruction.name == "initializeMint2" || instruction.name == "initializeMint" {
                debug!("🔍 extract_mint_from_instructions: 找到 {} 指令", instruction.name);
                debug!("🔍 extract_mint_from_instructions: 指令有 {} 个账户", instruction.accounts.len());
                
                for (acc_idx, account) in instruction.accounts.iter().enumerate() {
                    debug!("🔍 extract_mint_from_instructions: 账户 {} - {} (name: {})", acc_idx, account.pubkey, account.name);
                }
                
                // mint通常是第一个账户
                if let Some(account) = instruction.accounts.first() {
                    let mint_addr = account.pubkey.to_string();
                    debug!("🔍 extract_mint_from_instructions: 返回mint地址: {}", mint_addr);
                    return Some(mint_addr);
                }
            }
        }
        
        debug!("🔍 extract_mint_from_instructions: 没有找到mint地址");
        None
    }

    /// 从账户中提取mint地址
    fn extract_mint_from_accounts(&self, accounts: &[AccountMetadata]) -> Option<String> {
        // 查找名为"mint"的账户
        for account in accounts {
            if account.name.to_lowercase().contains("mint") {
                return Some(account.pubkey.to_string());
            }
        }
        
        // 如果没找到，返回第一个非程序账户
        for account in accounts {
            let addr_str = account.pubkey.to_string();
            if addr_str != PUMPFUN_PROGRAM_ID && addr_str != TOKEN_PROGRAM_ID {
                return Some(addr_str);
            }
        }
        
        None
    }

    /// 展开编译指令
    fn flatten_compiled_instructions(&self, tx_with_meta: &VersionedTransactionWithStatusMeta) -> Vec<TransactionInstructionWithParent> {
        let mut result = Vec::new();
        let instructions = tx_with_meta.transaction.message.instructions();
        
        // 从交易元数据中获取加载的地址
        let loaded_addresses = LoadedAddresses {
            writable: tx_with_meta.meta.loaded_addresses.writable.clone(),
            readonly: tx_with_meta.meta.loaded_addresses.readonly.clone(),
        };

        let parsed_accounts = self.parse_transaction_accounts(&tx_with_meta.transaction.message, loaded_addresses);

        debug!("Transaction has {} instructions, {} total accounts", instructions.len(), parsed_accounts.len());

        for (idx, instruction) in instructions.iter().enumerate() {
            debug!("Processing instruction {}: program_id_index={}, accounts={:?}", 
                   idx, instruction.program_id_index, instruction.accounts);
            
            result.push(TransactionInstructionWithParent {
                instruction: self.compiled_instruction_to_instruction(&instruction, &parsed_accounts),
                parent_program_id: None,
            });
        }

        result
    }

    /// 展开内部指令
    fn flatten_inner_instructions(&self, tx_with_meta: &VersionedTransactionWithStatusMeta) -> Vec<TransactionInstructionWithParent> {
        let mut result = Vec::new();
        let instructions = tx_with_meta.transaction.message.instructions();
        
        // 从交易元数据中获取加载的地址
        let loaded_addresses = LoadedAddresses {
            writable: tx_with_meta.meta.loaded_addresses.writable.clone(),
            readonly: tx_with_meta.meta.loaded_addresses.readonly.clone(),
        };

        let parsed_accounts = self.parse_transaction_accounts(&tx_with_meta.transaction.message, loaded_addresses);

        if let Some(inner_instructions) = &tx_with_meta.meta.inner_instructions {
            for inner_ix in inner_instructions {
                // 安全检查：确保索引在范围内
                if (inner_ix.index as usize) >= instructions.len() {
                    warn!("Inner instruction index {} out of bounds (total instructions: {})", inner_ix.index, instructions.len());
                    continue;
                }

                let parent_instruction = &instructions[inner_ix.index as usize];
                let parent_program_id = if (parent_instruction.program_id_index as usize) < parsed_accounts.len() {
                    parsed_accounts[parent_instruction.program_id_index as usize].pubkey
                } else {
                    warn!("Parent program ID index {} out of bounds", parent_instruction.program_id_index);
                    Pubkey::default()
                };
                
                for inner_instruction in &inner_ix.instructions {
                    result.push(TransactionInstructionWithParent {
                        instruction: self.compiled_instruction_to_instruction(&inner_instruction.instruction, &parsed_accounts),
                        parent_program_id: Some(parent_program_id),
                    });
                }
            }
        }

        result
    }

    /// 将编译指令转换为指令（带安全检查）
    fn compiled_instruction_to_instruction(&self, ci: &CompiledInstruction, parsed_accounts: &[AccountMeta]) -> Instruction {
        // 安全检查：确保程序ID索引在范围内
        let program_id = if (ci.program_id_index as usize) < parsed_accounts.len() {
            parsed_accounts[ci.program_id_index as usize].pubkey
        } else {
            warn!("Program ID index {} out of bounds (total accounts: {})", ci.program_id_index, parsed_accounts.len());
            return Instruction {
                program_id: Pubkey::default(),
                accounts: vec![],
                data: ci.data.clone(),
            };
        };

        // 安全检查：确保所有账户索引都在范围内
        let accounts: Vec<AccountMeta> = ci.accounts.iter()
            .filter_map(|&index| {
                if (index as usize) < parsed_accounts.len() {
                    Some(parsed_accounts[index as usize].clone())
                } else {
                    warn!("Account index {} out of bounds (total accounts: {}), skipping", index, parsed_accounts.len());
                    None
                }
            })
            .collect();

        // 如果账户列表为空且原始指令有账户，说明所有索引都越界了
        if accounts.is_empty() && !ci.accounts.is_empty() {
            warn!("All account indices out of bounds for instruction with program_id: {}", program_id);
        }

        Instruction {
            program_id,
            accounts,
            data: ci.data.clone(),
        }
    }

    /// 解析交易账户
    fn parse_transaction_accounts(&self, message: &VersionedMessage, loaded_addresses: LoadedAddresses) -> Vec<AccountMeta> {
        let accounts = message.static_account_keys();
        let header = message.header();
        let readonly_signed_accounts_count = header.num_readonly_signed_accounts as usize;
        let readonly_unsigned_accounts_count = header.num_readonly_unsigned_accounts as usize;
        let required_signatures_accounts_count = header.num_required_signatures as usize;
        let total_accounts = accounts.len();

        let mut parsed_accounts: Vec<AccountMeta> = accounts
            .iter()
            .enumerate()
            .map(|(index, pubkey)| {
                let is_writable = index < required_signatures_accounts_count - readonly_signed_accounts_count
                    || (index >= required_signatures_accounts_count && index < total_accounts - readonly_unsigned_accounts_count);

                AccountMeta {
                    pubkey: *pubkey,
                    is_signer: index < required_signatures_accounts_count,
                    is_writable,
                }
            })
            .collect();

        // 添加加载的地址
        parsed_accounts.extend(loaded_addresses.writable.into_iter().map(|pubkey| AccountMeta {
            pubkey,
            is_signer: false,
            is_writable: true,
        }));

        parsed_accounts.extend(loaded_addresses.readonly.into_iter().map(|pubkey| AccountMeta {
            pubkey,
            is_signer: false,
            is_writable: false,
        }));

        parsed_accounts
    }
}

#[derive(Debug, Clone)]
pub struct ParsedConfirmedTransaction {
    pub slot: u64,
    pub signature: String,
    pub compiled_instructions: Vec<DecodedInstruction>,
    pub inner_instructions: Vec<DecodedInstruction>,
    pub meta: TransactionStatusMeta,
}

impl Default for IdlTransactionProcessor {
    fn default() -> Self {
        Self::new().expect("Failed to create IdlTransactionProcessor")
    }
}