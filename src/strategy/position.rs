use serde::{Deserialize, Serialize};
use solana_sdk::pubkey::Pubkey;
use crate::executor::ExecutionResult;

/// 仓位状态
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum PositionStatus {
    /// 空仓 - 没有持仓
    Empty,
    /// 买入中 - 正在执行买入交易
    Buying,
    /// 持仓中 - 有活跃仓位
    Holding,
    /// 卖出中 - 正在执行卖出交易
    Selling,
    /// 已平仓 - 交易完成
    Closed,
    /// 错误状态 - 需要手动处理
    Error,
}

/// 交易记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradeRecord {
    /// 交易时间戳
    pub timestamp: i64,
    /// 交易类型 (买入/卖出)
    pub is_buy: bool,
    /// SOL数量 (lamports)
    pub sol_amount: u64,
    /// 代币数量
    pub token_amount: u64,
    /// 交易签名
    pub signature: String,
    /// 实际费用
    pub fee_paid: u64,
    /// 交易价格 (SOL per token)
    pub price: f64,
}

impl TradeRecord {
    /// 从执行结果创建交易记录
    pub fn from_execution_result(result: &ExecutionResult, is_buy: bool, sol_amount: u64, token_amount: u64) -> Self {
        let price = if token_amount > 0 {
            sol_amount as f64 / token_amount as f64
        } else {
            0.0
        };

        Self {
            timestamp: chrono::Utc::now().timestamp(),
            is_buy,
            sol_amount,
            token_amount,
            signature: result.signature.to_string(),
            fee_paid: result.actual_fee_paid,
            price,
        }
    }
}

/// 仓位信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Position {
    /// 代币mint地址
    pub mint: Pubkey,
    /// 策略ID
    pub strategy_id: String,
    /// 当前状态
    pub status: PositionStatus,
    /// 持有的代币数量
    pub token_amount: u64,
    /// 总投入的SOL数量 (lamports)
    pub total_sol_invested: u64,
    /// 平均买入价格 (SOL per token)
    pub average_buy_price: f64,
    /// 创建时间戳
    pub created_at: i64,
    /// 最后更新时间戳
    pub updated_at: i64,
    /// 交易记录
    pub trades: Vec<TradeRecord>,
    /// 总费用 (lamports)
    pub total_fees: u64,
}

impl Position {
    /// 创建新的空仓位
    pub fn new(mint: Pubkey, strategy_id: String) -> Self {
        let now = chrono::Utc::now().timestamp();
        
        Self {
            mint,
            strategy_id,
            status: PositionStatus::Empty,
            token_amount: 0,
            total_sol_invested: 0,
            average_buy_price: 0.0,
            created_at: now,
            updated_at: now,
            trades: Vec::new(),
            total_fees: 0,
        }
    }

    /// 记录买入交易
    pub fn record_buy(&mut self, sol_amount: u64, token_amount: u64, execution_result: &ExecutionResult) {
        let trade_record = TradeRecord::from_execution_result(execution_result, true, sol_amount, token_amount);
        
        // 更新仓位信息
        let _old_total_value = self.total_sol_invested;
        let _old_token_amount = self.token_amount;
        
        self.total_sol_invested += sol_amount;
        self.token_amount += token_amount;
        self.total_fees += execution_result.actual_fee_paid;
        
        // 计算新的平均买入价格
        if self.token_amount > 0 {
            self.average_buy_price = self.total_sol_invested as f64 / self.token_amount as f64;
        }
        
        self.trades.push(trade_record);
        self.status = PositionStatus::Holding;
        self.updated_at = chrono::Utc::now().timestamp();
        
        log::info!("📈 仓位买入记录 - Mint: {}", self.mint);
        log::info!("   💰 SOL投入: {:.4} (+{:.4})", 
            self.total_sol_invested as f64 / 1_000_000_000.0,
            sol_amount as f64 / 1_000_000_000.0
        );
        log::info!("   🪙 代币数量: {} (+{})", self.token_amount, token_amount);
        log::info!("   💵 平均价格: {:.9} SOL/token", self.average_buy_price);
        log::info!("   💸 总费用: {} lamports", self.total_fees);
    }

    /// 记录卖出交易
    pub fn record_sell(&mut self, sol_received: u64, token_amount_sold: u64, execution_result: &ExecutionResult) {
        let trade_record = TradeRecord::from_execution_result(execution_result, false, sol_received, token_amount_sold);
        
        // 更新仓位信息
        self.token_amount = self.token_amount.saturating_sub(token_amount_sold);
        self.total_fees += execution_result.actual_fee_paid;
        
        self.trades.push(trade_record);
        
        // 更新状态
        if self.token_amount == 0 {
            self.status = PositionStatus::Closed;
        } else {
            self.status = PositionStatus::Holding;
        }
        
        self.updated_at = chrono::Utc::now().timestamp();
        
        // 计算这笔交易的盈亏
        let cost_basis = token_amount_sold as f64 * self.average_buy_price;
        let pnl = sol_received as f64 - cost_basis;
        let pnl_percentage = if cost_basis > 0.0 {
            (pnl / cost_basis) * 100.0
        } else {
            0.0
        };
        
        log::info!("📉 仓位卖出记录 - Mint: {}", self.mint);
        log::info!("   💰 SOL收到: {:.4}", sol_received as f64 / 1_000_000_000.0);
        log::info!("   🪙 卖出数量: {}", token_amount_sold);
        log::info!("   📊 成本基础: {:.4} SOL", cost_basis / 1_000_000_000.0);
        log::info!("   💹 盈亏: {:.4} SOL ({:+.2}%)", 
            pnl / 1_000_000_000.0, pnl_percentage);
        log::info!("   🪙 剩余代币: {}", self.token_amount);
        log::info!("   💸 总费用: {} lamports", self.total_fees);
    }

    /// 设置仓位状态
    pub fn set_status(&mut self, status: PositionStatus) {
        self.status = status;
        self.updated_at = chrono::Utc::now().timestamp();
    }

    /// 计算当前持仓价值 (按给定价格)
    pub fn calculate_current_value(&self, current_price_per_token: f64) -> f64 {
        self.token_amount as f64 * current_price_per_token
    }

    /// 计算总盈亏 (按给定价格)
    pub fn calculate_pnl(&self, current_price_per_token: f64) -> (f64, f64) {
        let current_value = self.calculate_current_value(current_price_per_token);
        let total_invested = self.total_sol_invested as f64;
        let total_fees = self.total_fees as f64;
        
        // 计算已实现收益 (从卖出交易中)
        let realized_gains: f64 = self.trades
            .iter()
            .filter(|t| !t.is_buy)
            .map(|t| t.sol_amount as f64)
            .sum();
        
        // 未实现盈亏 = 当前价值 - 剩余成本基础
        let remaining_cost_basis = if self.token_amount > 0 {
            self.token_amount as f64 * self.average_buy_price
        } else {
            0.0
        };
        let _unrealized_pnl = current_value - remaining_cost_basis;
        
        // 总盈亏 = 已实现收益 + 未实现盈亏 - 总投入 - 总费用
        let total_pnl = realized_gains + current_value - total_invested - total_fees;
        let total_pnl_percentage = if total_invested > 0.0 {
            (total_pnl / total_invested) * 100.0
        } else {
            0.0
        };
        
        (total_pnl, total_pnl_percentage)
    }

    /// 获取交易统计
    pub fn get_trade_stats(&self) -> (usize, usize, f64, f64) {
        let total_trades = self.trades.len();
        let buy_trades = self.trades.iter().filter(|t| t.is_buy).count();
        let sell_trades = self.trades.iter().filter(|t| !t.is_buy).count();
        
        let total_volume: f64 = self.trades
            .iter()
            .map(|t| t.sol_amount as f64)
            .sum();
        
        let avg_trade_size = if total_trades > 0 {
            total_volume / total_trades as f64
        } else {
            0.0
        };
        
        (buy_trades, sell_trades, total_volume, avg_trade_size)
    }

    /// 是否为空仓
    pub fn is_empty(&self) -> bool {
        matches!(self.status, PositionStatus::Empty) && self.token_amount == 0
    }

    /// 是否为活跃仓位
    pub fn is_active(&self) -> bool {
        matches!(
            self.status, 
            PositionStatus::Buying | PositionStatus::Holding | PositionStatus::Selling
        ) && self.token_amount > 0
    }

    /// 是否已关闭
    pub fn is_closed(&self) -> bool {
        matches!(self.status, PositionStatus::Closed) || self.token_amount == 0
    }

    /// 获取持仓时长 (秒)
    pub fn get_holding_duration(&self) -> i64 {
        chrono::Utc::now().timestamp() - self.created_at
    }

    /// 打印仓位摘要
    pub fn print_summary(&self, current_price_per_token: Option<f64>) {
        log::info!("📊 仓位摘要 - {}", self.strategy_id);
        log::info!("   🪙 代币: {}", self.mint);
        log::info!("   📈 状态: {:?}", self.status);
        log::info!("   💰 持有数量: {}", self.token_amount);
        log::info!("   💵 总投入: {:.4} SOL", self.total_sol_invested as f64 / 1_000_000_000.0);
        log::info!("   💸 总费用: {:.4} SOL", self.total_fees as f64 / 1_000_000_000.0);
        log::info!("   📊 平均成本: {:.9} SOL/token", self.average_buy_price);
        log::info!("   🕐 持仓时长: {}秒", self.get_holding_duration());
        
        let (buy_count, sell_count, total_volume, avg_size) = self.get_trade_stats();
        log::info!("   📈 买入次数: {}", buy_count);
        log::info!("   📉 卖出次数: {}", sell_count);
        log::info!("   💰 总交易量: {:.4} SOL", total_volume / 1_000_000_000.0);
        log::info!("   📊 平均交易: {:.4} SOL", avg_size / 1_000_000_000.0);
        
        if let Some(price) = current_price_per_token {
            let current_value = self.calculate_current_value(price);
            let (pnl, pnl_pct) = self.calculate_pnl(price);
            log::info!("   💹 当前价值: {:.4} SOL", current_value / 1_000_000_000.0);
            log::info!("   📊 盈亏: {:.4} SOL ({:+.2}%)", pnl / 1_000_000_000.0, pnl_pct);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use solana_sdk::{pubkey::Pubkey, signature::Signature};
    use crate::executor::{ExecutionResult, ExecutionStrategy};

    fn create_mock_execution_result() -> ExecutionResult {
        ExecutionResult {
            signature: Signature::new_unique(),
            strategy_used: ExecutionStrategy::default(),
            actual_fee_paid: 5000,
            execution_latency_ms: 100,
            confirmation_status: "confirmed".to_string(),
            success: true,
            metadata: std::collections::HashMap::new(),
        }
    }

    #[test]
    fn test_position_creation() {
        let mint = Pubkey::new_unique();
        let strategy_id = "test-strategy".to_string();
        let position = Position::new(mint, strategy_id.clone());

        assert_eq!(position.mint, mint);
        assert_eq!(position.strategy_id, strategy_id);
        assert_eq!(position.status, PositionStatus::Empty);
        assert_eq!(position.token_amount, 0);
        assert_eq!(position.total_sol_invested, 0);
        assert_eq!(position.average_buy_price, 0.0);
        assert!(position.is_empty());
        assert!(!position.is_active());
    }

    #[test]
    fn test_buy_recording() {
        let mut position = Position::new(Pubkey::new_unique(), "test".to_string());
        let result = create_mock_execution_result();
        
        position.record_buy(1_000_000_000, 1_000_000, &result); // 1 SOL for 1M tokens
        
        assert_eq!(position.status, PositionStatus::Holding);
        assert_eq!(position.token_amount, 1_000_000);
        assert_eq!(position.total_sol_invested, 1_000_000_000);
        assert_eq!(position.average_buy_price, 0.001); // 1 SOL / 1M tokens
        assert_eq!(position.trades.len(), 1);
        assert!(position.is_active());
        assert!(!position.is_empty());
    }

    #[test]
    fn test_sell_recording() {
        let mut position = Position::new(Pubkey::new_unique(), "test".to_string());
        let result1 = create_mock_execution_result();
        let result2 = create_mock_execution_result();
        
        // 先买入
        position.record_buy(1_000_000_000, 1_000_000, &result1);
        
        // 再卖出一半
        position.record_sell(600_000_000, 500_000, &result2); // 卖出50万代币获得0.6 SOL
        
        assert_eq!(position.status, PositionStatus::Holding);
        assert_eq!(position.token_amount, 500_000); // 剩余50万代币
        assert_eq!(position.trades.len(), 2);
        
        // 计算盈亏
        let (pnl, pnl_pct) = position.calculate_pnl(0.001); // 假设当前价格不变
        assert!(pnl > 0.0); // 应该是盈利的，因为卖出价格更高
    }

    #[test]
    fn test_full_position_close() {
        let mut position = Position::new(Pubkey::new_unique(), "test".to_string());
        let result1 = create_mock_execution_result();
        let result2 = create_mock_execution_result();
        
        // 买入
        position.record_buy(1_000_000_000, 1_000_000, &result1);
        
        // 全部卖出
        position.record_sell(1_100_000_000, 1_000_000, &result2); // 盈利10%
        
        assert_eq!(position.status, PositionStatus::Closed);
        assert_eq!(position.token_amount, 0);
        assert!(position.is_closed());
        assert!(!position.is_active());
        
        let (pnl, pnl_pct) = position.calculate_pnl(0.0); // 价格无关紧要，因为没有持仓
        assert!(pnl > 0.0); // 应该是盈利的
    }

    #[test]
    fn test_average_buy_price_calculation() {
        let mut position = Position::new(Pubkey::new_unique(), "test".to_string());
        let result1 = create_mock_execution_result();
        let result2 = create_mock_execution_result();
        
        // 第一次买入: 1 SOL 买 1M 代币 (价格 0.001)
        position.record_buy(1_000_000_000, 1_000_000, &result1);
        assert!((position.average_buy_price - 0.001).abs() < f64::EPSILON);
        
        // 第二次买入: 2 SOL 买 1M 代币 (价格 0.002)
        position.record_buy(2_000_000_000, 1_000_000, &result2);
        
        // 平均价格应该是 (1+2) SOL / 2M 代币 = 0.0015
        assert!((position.average_buy_price - 0.0015).abs() < f64::EPSILON);
        assert_eq!(position.token_amount, 2_000_000);
        assert_eq!(position.total_sol_invested, 3_000_000_000);
    }
}