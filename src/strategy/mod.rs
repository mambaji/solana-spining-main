// pub mod token_filter;
// pub mod token_sniper;
pub mod trade_signal;
pub mod position;

// 优化后的模块
pub mod optimized_token_filter;
pub mod optimized_strategy_manager;
pub mod optimized_trading_strategy;

use serde::{Deserialize, Serialize};

/// 策略配置 (从trading_strategy.rs移动)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrategyConfig {
    /// 买入金额 (lamports)
    pub buy_amount_lamports: u64,
    /// 最大滑点 (基点, 100 = 1%)
    pub max_slippage_bps: u16,
    /// 持仓时间 (秒) - 60秒后全部卖出
    pub holding_duration_seconds: u64,
    /// 止损阈值 (负百分比, -20.0 = -20%)
    pub stop_loss_percentage: Option<f64>,
    /// 止盈阈值 (正百分比, 50.0 = +50%)
    pub take_profit_percentage: Option<f64>,
    /// 是否启用紧急卖出
    pub enable_emergency_sell: bool,
}

impl Default for StrategyConfig {
    fn default() -> Self {
        Self {
            buy_amount_lamports: 100_000_000, // 0.1 SOL
            max_slippage_bps: 300, // 3%
            holding_duration_seconds: 60, // 60秒
            stop_loss_percentage: Some(-20.0), // -20% 止损
            take_profit_percentage: Some(100.0), // +100% 止盈
            enable_emergency_sell: true,
        }
    }
}

// pub use token_filter::{TokenFilter, FilterCriteria, FilterResult, filter_token_with_default_strategy, filter_token_with_conservative_strategy};
// pub use token_sniper::{TokenSniper, example_token_sniper_usage, example_different_strategies};
pub use trade_signal::{TradeSignal, TradeSignalType, SignalPriority};
pub use position::{Position, PositionStatus, TradeRecord};

// 优化后的导出
pub use optimized_token_filter::{OptimizedTokenFilter, SimpleFilterResult, filter_token_optimized};
pub use optimized_strategy_manager::{OptimizedStrategyManager, OptimizedStrategyManagerStats};
pub use optimized_trading_strategy::{OptimizedTradingStrategy, OptimizedPosition, OptimizedStrategyStatus, OptimizedPositionStatus};