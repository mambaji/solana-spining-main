use solana_sdk::{signature::Signature, pubkey::Pubkey};
use async_trait::async_trait;
use serde::{Serialize, Deserialize};
use crate::executor::errors::ExecutionError;

/// 交易执行策略类型
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ExecutionStrategy {
    /// 0slot执行策略 - 零延迟确认
    ZeroSlot { 
        tip_lamports: u64,
        region: String, // "fra", "ams", "ny", "tyo", "lax"
    },
    /// 自动回退策略 - 按优先级尝试多个服务
    Fallback {
        strategies: Vec<ExecutionStrategy>,
        max_retries_per_strategy: u32,
    },
}

impl Default for ExecutionStrategy {
    fn default() -> Self {
        ExecutionStrategy::ZeroSlot {
            tip_lamports: 100_000,
            region: "ny".to_string(),
        }
    }
}

/// 交易执行结果
#[derive(Debug, Clone)]
pub struct ExecutionResult {
    /// 交易签名
    pub signature: Signature,
    /// 使用的执行策略
    pub strategy_used: ExecutionStrategy,
    /// 实际费用 (lamports)
    pub actual_fee_paid: u64,
    /// 执行延迟 (毫秒)
    pub execution_latency_ms: u64,
    /// 确认状态
    pub confirmation_status: String,
    /// 是否成功
    pub success: bool,
    /// 额外元数据
    pub metadata: std::collections::HashMap<String, String>,
}

/// 交易参数
#[derive(Debug, Clone)]
pub struct TradeParams {
    /// 代币mint地址
    pub mint: Pubkey,
    /// SOL数量 (lamports) - 买入交易使用
    pub sol_amount: u64,
    /// 最小代币输出 (滑点保护) - 买入交易使用
    pub min_tokens_out: u64,
    /// 代币数量 (最小单位) - 卖出交易使用
    pub token_amount: Option<u64>,
    /// 最小SOL输出 (滑点保护) - 卖出交易使用
    pub min_sol_out: Option<u64>,
    /// 最大滑点 (基点, 100 = 1%)
    pub max_slippage_bps: u16,
    /// 是否为买入交易 (false为卖出)
    pub is_buy: bool,
    /// 🔧 新增：代币创建者地址 (PumpFun 必需)
    pub creator: Option<Pubkey>,
}

/// 统一的交易执行器trait
#[async_trait]
pub trait TransactionExecutor: Send + Sync {
    /// 执行交易
    async fn execute_trade(
        &self,
        trade_params: TradeParams,
        strategy: ExecutionStrategy,
    ) -> Result<ExecutionResult, ExecutionError>;

    /// 获取钱包余额
    async fn get_balance(&self) -> Result<u64, ExecutionError>;

    /// 验证交易参数
    fn validate_params(&self, params: &TradeParams) -> Result<(), ExecutionError>;

    /// 检查服务健康状态
    async fn health_check(&self) -> Result<bool, ExecutionError>;
}

/// 交易构建器trait
pub trait TransactionBuilder {
    /// 构建PumpFun买入交易 (带 creator 参数)
    fn build_pumpfun_buy_with_creator(
        &self,
        mint: &Pubkey,
        buyer: &Pubkey,
        sol_amount: u64,
        min_tokens_out: u64,
        creator: &Pubkey,
    ) -> Result<solana_sdk::instruction::Instruction, ExecutionError>;

    /// 构建PumpFun卖出交易 (带 creator 参数)
    fn build_pumpfun_sell_with_creator(
        &self,
        mint: &Pubkey,
        seller: &Pubkey,
        token_amount: u64,
        min_sol_out: u64,
        creator: &Pubkey,
    ) -> Result<solana_sdk::instruction::Instruction, ExecutionError>;

    /// 构建优先费用指令
    fn build_priority_fee_instruction(
        &self,
        priority_fee: u64,
    ) -> solana_sdk::instruction::Instruction;

    /// 构建计算预算指令 (限制和优先费用)
    fn build_compute_budget_instructions(
        &self,
    ) -> Vec<solana_sdk::instruction::Instruction>;

    /// 构建卖出专用计算预算指令 (卖出通常需要更少的计算单元和优先费)
    fn build_sell_compute_budget_instructions(
        &self,
    ) -> Vec<solana_sdk::instruction::Instruction>;

    /// 为特定交易类型构建计算预算指令
    fn build_compute_budget_for_transaction(
        &self,
        transaction_type: &str,
        fee_level: crate::executor::compute_budget::FeeLevel,
    ) -> Vec<solana_sdk::instruction::Instruction>;

    /// 🆕 从TradeSignal构建计算预算指令
    fn build_compute_budget_from_signal(
        &self, 
        signal: &crate::strategy::TradeSignal,
        compute_budget_manager: Option<&crate::executor::compute_budget::DynamicComputeBudgetManager>,
    ) -> Vec<solana_sdk::instruction::Instruction>;

    /// 构建完整的交易 (未签名)
    fn build_transaction(
        &self,
        instructions: Vec<solana_sdk::instruction::Instruction>,
        payer: &Pubkey,
        recent_blockhash: solana_sdk::hash::Hash,
    ) -> Result<solana_sdk::transaction::VersionedTransaction, ExecutionError>;

    /// 构建并签名交易
    fn build_signed_transaction(
        &self,
        instructions: Vec<solana_sdk::instruction::Instruction>,
        payer: &solana_sdk::signature::Keypair,
        recent_blockhash: solana_sdk::hash::Hash,
    ) -> Result<solana_sdk::transaction::VersionedTransaction, ExecutionError>;
}