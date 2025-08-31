use solana_sdk::{
    instruction::{Instruction, AccountMeta},
    pubkey::Pubkey,
    transaction::VersionedTransaction,
    message::{VersionedMessage, v0::Message},
    hash::Hash,
    compute_budget::ComputeBudgetInstruction,
    signature::{Keypair, Signer},
};
use spl_associated_token_account::{get_associated_token_address, instruction::create_associated_token_account};
use std::str::FromStr;
use log::info;
use crate::executor::{
    errors::ExecutionError, 
    traits::{TradeParams, TransactionBuilder as TransactionBuilderTrait},
    compute_budget::{DynamicComputeBudgetManager, FeeLevel, ComputeBudgetConfig},
};
use crate::strategy::TradeSignal;
use solana_client::rpc_client::RpcClient;

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
            pumpfun_program_id: Pubkey::from_str("6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P")
                .expect("Invalid PumpFun program ID"),
            compute_budget_manager: DynamicComputeBudgetManager::new(
                ComputeBudgetConfig::default(),
                None, // 可以后续设置RPC客户端
            ),
            default_fee_level: FeeLevel::Standard,
            endpoint: None,
        }
    }

    /// 创建带RPC客户端的交易构建器并启动费用监控
    pub async fn with_rpc_client_and_monitoring(rpc_client: RpcClient, endpoint: String) -> Result<Self, ExecutionError> {
        let builder = Self {
            pumpfun_program_id: Pubkey::from_str("6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P")
                .expect("Invalid PumpFun program ID"),
            compute_budget_manager: DynamicComputeBudgetManager::new(
                ComputeBudgetConfig::default(),
                Some(rpc_client),
            ),
            default_fee_level: FeeLevel::Standard,
            endpoint: Some(endpoint),
        };

        // 启动费用监控任务
        builder.compute_budget_manager.start_fee_monitoring().await?;
        
        info!("🚀 TransactionBuilder 已创建并启动费用监控");
        Ok(builder)
    }

    /// 使用外部计算预算管理器创建交易构建器 (避免创建多个实例)
    pub fn with_compute_budget_manager(compute_budget_manager: DynamicComputeBudgetManager) -> Self {
        Self {
            pumpfun_program_id: Pubkey::from_str("6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P")
                .expect("Invalid PumpFun program ID"),
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

    /// 构建完整的 PumpFun 买入交易 (包含计算预算) - 同步版本，使用预设值
    pub fn build_complete_pumpfun_buy_transaction(
        &self,
        _mint: &Pubkey,
        _buyer: &Pubkey,
        _sol_amount: u64,
        _min_tokens_out: u64,
        _recent_blockhash: Hash,
    ) -> Result<VersionedTransaction, ExecutionError> {
        return Err(ExecutionError::InvalidParams(
            "Use build_complete_pumpfun_buy_transaction_with_creator instead - creator address required".to_string()
        ));
    }

    /// 构建完整的 PumpFun 买入交易 (包含计算预算和 creator)
    pub fn build_complete_pumpfun_buy_transaction_with_creator(
        &self,
        mint: &Pubkey,
        buyer: &Keypair,
        sol_amount: u64,
        min_tokens_out: u64,
        creator: &Pubkey,
        recent_blockhash: Hash,
    ) -> Result<VersionedTransaction, ExecutionError> {
        let mut instructions = Vec::new();
        
        // 1. 添加计算预算指令 (必须在最前面)
        instructions.extend(self.build_compute_budget_instructions());
        
        // 2. 检查是否需要创建ATA账户
        // 注意：PumpFun程序会自动处理账户创建，避免重复创建
        // 只有在确认账户不存在时才创建
        // TODO: 添加账户存在性检查，暂时移除自动创建以避免重复
        
        // 3. 添加 PumpFun 买入指令 (程序内部会处理账户创建)
        let pumpfun_instruction = self.build_pumpfun_buy_with_creator(mint, &buyer.pubkey(), sol_amount, min_tokens_out, creator)?;
        instructions.push(pumpfun_instruction);
        
        // 4. 构建交易
        self.build_signed_transaction(instructions, buyer, recent_blockhash)
    }

    /// 构建完整的 PumpFun 买入交易 (手动创建账户版本，参考别人的实现)
    pub fn build_complete_pumpfun_buy_transaction_with_manual_account_creation(
        &self,
        mint: &Pubkey,
        buyer: &Keypair,
        sol_amount: u64,
        min_tokens_out: u64,
        creator: &Pubkey,
        recent_blockhash: Hash,
    ) -> Result<VersionedTransaction, ExecutionError> {
        let mut instructions = Vec::new();
        
        // 1. 添加计算预算指令
        instructions.extend(self.build_compute_budget_instructions());
        
        // 2. 手动创建代币账户 (使用createAccountWithSeed方式) 
        let (manual_account_instructions, _token_account) = self.build_manual_token_account_creation(mint, &buyer.pubkey())?;
        instructions.extend(manual_account_instructions);
        
        // 3. 添加 PumpFun 买入指令
        let pumpfun_instruction = self.build_pumpfun_buy_with_creator(mint, &buyer.pubkey(), sol_amount, min_tokens_out, creator)?;
        instructions.push(pumpfun_instruction);
        
        // 4. 构建交易
        self.build_signed_transaction(instructions, buyer, recent_blockhash)
    }

    /// 构建完整的 PumpFun 卖出交易 (无需创建账户)
    pub fn build_complete_pumpfun_sell_transaction(
        &self,
        mint: &Pubkey,
        seller: &Keypair,
        token_amount: u64,
        min_sol_out: u64,
        creator: &Pubkey,
        recent_blockhash: Hash,
    ) -> Result<VersionedTransaction, ExecutionError> {
        let mut instructions = Vec::new();
        
        // 1. 添加计算预算指令 (卖出通常需要更少的计算单元)
        instructions.extend(self.build_sell_compute_budget_instructions());
        
        // 2. 添加 PumpFun 卖出指令 (不需要创建账户，直接使用已存在的ATA)
        let pumpfun_instruction = self.build_pumpfun_sell_with_creator(mint, &seller.pubkey(), token_amount, min_sol_out, creator)?;
        instructions.push(pumpfun_instruction);
        
        // 3. 构建交易
        self.build_signed_transaction(instructions, seller, recent_blockhash)
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

    /// 构建带 tip 的完整 PumpFun 买入交易
    pub fn build_complete_pumpfun_buy_transaction_with_tip(
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
        
        // 2. 添加 PumpFun 买入指令 (程序内部会处理账户创建)
        let pumpfun_instruction = self.build_pumpfun_buy_with_creator(mint, &buyer.pubkey(), sol_amount, min_tokens_out, creator)?;
        instructions.push(pumpfun_instruction);
        
        // 3. 添加 tip 指令 (在流程最后执行)
        instructions.push(tip_instruction);
        
        // 4. 构建交易
        self.build_signed_transaction(instructions, buyer, recent_blockhash)
    }

    /// 构建带 tip 的完整 PumpFun 买入交易 (高效手动账户创建版本)
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
        
        // 2. 手动创建代币账户 (使用成功的 createAccountWithSeed 方式)
        let (manual_account_instructions, _token_account) = self.build_manual_token_account_creation(mint, &buyer.pubkey())?;
        instructions.extend(manual_account_instructions);
        
        // 3. 添加 PumpFun 买入指令
        let pumpfun_instruction = self.build_pumpfun_buy_with_creator(mint, &buyer.pubkey(), sol_amount, min_tokens_out, creator)?;
        instructions.push(pumpfun_instruction);
        
        // 4. 添加 tip 指令 (在流程最后执行)
        instructions.push(tip_instruction);
        
        // 5. 构建交易 (不需要额外签名者)
        self.build_signed_transaction(instructions, buyer, recent_blockhash)
    }

    /// 构建手动代币账户创建指令 (使用成功的 createAccountWithSeed 方式)
    pub fn build_manual_token_account_creation(
        &self,
        mint: &Pubkey,
        owner: &Pubkey,
    ) -> Result<(Vec<Instruction>, Pubkey), ExecutionError> {
        use solana_sdk::system_instruction;
        use spl_token::instruction as token_instruction;
        
        let mut instructions = Vec::new();
        
        // 1. 使用 createAccountWithSeed 创建账户 (基于成功交易分析)
        let seed = format!("{:08x}", rand::random::<u32>()); // 8位十六进制种子
        let token_account = Pubkey::create_with_seed(owner, &seed, &spl_token::id())
            .map_err(|e| ExecutionError::Internal(format!("Failed to create account with seed: {}", e)))?;
        
        info!("🔑 创建代币账户 (with seed): {}, seed: {}", token_account, seed);
        
        // 2. 创建账户指令 (使用种子)
        let lamports = 2039280; // 代币账户所需的最小租金
        let space = 165; // SPL代币账户的标准大小
        
        let create_account_instruction = system_instruction::create_account_with_seed(
            owner,              // from (付费者)
            &token_account,     // new_account (新账户)
            owner,              // base (基础账户)
            &seed,              // seed (种子)
            lamports,           // lamports (租金)
            space,              // space (账户大小)
            &spl_token::id(),   // owner (程序所有者)
        );
        instructions.push(create_account_instruction);
        
        // 3. 初始化代币账户指令 (SPL Token程序)
        let initialize_account_instruction = token_instruction::initialize_account(
            &spl_token::id(),   // token_program_id
            &token_account,     // account (要初始化的账户)
            mint,               // mint (代币mint)
            owner,              // owner (账户所有者)
        ).map_err(|e| ExecutionError::Internal(format!("Failed to create initialize_account instruction: {}", e)))?;
        instructions.push(initialize_account_instruction);
        
        Ok((instructions, token_account))
    }

    /// 创建用户的关联代币账户指令 (基于参考实现)
    pub fn build_create_ata_instruction(
        &self,
        mint: &Pubkey,
        owner: &Pubkey,
    ) -> Result<Instruction, ExecutionError> {
        // 基于 pumpfun-rs 参考实现：总是创建 ATA 指令
        // 如果账户已存在，Solana 会忽略重复创建
        let token_program = spl_token::id();
        let instruction = create_associated_token_account(
            owner,          // payer
            owner,          // wallet  
            mint,           // mint
            &token_program, // token_program
        );
        
        Ok(instruction)
    }

    /// 构建PumpFun交易数据 - 基于官方 IDL
    fn build_pumpfun_instruction_data(trade: &PumpFunTrade) -> Vec<u8> {
        match trade {
            PumpFunTrade::Buy { sol_amount, min_tokens_out, .. } => {
                // 根据 IDL: discriminator: [102, 6, 61, 18, 1, 218, 235, 234]
                // args: amount(u64), max_sol_cost(u64)
                let mut data = vec![102, 6, 61, 18, 1, 218, 235, 234]; // 正确的买入指令标识
                data.extend_from_slice(&min_tokens_out.to_le_bytes()); // amount - 要买入的代币数量
                data.extend_from_slice(&sol_amount.to_le_bytes());     // max_sol_cost - 最大 SOL 成本
                data
            }
            PumpFunTrade::Sell { token_amount, min_sol_out, .. } => {
                // 根据 IDL: discriminator: [51, 230, 133, 164, 1, 127, 131, 173]
                // args: amount(u64), min_sol_output(u64)
                let mut data = vec![51, 230, 133, 164, 1, 127, 131, 173]; // 正确的卖出指令标识
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
        let system_program = Pubkey::from_str("11111111111111111111111111111111")
            .map_err(|e| ExecutionError::Internal(format!("Invalid system program ID: {}", e)))?;
        
        // Token程序
        let token_program = Pubkey::from_str("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA")
            .map_err(|e| ExecutionError::Internal(format!("Invalid token program ID: {}", e)))?;
        
        // 1. global PDA - 根据 IDL seeds: ["global"]
        let (global, _) = Pubkey::find_program_address(
            &[b"global"],
            &self.pumpfun_program_id,
        );
        
        // 2. fee_recipient - 从全局状态读取，这里使用链上交易中的地址
        let fee_recipient = derive_pumpfun_fee_account()?;
        
        // 4. bonding_curve PDA - 根据 IDL seeds: ["bonding-curve", mint]  
        let (bonding_curve, _) = Pubkey::find_program_address(
            &[b"bonding-curve", mint.as_ref()],
            &self.pumpfun_program_id,
        );
        
        // 5. associated_bonding_curve - ATA of bonding_curve for mint
        let associated_bonding_curve = get_associated_token_address(&bonding_curve, mint);
        
        // 6. associated_user - 用户的代币关联账户
        let associated_user = get_associated_token_address(user, mint);
        
        // 10. creator_vault PDA - 使用传入的真实 creator 地址
        let creator_vault = if let Some(creator_addr) = creator {
            let (vault, _) = Pubkey::find_program_address(
                &[b"creator-vault", creator_addr.as_ref()],
                &self.pumpfun_program_id,
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
            &[b"__event_authority"],
            &self.pumpfun_program_id,
        );

        // 12. 全局交易量累加器 - 新增必需账户
        let global_volume_accumulator = get_global_volume_accumulator()?;

        // 13. 用户交易量累加器 PDA - 新增必需账户
        let user_volume_accumulator = get_user_volume_accumulator_pda(user, &self.pumpfun_program_id);

        // 根据成功交易的精确顺序构建账户列表 (14个账户，与参考实现一致)
        Ok(vec![
            AccountMeta::new_readonly(global, false),              // 0. global
            AccountMeta::new(fee_recipient, false),                // 1. fee_recipient  
            AccountMeta::new_readonly(*mint, false),               // 2. mint
            AccountMeta::new(bonding_curve, false),                // 3. bonding_curve
            AccountMeta::new(associated_bonding_curve, false),     // 4. associated_bonding_curve
            AccountMeta::new(associated_user, false),              // 5. associated_user
            AccountMeta::new(*user, true),                         // 6. user (签名者)
            AccountMeta::new_readonly(system_program, false),      // 7. system_program
            AccountMeta::new_readonly(token_program, false),       // 8. token_program
            AccountMeta::new(creator_vault, false),                // 9. creator_vault
            AccountMeta::new_readonly(event_authority, false),     // 10. event_authority
            AccountMeta::new_readonly(self.pumpfun_program_id, false), // 11. pump.fun program ✅ 新增
            AccountMeta::new(global_volume_accumulator, false),    // 12. global_volume_accumulator ✅
            AccountMeta::new(user_volume_accumulator, false),      // 13. user_volume_accumulator ✅
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
        let system_program = Pubkey::from_str("11111111111111111111111111111111")
            .map_err(|e| ExecutionError::Internal(format!("Invalid system program ID: {}", e)))?;
        
        // Token程序
        let token_program = Pubkey::from_str("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA")
            .map_err(|e| ExecutionError::Internal(format!("Invalid token program ID: {}", e)))?;
        
        // 1. global PDA
        let (global, _) = Pubkey::find_program_address(
            &[b"global"],
            &self.pumpfun_program_id,
        );
        
        // 2. fee_recipient - 卖出使用不同的fee账户
        let fee_recipient = derive_pumpfun_sell_fee_account()?;
        
        // 4. bonding_curve PDA  
        let (bonding_curve, _) = Pubkey::find_program_address(
            &[b"bonding-curve", mint.as_ref()],
            &self.pumpfun_program_id,
        );
        
        // 5. associated_bonding_curve - ATA of bonding_curve for mint
        let associated_bonding_curve = get_associated_token_address(&bonding_curve, mint);
        
        // 6. associated_user - 用户的代币关联账户 (卖出时作为源账户)
        let associated_user = get_associated_token_address(user, mint);
        
        // 9. creator_vault PDA
        let creator_vault = if let Some(creator_addr) = creator {
            let (vault, _) = Pubkey::find_program_address(
                &[b"creator-vault", creator_addr.as_ref()],
                &self.pumpfun_program_id,
            );
            vault
        } else {
            return Err(ExecutionError::InvalidParams(
                "Creator address is required for PumpFun sell transactions".to_string()
            ));
        };
        
        // 11. event_authority PDA
        let (event_authority, _) = Pubkey::find_program_address(
            &[b"__event_authority"],
            &self.pumpfun_program_id,
        );

        // 卖出账户列表 (12个账户，基于链上数据)
        Ok(vec![
            AccountMeta::new_readonly(global, false),              // 0. global
            AccountMeta::new(fee_recipient, false),                // 1. fee_recipient (卖出专用)
            AccountMeta::new_readonly(*mint, false),               // 2. mint
            AccountMeta::new(bonding_curve, false),                // 3. bonding_curve
            AccountMeta::new(associated_bonding_curve, false),     // 4. associated_bonding_curve
            AccountMeta::new(associated_user, false),              // 5. associated_user (卖出源)
            AccountMeta::new(*user, true),                         // 6. user (签名者)
            AccountMeta::new_readonly(system_program, false),      // 7. system_program
            AccountMeta::new(creator_vault, false),                // 8. creator_vault
            AccountMeta::new_readonly(token_program, false),       // 9. token_program
            AccountMeta::new_readonly(event_authority, false),     // 10. event_authority
            AccountMeta::new_readonly(self.pumpfun_program_id, false), // 11. pump.fun program
        ])
    }

    /// 构建带有滑点保护的交易参数
    pub fn apply_slippage_protection(
        trade_params: &TradeParams,
    ) -> Result<PumpFunTrade, ExecutionError> {
        match trade_params.is_buy {
            true => {
                // 买入：计算最小代币输出 (考虑滑点)
                let min_tokens_out = if trade_params.min_tokens_out > 0 {
                    trade_params.min_tokens_out
                } else {
                    // 如果没有指定，根据滑点计算
                    // 这里需要实际的价格计算逻辑，暂时使用占位符
                    calculate_min_tokens_with_slippage(
                        trade_params.sol_amount,
                        trade_params.max_slippage_bps,
                        &trade_params.mint,
                    )?
                };

                Ok(PumpFunTrade::Buy {
                    mint: trade_params.mint,
                    sol_amount: trade_params.sol_amount,
                    min_tokens_out,
                })
            }
            false => {
                // 卖出：需要从用户余额获取代币数量
                return Err(ExecutionError::InvalidParams(
                    "Sell transactions need token amount from user balance".to_string()
                ));
            }
        }
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
            program_id: self.pumpfun_program_id,
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
            program_id: self.pumpfun_program_id,
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
                // 验证账户数量为14个（包含新增的交易量追踪账户）
                assert_eq!(account_list.len(), 14, "PumpFun账户列表应该有14个账户");
                
                // 验证签名者账户
                assert!(account_list[6].is_signer, "第6个账户应该是签名者");
                
                // 验证程序账户
                assert_eq!(account_list[11].pubkey, builder.pumpfun_program_id, "第11个账户应该是PumpFun程序");
                
                println!("✅ PumpFun 账户列表验证通过: {} 个账户", account_list.len());
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
        
        // 验证数据长度 (8字节标识符 + 8字节数量 + 8字节最大成本)
        assert_eq!(data.len(), 24, "买入指令数据长度应该是24字节");
        
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
