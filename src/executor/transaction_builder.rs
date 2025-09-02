use solana_sdk::{
    instruction::{Instruction, AccountMeta},
    pubkey::Pubkey,
    transaction::VersionedTransaction,
    message::{VersionedMessage, v0::Message},
    hash::Hash,
    compute_budget::ComputeBudgetInstruction,
    signature::{Keypair, Signer},
};
use spl_associated_token_account::get_associated_token_address;
use spl_token::instruction::close_account;
use std::str::FromStr;
use log::info;
use crate::constant::accounts::{PUMPFUN, SYSTEM_PROGRAM, TOKEN_PROGRAM};
use crate::constant::seeds::{GLOBAL_SEED, BONDING_CURVE_SEED, EVENT_AUTHORITY_SEED, CREATOR_VAULT_SEED};
use crate::executor::{
    errors::ExecutionError, 
    traits::{TransactionBuilder as TransactionBuilderTrait},
    compute_budget::{DynamicComputeBudgetManager, FeeLevel, ComputeBudgetConfig},
};
use crate::strategy::TradeSignal;

/// PumpFun交易类型
#[derive(Debug, Clone)]

pub enum PumpFunTrade {
    Buy {
        mint: Pubkey,
        sol_amount: u64,
        min_tokens_out: u64,
    },
    Sell {
        mint: Pubkey,
        token_amount: u64,
        min_sol_out: u64,
    },
}

/// 交易构建器
pub struct TransactionBuilder {
    /// PumpFun程序ID
    pub pumpfun_program_id: Pubkey,
    /// 动态计算预算管理器
    pub compute_budget_manager: DynamicComputeBudgetManager,
    /// 默认费用级别
    pub default_fee_level: FeeLevel,
    /// RPC端点标识符（用于费用历史记录）
    pub endpoint: Option<String>,
}

impl Default for TransactionBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl TransactionBuilder {
    /// 创建新的交易构建器
    pub fn new() -> Self {
        Self {
            // PumpFun官方程序ID
            pumpfun_program_id: PUMPFUN,
            compute_budget_manager: DynamicComputeBudgetManager::new(
                ComputeBudgetConfig::default(),
                None, // 可以后续设置RPC客户端
            ),
            default_fee_level: FeeLevel::Standard,
            endpoint: None,
        }
    }

    /// 使用外部计算预算管理器创建交易构建器 (避免创建多个实例)
    pub fn with_compute_budget_manager(compute_budget_manager: DynamicComputeBudgetManager) -> Self {
        Self {
            pumpfun_program_id: PUMPFUN,
            compute_budget_manager,
            default_fee_level: FeeLevel::Standard,
            endpoint: None,
        }
    }

    /// 启动费用监控 (如果尚未启动)
    pub async fn start_fee_monitoring(&self) -> Result<(), ExecutionError> {
        self.compute_budget_manager.start_fee_monitoring().await
    }

    /// 停止费用监控
    pub fn stop_fee_monitoring(&self) {
        self.compute_budget_manager.stop_fee_monitoring();
    }

    /// 获取费用统计信息
    pub fn get_fee_stats(&self) -> (usize, usize, Option<u64>, Option<u64>) {
        self.compute_budget_manager.get_fee_stats()
    }

    /// 设置默认费用级别
    pub fn with_fee_level(mut self, fee_level: FeeLevel) -> Self {
        self.default_fee_level = fee_level;
        self
    }
    
    /// 构建带 tip 的完整 PumpFun 卖出交易
    pub fn build_complete_pumpfun_sell_transaction_with_tip(
        &self,
        mint: &Pubkey,
        seller: &Keypair,
        token_amount: u64,
        min_sol_out: u64,
        creator: &Pubkey,
        tip_instruction: solana_sdk::instruction::Instruction,
        recent_blockhash: Hash,
    ) -> Result<VersionedTransaction, ExecutionError> {
        let mut instructions = Vec::new();
        
        // 1. 添加计算预算指令 (卖出使用专门配置)
        instructions.extend(self.build_sell_compute_budget_instructions());
        
        // 2. 添加 PumpFun 卖出指令
        let pumpfun_instruction = self.build_pumpfun_sell_with_creator(mint, &seller.pubkey(), token_amount, min_sol_out, creator)?;
        instructions.push(pumpfun_instruction);
        
        // 3. 添加 tip 指令 (在流程最后执行)
        instructions.push(tip_instruction);
        
        // 4. 构建交易
        self.build_signed_transaction(instructions, seller, recent_blockhash)
    }

    /// 构建带 tip 和 ATA 关闭的完整 PumpFun 卖出交易
    pub fn build_complete_pumpfun_sell_transaction_with_tip_and_ata_close(
        &self,
        mint: &Pubkey,
        seller: &Keypair,
        token_amount: u64,
        min_sol_out: u64,
        creator: &Pubkey,
        tip_instruction: solana_sdk::instruction::Instruction,
        recent_blockhash: Hash,
        should_close_ata: bool,
    ) -> Result<VersionedTransaction, ExecutionError> {
        let mut instructions = Vec::new();
        
        // 1. 添加计算预算指令 (卖出使用专门配置)
        instructions.extend(self.build_sell_compute_budget_instructions());
        
        // 2. 添加 PumpFun 卖出指令
        let pumpfun_instruction = self.build_pumpfun_sell_with_creator(mint, &seller.pubkey(), token_amount, min_sol_out, creator)?;
        instructions.push(pumpfun_instruction);
        
        // 3. 如果需要，添加 ATA 关闭指令
        if should_close_ata {
            let ata = get_associated_token_address(&seller.pubkey(), mint);
            let close_instruction = close_account(
                &TOKEN_PROGRAM,
                &ata,
                &seller.pubkey(),
                &seller.pubkey(),
                &[&seller.pubkey()],
            ).map_err(|e| ExecutionError::Internal(format!("Failed to create close account instruction: {}", e)))?;
            instructions.push(close_instruction);
        }
        
        // 4. 添加 tip 指令 (在流程最后执行)
        instructions.push(tip_instruction);
        
        // 5. 构建交易
        self.build_signed_transaction(instructions, seller, recent_blockhash)
    }

    /// 构建带 tip 的完整 PumpFun 买入交易 (基于种子的账户创建方式)
    pub fn build_complete_pumpfun_buy_transaction_with_tip_and_manual_account(
        &self,
        mint: &Pubkey,
        buyer: &Keypair,
        sol_amount: u64,
        min_tokens_out: u64,
        creator: &Pubkey,
        tip_instruction: solana_sdk::instruction::Instruction,
        recent_blockhash: Hash,
    ) -> Result<VersionedTransaction, ExecutionError> {
        let mut instructions = Vec::new();
        
        // 1. 添加计算预算指令 (必须在最前面)
        instructions.extend(self.build_compute_budget_instructions());
        
        // 2. 使用基于种子的账户创建方式 (模拟成功交易)
        let seed = self.generate_token_account_seed(mint, &buyer.pubkey())?;
        let token_account = self.derive_token_account_with_seed(&buyer.pubkey(), &seed)?;
        
        // 2.1 创建带种子的账户
        let create_account_instruction = solana_sdk::system_instruction::create_account_with_seed(
            &buyer.pubkey(),        // 付款人
            &token_account,         // 新账户地址
            &buyer.pubkey(),        // 基地址
            &seed,                  // 种子
            2039280,                // rent-exempt lamports (固定值，基于成功交易)
            165,                    // 空间大小 (token账户标准大小)
            &TOKEN_PROGRAM,         // 所有者程序
        );
        instructions.push(create_account_instruction);
        
        // 2.2 初始化Token账户
        let init_account_instruction = spl_token::instruction::initialize_account3(
            &TOKEN_PROGRAM,
            &token_account,
            mint,
            &buyer.pubkey(),
        ).map_err(|e| ExecutionError::Internal(format!("Failed to create initialize_account3 instruction: {}", e)))?;
        instructions.push(init_account_instruction);
        
        // 3. 添加 PumpFun 买入指令
        let pumpfun_instruction = self.build_pumpfun_buy_with_creator(mint, &buyer.pubkey(), sol_amount, min_tokens_out, creator)?;
        instructions.push(pumpfun_instruction);
        
        // 4. 添加 tip 指令 (在流程最后执行)
        instructions.push(tip_instruction);
        
        // 5. 构建交易 (不需要额外签名者)
        self.build_signed_transaction(instructions, buyer, recent_blockhash)
    }

    /// 构建PumpFun交易数据 - 基于官方 IDL v0.1.0
    fn build_pumpfun_instruction_data(trade: &PumpFunTrade) -> Vec<u8> {
        match trade {
            PumpFunTrade::Buy { sol_amount, min_tokens_out, .. } => {
                // 根据 IDL v0.1.0: discriminator: [102, 6, 61, 18, 1, 218, 235, 234]
                // args: amount(u64), max_sol_cost(u64), track_volume(OptionBool)
                let mut data = vec![102, 6, 61, 18, 1, 218, 235, 234]; // 买入指令标识
                data.extend_from_slice(&min_tokens_out.to_le_bytes()); // amount - 要买入的代币数量
                data.extend_from_slice(&sol_amount.to_le_bytes());     // max_sol_cost - 最大 SOL 成本
                data.push(0); // track_volume: OptionBool = false (0 = None, 1 = Some(false), 2 = Some(true))
                data
            }
            PumpFunTrade::Sell { token_amount, min_sol_out, .. } => {
                // 根据 IDL v0.1.0: discriminator: [51, 230, 133, 164, 1, 127, 131, 173]
                // args: amount(u64), min_sol_output(u64)
                let mut data = vec![51, 230, 133, 164, 1, 127, 131, 173]; // 卖出指令标识
                data.extend_from_slice(&token_amount.to_le_bytes());  // amount - 要卖出的代币数量
                data.extend_from_slice(&min_sol_out.to_le_bytes());   // min_sol_output - 最小 SOL 输出
                data
            }
        }
    }

    /// 获取PumpFun相关账户 - 基于官方 IDL 精确顺序
    fn get_pumpfun_accounts(
        &self,
        trade: &PumpFunTrade,
        user: &Pubkey,
        creator: Option<&Pubkey>,
    ) -> Result<Vec<AccountMeta>, ExecutionError> {
        match trade {
            PumpFunTrade::Buy { mint, .. } => {
                self.get_pumpfun_buy_accounts(mint, user, creator)
            }
            PumpFunTrade::Sell { mint, .. } => {
                self.get_pumpfun_sell_accounts(mint, user, creator)
            }
        }
    }

    /// 获取PumpFun买入账户 (完整版本包含交易量累加器)
    fn get_pumpfun_buy_accounts(
        &self,
        mint: &Pubkey,
        user: &Pubkey,
        creator: Option<&Pubkey>,
    ) -> Result<Vec<AccountMeta>, ExecutionError> {
        // 系统程序
        let system_program = SYSTEM_PROGRAM;
        
        // Token程序
        let token_program = TOKEN_PROGRAM;
        
        // 1. global PDA - 根据 IDL seeds: ["global"]
        let (global, _) = Pubkey::find_program_address(
            &[GLOBAL_SEED],
            &PUMPFUN,
        );
        
        // 2. fee_recipient - 从全局状态读取，这里使用链上交易中的地址
        let fee_recipient = derive_pumpfun_fee_account()?;
        
        // 4. bonding_curve PDA - 根据 IDL seeds: ["bonding-curve", mint]  
        let (bonding_curve, _) = Pubkey::find_program_address(
            &[BONDING_CURVE_SEED, mint.as_ref()],
            &PUMPFUN,
        );
        
        // 5. associated_bonding_curve - ATA of bonding_curve for mint
        let associated_bonding_curve = get_associated_token_address(&bonding_curve, mint);
        
        // 6. associated_user - 使用基于种子的账户地址而不是ATA
        let seed = self.generate_token_account_seed(mint, user)?;
        let associated_user = self.derive_token_account_with_seed(user, &seed)?;
        
        // 10. creator_vault PDA - 使用传入的真实 creator 地址
        let creator_vault = if let Some(creator_addr) = creator {
            let (vault, _) = Pubkey::find_program_address(
                &[CREATOR_VAULT_SEED, creator_addr.as_ref()],
                &PUMPFUN,
            );
            vault
        } else {
            // 如果没有 creator，返回错误
            return Err(ExecutionError::InvalidParams(
                "Creator address is required for PumpFun transactions".to_string()
            ));
        };
        
        // 11. event_authority PDA - 根据 IDL seeds: ["__event_authority"]
        let (event_authority, _) = Pubkey::find_program_address(
            &[EVENT_AUTHORITY_SEED],
            &PUMPFUN,
        );

        // 12. 全局交易量累加器 - 新增必需账户
        let global_volume_accumulator = get_global_volume_accumulator()?;

        // 13. 用户交易量累加器 PDA - 新增必需账户
        let user_volume_accumulator = get_user_volume_accumulator_pda(user, &PUMPFUN);

        // 14. fee_program - 新增费用程序地址
        let fee_program = Pubkey::from_str("pfeeUxB6jkeY1Hxd7CsFCAjcbHA9rWtchMGdZ6VojVZ")
            .map_err(|e| ExecutionError::Internal(format!("Invalid fee_program address: {}", e)))?;

        // 15. fee_config PDA - 新增费用配置账户
        let fee_config_seeds = [
            b"fee_config".as_ref(),
            &[1, 86, 224, 246, 147, 102, 90, 207, 68, 219, 21, 104, 191, 23, 91, 170, 81, 137, 203, 151, 245, 210, 255, 59, 101, 93, 43, 182, 253, 109, 24, 176],
        ];
        let (fee_config, _) = Pubkey::find_program_address(&fee_config_seeds, &fee_program);

        // 根据最新 IDL 构建账户列表 (16个账户，包含新增的 fee_config 和 fee_program)
        Ok(vec![
            AccountMeta::new_readonly(global, false),              // 0. global
            AccountMeta::new(fee_recipient, false),                // 1. fee_recipient  
            AccountMeta::new_readonly(*mint, false),               // 2. mint
            AccountMeta::new(bonding_curve, false),                // 3. bonding_curve
            AccountMeta::new(associated_bonding_curve, false),     // 4. associated_bonding_curve
            AccountMeta::new(associated_user, false),              // 5. associated_user (基于种子)
            AccountMeta::new(*user, true),                         // 6. user (签名者)
            AccountMeta::new_readonly(system_program, false),      // 7. system_program
            AccountMeta::new_readonly(token_program, false),       // 8. token_program
            AccountMeta::new(creator_vault, false),                // 9. creator_vault
            AccountMeta::new_readonly(event_authority, false),     // 10. event_authority
            AccountMeta::new_readonly(PUMPFUN, false),             // 11. pump.fun program
            AccountMeta::new(global_volume_accumulator, false),    // 12. global_volume_accumulator
            AccountMeta::new(user_volume_accumulator, false),      // 13. user_volume_accumulator
            AccountMeta::new_readonly(fee_config, false),          // 14. fee_config ✅ 新增
            AccountMeta::new_readonly(fee_program, false),         // 15. fee_program ✅ 新增
        ])
    }

    /// 获取PumpFun卖出账户 (简化版本，基于链上数据分析)
    fn get_pumpfun_sell_accounts(
        &self,
        mint: &Pubkey,
        user: &Pubkey,
        creator: Option<&Pubkey>,
    ) -> Result<Vec<AccountMeta>, ExecutionError> {
        // 系统程序
        let system_program = SYSTEM_PROGRAM;
        
        // Token程序
        let token_program = TOKEN_PROGRAM;
        
        // 1. global PDA
        let (global, _) = Pubkey::find_program_address(
            &[GLOBAL_SEED],
            &PUMPFUN,
        );
        
        // 2. fee_recipient - 卖出使用不同的fee账户
        let fee_recipient = derive_pumpfun_sell_fee_account()?;
        
        // 4. bonding_curve PDA  
        let (bonding_curve, _) = Pubkey::find_program_address(
            &[BONDING_CURVE_SEED, mint.as_ref()],
            &PUMPFUN,
        );
        
        // 5. associated_bonding_curve - ATA of bonding_curve for mint
        let associated_bonding_curve = get_associated_token_address(&bonding_curve, mint);
        
        // 6. associated_user - 卖出时使用基于种子的账户地址 (如果存在)，否则使用ATA
        let associated_user = if let Ok(seed) = self.generate_token_account_seed(mint, user) {
            self.derive_token_account_with_seed(user, &seed)?
        } else {
            // 如果种子生成失败，回退到ATA
            get_associated_token_address(user, mint)
        };
        
        // 9. creator_vault PDA
        let creator_vault = if let Some(creator_addr) = creator {
            let (vault, _) = Pubkey::find_program_address(
                &[CREATOR_VAULT_SEED, creator_addr.as_ref()],
                &PUMPFUN,
            );
            vault
        } else {
            return Err(ExecutionError::InvalidParams(
                "Creator address is required for PumpFun sell transactions".to_string()
            ));
        };
        
        // 11. event_authority PDA
        let (event_authority, _) = Pubkey::find_program_address(
            &[EVENT_AUTHORITY_SEED],
            &PUMPFUN,
        );

        // 12. 全局交易量累加器 - 新增必需账户
        let global_volume_accumulator = get_global_volume_accumulator()?;

        // 13. 用户交易量累加器 PDA - 新增必需账户
        let user_volume_accumulator = get_user_volume_accumulator_pda(user, &PUMPFUN);

        // 14. fee_program - 新增费用程序地址
        let fee_program = Pubkey::from_str("pfeeUxB6jkeY1Hxd7CsFCAjcbHA9rWtchMGdZ6VojVZ")
            .map_err(|e| ExecutionError::Internal(format!("Invalid fee_program address: {}", e)))?;

        // 15. fee_config PDA - 新增费用配置账户
        let fee_config_seeds = [
            b"fee_config".as_ref(),
            &[1, 86, 224, 246, 147, 102, 90, 207, 68, 219, 21, 104, 191, 23, 91, 170, 81, 137, 203, 151, 245, 210, 255, 59, 101, 93, 43, 182, 253, 109, 24, 176],
        ];
        let (fee_config, _) = Pubkey::find_program_address(&fee_config_seeds, &fee_program);

        // 卖出账户列表 (16个账户，与最新IDL一致)
        Ok(vec![
            AccountMeta::new_readonly(global, false),              // 0. global
            AccountMeta::new(fee_recipient, false),                // 1. fee_recipient (卖出专用)
            AccountMeta::new_readonly(*mint, false),               // 2. mint
            AccountMeta::new(bonding_curve, false),                // 3. bonding_curve
            AccountMeta::new(associated_bonding_curve, false),     // 4. associated_bonding_curve
            AccountMeta::new(associated_user, false),              // 5. associated_user (基于种子或ATA)
            AccountMeta::new(*user, true),                         // 6. user (签名者)
            AccountMeta::new_readonly(system_program, false),      // 7. system_program
            AccountMeta::new(creator_vault, false),                // 8. creator_vault
            AccountMeta::new_readonly(token_program, false),       // 9. token_program
            AccountMeta::new_readonly(event_authority, false),     // 10. event_authority
            AccountMeta::new_readonly(PUMPFUN, false),             // 11. pump.fun program
            AccountMeta::new(global_volume_accumulator, false),    // 12. global_volume_accumulator
            AccountMeta::new(user_volume_accumulator, false),      // 13. user_volume_accumulator
            AccountMeta::new_readonly(fee_config, false),          // 14. fee_config ✅ 新增
            AccountMeta::new_readonly(fee_program, false),         // 15. fee_program ✅ 新增
        ])
    }
}

impl TransactionBuilderTrait for TransactionBuilder {
    /// 构建PumpFun买入交易 (带 creator 参数)
    fn build_pumpfun_buy_with_creator(
        &self,
        mint: &Pubkey,
        buyer: &Pubkey,
        sol_amount: u64,
        min_tokens_out: u64,
        creator: &Pubkey,
    ) -> Result<Instruction, ExecutionError> {
        let trade = PumpFunTrade::Buy {
            mint: *mint,
            sol_amount,
            min_tokens_out,
        };

        let instruction_data = Self::build_pumpfun_instruction_data(&trade);
        let accounts = self.get_pumpfun_accounts(&trade, buyer, Some(creator))?;

        Ok(Instruction {
            program_id: PUMPFUN,
            accounts,
            data: instruction_data,
        })
    }

    /// 构建PumpFun卖出交易 (带 creator 参数)
    fn build_pumpfun_sell_with_creator(
        &self,
        mint: &Pubkey,
        seller: &Pubkey,
        token_amount: u64,
        min_sol_out: u64,
        creator: &Pubkey,
    ) -> Result<Instruction, ExecutionError> {
        let trade = PumpFunTrade::Sell {
            mint: *mint,
            token_amount,
            min_sol_out,
        };

        let instruction_data = Self::build_pumpfun_instruction_data(&trade);
        let accounts = self.get_pumpfun_accounts(&trade, seller, Some(creator))?;

        Ok(Instruction {
            program_id: PUMPFUN,
            accounts,
            data: instruction_data,
        })
    }

    /// 构建计算预算指令 - 现在支持动态计算，但保持向后兼容
    fn build_compute_budget_instructions(&self) -> Vec<Instruction> {
        // 使用预设的标准配置，保持向后兼容
        self.build_compute_budget_for_transaction("pumpfun_buy", FeeLevel::Standard)
    }

    /// 构建卖出专用计算预算指令 - 现在支持动态计算
    fn build_sell_compute_budget_instructions(&self) -> Vec<Instruction> {
        // 使用预设的标准配置，但优化为卖出交易
        self.build_compute_budget_for_transaction("pumpfun_sell", FeeLevel::Standard)
    }

    /// 为特定交易类型构建计算预算指令 (同步版本，使用固定CU和动态费用)
    fn build_compute_budget_for_transaction(&self, transaction_type: &str, fee_level: FeeLevel) -> Vec<Instruction> {
        // 使用固定的CU值
        let compute_units = match transaction_type {
            "pumpfun_buy" => crate::executor::compute_budget::PUMPFUN_BUY_CU,
            "pumpfun_sell" => crate::executor::compute_budget::PUMPFUN_SELL_CU,
            _ => crate::executor::compute_budget::PUMPFUN_BUY_CU, // 默认使用买入CU
        };

        // 使用动态获取的优先费用
        let priority_fee = match transaction_type {
            "pumpfun_buy" => self.compute_budget_manager.get_current_buy_priority_fee(fee_level),
            "pumpfun_sell" => self.compute_budget_manager.get_current_sell_priority_fee(fee_level),
            _ => self.compute_budget_manager.get_current_buy_priority_fee(fee_level),
        };

        info!("📊 固定预算配置: CU={}, 优先费={} micro-lamports/CU, 类型={}, 级别={:?}", 
              compute_units, priority_fee, transaction_type, fee_level);

        vec![
            ComputeBudgetInstruction::set_compute_unit_limit(compute_units),
            ComputeBudgetInstruction::set_compute_unit_price(priority_fee),
        ]
    }

    /// 🆕 从TradeSignal构建计算预算指令
    fn build_compute_budget_from_signal(
        &self, 
        signal: &TradeSignal,
        compute_budget_manager: Option<&DynamicComputeBudgetManager>,
    ) -> Vec<Instruction> {
        let compute_units = signal.compute_units;
        
        // 获取优先费用：优先使用自定义费用，否则通过管理器查询分档费用
        let priority_fee = if let Some(custom_fee) = signal.custom_priority_fee {
            custom_fee
        } else if let Some(manager) = compute_budget_manager {
            let is_buy = matches!(signal.signal_type, crate::strategy::TradeSignalType::Buy);
            if is_buy {
                manager.get_buy_priority_fee_by_tier(signal.priority_fee_tier)
            } else {
                manager.get_sell_priority_fee_by_tier(signal.priority_fee_tier)
            }
        } else {
            // 没有管理器时使用默认费用
            10000 // 默认10k micro-lamports/CU
        };
        
        info!("⚡ 从信号构建计算预算: CU={}, 档位={}, 优先费={} micro-lamports/CU", 
              compute_units, signal.priority_fee_tier.as_str(), priority_fee);
        
        vec![
            ComputeBudgetInstruction::set_compute_unit_limit(compute_units),
            ComputeBudgetInstruction::set_compute_unit_price(priority_fee),
        ]
    }

    /// 构建优先费用指令 (保留向后兼容性)
    fn build_priority_fee_instruction(&self, priority_fee: u64) -> Instruction {
        ComputeBudgetInstruction::set_compute_unit_price(priority_fee)
    }


    /// 构建并签名交易
    fn build_signed_transaction(
        &self,
        instructions: Vec<Instruction>,
        payer: &Keypair,
        recent_blockhash: Hash,
    ) -> Result<VersionedTransaction, ExecutionError> {
        self.build_signed_transaction_with_additional_signers(instructions, payer, &[], recent_blockhash)
    }

}

impl TransactionBuilder {
    /// 🆕 公开方法：获取用户在特定mint的代币账户地址（基于种子派生）
    /// 这个方法确保余额查询和交易构建使用相同的账户地址
    pub fn get_user_token_account_address(&self, mint: &Pubkey, user: &Pubkey) -> Result<Pubkey, ExecutionError> {
        let seed = self.generate_token_account_seed(mint, user)?;
        self.derive_token_account_with_seed(user, &seed)
    }

    /// 生成Token账户种子 (基于成功交易的模式)
    fn generate_token_account_seed(&self, mint: &Pubkey, user: &Pubkey) -> Result<String, ExecutionError> {
        // 根据成功交易分析，使用16字节的hex字符串作为种子
        // 原成功交易使用的种子: "56d38adc42e2b91e579271e74067f5b7"
        // 我们可以基于用户地址和mint生成类似的种子
        
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        
        let mut hasher = DefaultHasher::new();
        user.hash(&mut hasher);
        mint.hash(&mut hasher);
        let hash = hasher.finish();
        
        // 转换为32字符的十六进制字符串 (16字节)
        Ok(format!("{:016x}{:016x}", hash, hash.wrapping_add(12345)))
    }
    
    /// 使用种子派生Token账户地址
    fn derive_token_account_with_seed(&self, base: &Pubkey, seed: &str) -> Result<Pubkey, ExecutionError> {
        Pubkey::create_with_seed(base, seed, &TOKEN_PROGRAM)
            .map_err(|e| ExecutionError::Internal(format!("Failed to derive account with seed: {}", e)))
    }

    /// 构建并签名交易 (支持额外签名者) - 专用于手动账户创建
    pub fn build_signed_transaction_with_additional_signers(
        &self,
        instructions: Vec<Instruction>,
        payer: &Keypair,
        additional_signers: &[Keypair],
        recent_blockhash: Hash,
    ) -> Result<VersionedTransaction, ExecutionError> {
        let message = Message::try_compile(
            &payer.pubkey(),
            &instructions,
            &[], // 地址查找表 (暂时为空)
            recent_blockhash,
        ).map_err(|e| ExecutionError::Serialization(format!("Failed to compile message: {}", e)))?;

        let versioned_message = VersionedMessage::V0(message);
        
        // 构建签名者列表：payer + 额外签名者
        let mut signers = vec![payer];
        signers.extend(additional_signers.iter());
        
        // 创建签名的交易
        VersionedTransaction::try_new(versioned_message, &signers)
            .map_err(|e| ExecutionError::Serialization(format!("Failed to sign transaction: {}", e)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use solana_sdk::signature::Keypair;

    #[test]
    fn test_pumpfun_account_count() {
        let builder = TransactionBuilder::new();
        let mint = Keypair::new().pubkey();
        let user = Keypair::new().pubkey();
        let creator = Keypair::new().pubkey();
        
        let trade = PumpFunTrade::Buy {
            mint,
            sol_amount: 1_000_000,
            min_tokens_out: 1000,
        };

        let accounts = builder.get_pumpfun_accounts(&trade, &user, Some(&creator));
        
        match accounts {
            Ok(account_list) => {
                // 验证账户数量为16个（包含新增的 fee_config 和 fee_program）
                assert_eq!(account_list.len(), 16, "PumpFun账户列表应该有16个账户");
                
                // 验证签名者账户
                assert!(account_list[6].is_signer, "第6个账户应该是签名者");
                
                // 验证程序账户
                assert_eq!(account_list[11].pubkey, builder.pumpfun_program_id, "第11个账户应该是PumpFun程序");
                
                // 验证新增的 fee_program 账户
                let expected_fee_program = Pubkey::from_str("pfeeUxB6jkeY1Hxd7CsFCAjcbHA9rWtchMGdZ6VojVZ").unwrap();
                assert_eq!(account_list[15].pubkey, expected_fee_program, "第15个账户应该是费用程序");
                
                println!("✅ PumpFun 账户列表验证通过: {} 个账户 (已包含 fee_config 和 fee_program)", account_list.len());
                for (i, account) in account_list.iter().enumerate() {
                    println!("  账户 {}: {} (可写: {}, 签名: {})", 
                        i, account.pubkey, account.is_writable, account.is_signer);
                }
            }
            Err(e) => {
                panic!("❌ 获取PumpFun账户失败: {}", e);
            }
        }
    }

    #[test] 
    fn test_buy_instruction_data() {
        let trade = PumpFunTrade::Buy {
            mint: Keypair::new().pubkey(),
            sol_amount: 1_000_000,     // 0.001 SOL
            min_tokens_out: 500,       // 最少500个代币
        };

        let data = TransactionBuilder::build_pumpfun_instruction_data(&trade);
        
        // 验证指令标识符
        assert_eq!(&data[0..8], &[102, 6, 61, 18, 1, 218, 235, 234], "买入指令标识符不正确");
        
        // 验证数据长度 (8字节标识符 + 8字节数量 + 8字节最大成本 + 1字节track_volume)
        assert_eq!(data.len(), 25, "买入指令数据长度应该是25字节（包含track_volume参数）");
        
        println!("✅ 买入指令数据验证通过: {:?}", data);
    }

    #[test]
    fn test_ata_calculation() {
        // 验证失败交易中的ATA地址计算
        let user = Pubkey::try_from("GrFqNyRtKoHdGAUfZTS3oRMZJeGxrbAt1hyyDJD5YN8S").unwrap();
        let mint = Pubkey::try_from("5LkRMviCAsmko8WW53giuomstk1u165es73JEeqppump").unwrap();
        let expected_ata = Pubkey::try_from("6pLKHMcFQhsMQgvkee9tZmEVHFCFUc8B14amF4P3cVb8").unwrap();
        
        let calculated_ata = get_associated_token_address(&user, &mint);
        
        println!("用户地址: {}", user);
        println!("代币mint: {}", mint);
        println!("期望ATA: {}", expected_ata);
        println!("计算ATA: {}", calculated_ata);
        
        assert_eq!(calculated_ata, expected_ata, "ATA地址计算不匹配！");
        
        println!("✅ ATA地址计算验证通过");
    }
}

/// 辅助函数：派生PumpFun全局账户 - 基于 IDL

/// 辅助函数：派生PumpFun费用账户 - 来自链上交易数据 (买入)
fn derive_pumpfun_fee_account() -> Result<Pubkey, ExecutionError> {
    // 从链上成功交易中观察到的费用接收账户
    // Account 4: AVmoTthdrX6tKt4nDjco2D775W2YK3sDhxPcMmzUAmTY
    Pubkey::from_str("AVmoTthdrX6tKt4nDjco2D775W2YK3sDhxPcMmzUAmTY")
        .map_err(|e| ExecutionError::Internal(format!("Invalid fee account: {}", e)))
}

/// 辅助函数：派生PumpFun卖出费用账户 - 来自链上交易数据 (卖出)
fn derive_pumpfun_sell_fee_account() -> Result<Pubkey, ExecutionError> {
    // 从链上卖出交易中观察到的费用接收账户
    // Account: CebN5WGQ4jvEPvsVU4EoHEpgzq1VV7AbicfhtW4xC9iM
    Pubkey::from_str("CebN5WGQ4jvEPvsVU4EoHEpgzq1VV7AbicfhtW4xC9iM")
        .map_err(|e| ExecutionError::Internal(format!("Invalid sell fee account: {}", e)))
}

/// 辅助函数：获取全局交易量累加器地址
fn get_global_volume_accumulator() -> Result<Pubkey, ExecutionError> {
    // 从成功交易中观察到的全局交易量累加器地址
    Pubkey::from_str("Hq2wp8uJ9jCPsYgNHex8RtqdvMPfVGoYwjvF1ATiwn2Y")
        .map_err(|e| ExecutionError::Internal(format!("Invalid global volume accumulator: {}", e)))
}

/// 辅助函数：派生用户交易量累加器 PDA
fn get_user_volume_accumulator_pda(user: &Pubkey, program_id: &Pubkey) -> Pubkey {
    let (pda, _) = Pubkey::find_program_address(
        &[b"user_volume_accumulator", user.as_ref()],
        program_id,
    );
    pda
}

/// 辅助函数：根据滑点计算最小代币输出
fn calculate_min_tokens_with_slippage(
    sol_amount: u64,
    slippage_bps: u16,
    _mint: &Pubkey,
) -> Result<u64, ExecutionError> {
    // 这里需要实际的价格计算逻辑
    // 暂时返回一个基于sol_amount的估算值
    let estimated_tokens = sol_amount * 1_000; // 假设1 SOL = 1000 tokens
    let slippage_multiplier = 10000 - slippage_bps as u64; // 10000基点 = 100%
    Ok((estimated_tokens * slippage_multiplier) / 10000)
}
