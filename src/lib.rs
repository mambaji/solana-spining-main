pub mod streams;
pub mod processors;
pub mod serialization;
pub mod idl;
pub mod config;
pub mod strategy;
pub mod executor;
pub mod utils;

// Re-export commonly used types
pub use config::StreamShyftConfig;
pub use processors::{TokenEvent, TransactionType, TokenDetector, process_transaction_for_tokens};
pub use streams::{ShyftStream, LetsbonkStream};
pub use strategy::{
    // TokenFilter, FilterCriteria, FilterResult, TokenSniper,
    TradeSignal, TradeSignalType, SignalPriority,
    Position, PositionStatus, TradeRecord,
    StrategyConfig,
    // 新增：优化后的组件
    OptimizedStrategyManager, OptimizedTokenFilter, SimpleFilterResult
};
pub use executor::{
    ExecutorConfig, ExecutionStrategy, ExecutionResult, TradeParams,
    TransactionExecutor, ExecutionError,
    // 优化后的组件
    OptimizedExecutorManager, ExecutorManagerStats,
    // 区块哈希缓存
    BlockhashCache,
};
pub use utils::{
    EventLogger, TokenBalanceClient,
};