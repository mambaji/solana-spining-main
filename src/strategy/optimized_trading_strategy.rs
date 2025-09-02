use anyhow::Result;
use log::{info, warn, error, debug};
use solana_sdk::pubkey::Pubkey;
use std::sync::atomic::{AtomicU8, AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::time::Duration;

use crate::processors::TokenEvent;
use crate::executor::ExecutionResult;
use super::{TradeSignal, SignalPriority, StrategyConfig};

/// 优化后的策略状态 - 使用原子操作，无锁访问
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(u8)]
pub enum OptimizedStrategyStatus {
    Initializing = 0,
    Running = 1,
    Paused = 2,
    Stopping = 3,
    Stopped = 4,
    Error = 5,
}

impl From<u8> for OptimizedStrategyStatus {
    fn from(value: u8) -> Self {
        match value {
            0 => Self::Initializing,
            1 => Self::Running,
            2 => Self::Paused,
            3 => Self::Stopping,
            4 => Self::Stopped,
            5 => Self::Error,
            _ => Self::Error,
        }
    }
}

/// 优化后的仓位状态 - 使用原子操作
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(u8)]
pub enum OptimizedPositionStatus {
    Empty = 0,
    Buying = 1,
    Holding = 2,
    Selling = 3,
    Closed = 4,
}

impl From<u8> for OptimizedPositionStatus {
    fn from(value: u8) -> Self {
        match value {
            0 => Self::Empty,
            1 => Self::Buying,
            2 => Self::Holding,
            3 => Self::Selling,
            4 => Self::Closed,
            _ => Self::Empty,
        }
    }
}

/// 优化后的仓位信息 - 无锁并发访问
#[derive(Debug)]
pub struct OptimizedPosition {
    pub strategy_id: String,
    pub mint: Pubkey,
    
    // 使用原子操作管理关键数值
    pub status: Arc<AtomicU8>, // OptimizedPositionStatus
    pub token_amount: Arc<AtomicU64>,
    pub sol_invested: Arc<AtomicU64>,
    pub sol_returned: Arc<AtomicU64>,
    pub total_fees: Arc<AtomicU64>,
    pub trade_count: Arc<AtomicU64>,
    
    // 时间戳 - 使用原子操作存储epoch millis
    pub created_at_ms: Arc<AtomicU64>,
    pub first_buy_at_ms: Arc<AtomicU64>,
    pub last_trade_at_ms: Arc<AtomicU64>,
}

impl OptimizedPosition {
    pub fn new(mint: Pubkey, strategy_id: String) -> Self {
        let now_ms = chrono::Utc::now().timestamp_millis() as u64;
        
        Self {
            strategy_id,
            mint,
            status: Arc::new(AtomicU8::new(OptimizedPositionStatus::Empty as u8)),
            token_amount: Arc::new(AtomicU64::new(0)),
            sol_invested: Arc::new(AtomicU64::new(0)),
            sol_returned: Arc::new(AtomicU64::new(0)),
            total_fees: Arc::new(AtomicU64::new(0)),
            trade_count: Arc::new(AtomicU64::new(0)),
            created_at_ms: Arc::new(AtomicU64::new(now_ms)),
            first_buy_at_ms: Arc::new(AtomicU64::new(0)),
            last_trade_at_ms: Arc::new(AtomicU64::new(0)),
        }
    }
    
    /// 原子操作记录买入
    pub fn record_buy_atomic(&self, sol_amount: u64, token_amount: u64, result: &ExecutionResult) {
        let now_ms = chrono::Utc::now().timestamp_millis() as u64;
        
        // 更新状态
        self.status.store(OptimizedPositionStatus::Holding as u8, Ordering::Release);
        
        // 更新数量和金额 - 原子操作
        self.token_amount.fetch_add(token_amount, Ordering::AcqRel);
        self.sol_invested.fetch_add(sol_amount, Ordering::AcqRel);
        self.total_fees.fetch_add(result.actual_fee_paid, Ordering::AcqRel);
        self.trade_count.fetch_add(1, Ordering::AcqRel);
        
        // 更新时间戳
        self.last_trade_at_ms.store(now_ms, Ordering::Release);
        
        // 如果是第一次买入，记录首次买入时间
        self.first_buy_at_ms.compare_exchange(0, now_ms, Ordering::AcqRel, Ordering::Acquire).ok();
        
        debug!("📊 原子操作记录买入: SOL={}, TOKEN={}, 费用={}", 
            sol_amount, token_amount, result.actual_fee_paid);
    }
    
    /// 原子操作记录卖出
    pub fn record_sell_atomic(&self, sol_received: u64, token_amount: u64, result: &ExecutionResult) {
        let now_ms = chrono::Utc::now().timestamp_millis() as u64;
        
        // 更新数量和金额 - 原子操作
        let remaining_tokens = self.token_amount.fetch_sub(token_amount.min(self.token_amount.load(Ordering::Acquire)), Ordering::AcqRel);
        self.sol_returned.fetch_add(sol_received, Ordering::AcqRel);
        self.total_fees.fetch_add(result.actual_fee_paid, Ordering::AcqRel);
        self.trade_count.fetch_add(1, Ordering::AcqRel);
        
        // 更新时间戳
        self.last_trade_at_ms.store(now_ms, Ordering::Release);
        
        // 如果完全卖出，更新状态
        if remaining_tokens <= token_amount {
            self.status.store(OptimizedPositionStatus::Closed as u8, Ordering::Release);
        }
        
        debug!("📊 原子操作记录卖出: SOL={}, TOKEN={}, 剩余TOKEN={}", 
            sol_received, token_amount, remaining_tokens.saturating_sub(token_amount));
    }
    
    /// 无锁获取当前状态快照
    pub fn get_status_snapshot(&self) -> OptimizedPositionStatus {
        OptimizedPositionStatus::from(self.status.load(Ordering::Acquire))
    }
    
    /// 无锁检查是否有持仓
    pub fn has_position(&self) -> bool {
        self.token_amount.load(Ordering::Acquire) > 0
    }
    
    /// 无锁检查是否已关闭
    pub fn is_closed(&self) -> bool {
        matches!(self.get_status_snapshot(), OptimizedPositionStatus::Closed)
    }
    
    /// 无锁获取盈亏情况
    pub fn get_pnl_lamports(&self) -> i64 {
        let invested = self.sol_invested.load(Ordering::Acquire) as i64;
        let returned = self.sol_returned.load(Ordering::Acquire) as i64;
        let fees = self.total_fees.load(Ordering::Acquire) as i64;
        returned - invested - fees
    }
    
    /// 设置仓位状态 - 原子操作
    pub fn set_status(&self, status: OptimizedPositionStatus) {
        self.status.store(status as u8, Ordering::Release);
    }
    
    /// 获取持仓时长（毫秒）
    pub fn get_holding_duration_ms(&self) -> u64 {
        let first_buy = self.first_buy_at_ms.load(Ordering::Acquire);
        if first_buy == 0 {
            return 0;
        }
        let now_ms = chrono::Utc::now().timestamp_millis() as u64;
        now_ms.saturating_sub(first_buy)
    }
    
    /// 打印仓位摘要 - 无锁操作
    pub fn print_summary(&self, strategy_runtime: Option<Duration>) {
        let status = self.get_status_snapshot();
        let token_amount = self.token_amount.load(Ordering::Acquire);
        let sol_invested = self.sol_invested.load(Ordering::Acquire);
        let sol_returned = self.sol_returned.load(Ordering::Acquire);
        let total_fees = self.total_fees.load(Ordering::Acquire);
        let trade_count = self.trade_count.load(Ordering::Acquire);
        let pnl = self.get_pnl_lamports();
        
        info!("📊 优化仓位摘要 - 策略: {}", self.strategy_id);
        info!("   🪙 代币: {}", self.mint);
        info!("   📈 状态: {:?}", status);
        info!("   💰 持仓数量: {} tokens", token_amount);
        info!("   💸 投入SOL: {:.4}", sol_invested as f64 / 1_000_000_000.0);
        info!("   💰 回收SOL: {:.4}", sol_returned as f64 / 1_000_000_000.0);
        info!("   💸 总费用: {:.4} SOL", total_fees as f64 / 1_000_000_000.0);
        info!("   📊 交易次数: {}", trade_count);
        info!("   📈 盈亏: {:.4} SOL ({})", 
            pnl as f64 / 1_000_000_000.0,
            if pnl >= 0 { "盈利" } else { "亏损" }
        );
        
        if let Some(runtime) = strategy_runtime {
            info!("   ⏱️ 策略运行时长: {:.1}秒", runtime.as_secs_f64());
        }
        
        let holding_duration_ms = self.get_holding_duration_ms();
        if holding_duration_ms > 0 {
            info!("   ⏱️ 持仓时长: {:.1}秒", holding_duration_ms as f64 / 1000.0);
        }
    }
}

/// 优化后的交易策略 - 高性能无锁版本
/// 
/// 关键优化点：
/// 1. 使用原子操作替代 RwLock，消除锁竞争
/// 2. 状态管理完全无锁，支持高并发访问
/// 3. 资源使用最小化，避免每策略独立timer
/// 4. 与优化版策略管理器完美集成
/// 5. 🔧 新增：支持真实价格信息进行精确交易
pub struct OptimizedTradingStrategy {
    /// 策略唯一ID
    pub id: String,
    /// 代币mint地址
    pub mint: Pubkey,
    /// 策略配置
    pub config: StrategyConfig,
    
    // 原子状态管理 - 无锁并发访问
    status: Arc<AtomicU8>, // OptimizedStrategyStatus
    
    /// 优化后的仓位信息
    position: Arc<OptimizedPosition>,
    
    /// 交易信号发送器
    signal_sender: mpsc::UnboundedSender<TradeSignal>,
    
    /// 策略开始时间戳 (epoch millis)
    start_time_ms: Arc<AtomicU64>,
    
    /// 买入完成时间戳 (epoch millis) - 用于计算持仓时长
    buy_completed_at_ms: Arc<AtomicU64>,
    
    /// 取消令牌发送器 (用于停止策略)
    cancel_sender: Arc<tokio::sync::Mutex<Option<mpsc::UnboundedSender<()>>>>,
    
    /// 性能统计计数器
    pub performance_stats: Arc<OptimizedStrategyStats>,
    
    /// 🔧 新增：当前价格信息 (SOL per token)
    current_price: Arc<tokio::sync::RwLock<Option<f64>>>,
    
    /// 🔧 新增：价格来源信息
    price_source: Arc<tokio::sync::RwLock<Option<String>>>,
    
    /// 🔧 新增：代币创建者地址
    creator: Arc<tokio::sync::RwLock<Option<Pubkey>>>,
    
    /// 🔧 修复：策略停止通知发送器 - 用于通知策略管理器移除策略
    strategy_stop_notifier: Arc<tokio::sync::Mutex<Option<mpsc::UnboundedSender<Pubkey>>>>,
}

/// 优化策略性能统计 - 原子计数器
#[derive(Debug, Default)]
pub struct OptimizedStrategyStats {
    pub events_processed: Arc<AtomicU64>,
    pub signals_sent: Arc<AtomicU64>,
    pub execution_results_handled: Arc<AtomicU64>,
    pub state_changes: Arc<AtomicU64>,
    pub lock_free_operations: Arc<AtomicU64>,
}

impl OptimizedStrategyStats {
    pub fn print(&self) {
        let events = self.events_processed.load(Ordering::Acquire);
        let signals = self.signals_sent.load(Ordering::Acquire);
        let results = self.execution_results_handled.load(Ordering::Acquire);
        let changes = self.state_changes.load(Ordering::Acquire);
        let lock_free = self.lock_free_operations.load(Ordering::Acquire);
        
        info!("📊 优化策略性能统计:");
        info!("   🔄 处理事件数: {}", events);
        info!("   📤 发送信号数: {}", signals);
        info!("   📨 处理执行结果: {}", results);
        info!("   🔄 状态变更: {}", changes);
        info!("   🚀 无锁操作数: {}", lock_free);
    }
}

impl OptimizedTradingStrategy {
    /// 创建新的优化交易策略
    pub fn new(
        mint: Pubkey,
        config: StrategyConfig,
        signal_sender: mpsc::UnboundedSender<TradeSignal>,
    ) -> Self {
        let strategy_id = format!("opt_strategy_{}_{}", 
            mint.to_string()[..8].to_string(),
            chrono::Utc::now().timestamp_millis()
        );

        Self {
            id: strategy_id.clone(),
            mint,
            config,
            status: Arc::new(AtomicU8::new(OptimizedStrategyStatus::Initializing as u8)),
            position: Arc::new(OptimizedPosition::new(mint, strategy_id)),
            signal_sender,
            start_time_ms: Arc::new(AtomicU64::new(0)),
            buy_completed_at_ms: Arc::new(AtomicU64::new(0)),
            cancel_sender: Arc::new(tokio::sync::Mutex::new(None)),
            performance_stats: Arc::new(OptimizedStrategyStats::default()),
            current_price: Arc::new(tokio::sync::RwLock::new(None)),
            price_source: Arc::new(tokio::sync::RwLock::new(None)),
            creator: Arc::new(tokio::sync::RwLock::new(None)),
            strategy_stop_notifier: Arc::new(tokio::sync::Mutex::new(None)),
        }
    }

    /// 🔧 新增：创建带价格和创建者信息的优化交易策略
    pub fn new_with_price_and_creator(
        mint: Pubkey,
        config: StrategyConfig,
        signal_sender: mpsc::UnboundedSender<TradeSignal>,
        price_info: Option<(f64, String)>,
        creator: Option<Pubkey>,
    ) -> Self {
        let strategy_id = format!("opt_strategy_{}_{}", 
            mint.to_string()[..8].to_string(),
            chrono::Utc::now().timestamp_millis()
        );

        let (initial_price, initial_source) = if let Some((price, source)) = price_info {
            (Some(price), Some(source))
        } else {
            (None, None)
        };

        Self {
            id: strategy_id.clone(),
            mint,
            config,
            status: Arc::new(AtomicU8::new(OptimizedStrategyStatus::Initializing as u8)),
            position: Arc::new(OptimizedPosition::new(mint, strategy_id)),
            signal_sender,
            start_time_ms: Arc::new(AtomicU64::new(0)),
            buy_completed_at_ms: Arc::new(AtomicU64::new(0)),
            cancel_sender: Arc::new(tokio::sync::Mutex::new(None)),
            performance_stats: Arc::new(OptimizedStrategyStats::default()),
            current_price: Arc::new(tokio::sync::RwLock::new(initial_price)),
            price_source: Arc::new(tokio::sync::RwLock::new(initial_source)),
            creator: Arc::new(tokio::sync::RwLock::new(creator)),
            strategy_stop_notifier: Arc::new(tokio::sync::Mutex::new(None)),
        }
    }
    pub fn new_with_price(
        mint: Pubkey,
        config: StrategyConfig,
        signal_sender: mpsc::UnboundedSender<TradeSignal>,
        price_info: Option<(f64, String)>,
    ) -> Self {
        let strategy_id = format!("opt_strategy_{}_{}", 
            mint.to_string()[..8].to_string(),
            chrono::Utc::now().timestamp_millis()
        );

        let (initial_price, initial_source) = if let Some((price, source)) = price_info {
            (Some(price), Some(source))
        } else {
            (None, None)
        };

        Self {
            id: strategy_id.clone(),
            mint,
            config,
            status: Arc::new(AtomicU8::new(OptimizedStrategyStatus::Initializing as u8)),
            position: Arc::new(OptimizedPosition::new(mint, strategy_id)),
            signal_sender,
            start_time_ms: Arc::new(AtomicU64::new(0)),
            buy_completed_at_ms: Arc::new(AtomicU64::new(0)),
            cancel_sender: Arc::new(tokio::sync::Mutex::new(None)),
            performance_stats: Arc::new(OptimizedStrategyStats::default()),
            current_price: Arc::new(tokio::sync::RwLock::new(initial_price)),
            price_source: Arc::new(tokio::sync::RwLock::new(initial_source)),
            creator: Arc::new(tokio::sync::RwLock::new(None)),
            strategy_stop_notifier: Arc::new(tokio::sync::Mutex::new(None)),
        }
    }

    /// 🔧 新增：更新价格信息
    pub async fn update_price(&self, price: f64, source: String) {
        {
            let mut current_price = self.current_price.write().await;
            *current_price = Some(price);
        }
        {
            let mut price_source = self.price_source.write().await;
            *price_source = Some(source);
        }
        info!("📊 策略 {} 价格更新: {:.9} SOL/token", self.id, price);
    }

    /// 🔧 新增：获取当前价格信息
    pub async fn get_current_price(&self) -> Option<(f64, String)> {
        let price = {
            let current_price = self.current_price.read().await;
            *current_price
        };
        let source = {
            let price_source = self.price_source.read().await;
            price_source.clone()
        };
        
        if let (Some(price), Some(source)) = (price, source) {
            Some((price, source))
        } else {
            None
        }
    }

    /// 🔧 新增：获取创建者地址
    pub async fn get_creator(&self) -> Option<Pubkey> {
        let creator = self.creator.read().await;
        *creator
    }

    /// 🔧 新增：设置创建者地址
    pub async fn set_creator(&self, creator: Pubkey) {
        let mut creator_lock = self.creator.write().await;
        *creator_lock = Some(creator);
        info!("👤 策略 {} 设置创建者地址: {}", self.id, creator);
    }

    /// 🔧 修复：设置策略停止通知发送器
    pub async fn set_strategy_stop_notifier(&self, notifier: mpsc::UnboundedSender<Pubkey>) {
        let mut notifier_lock = self.strategy_stop_notifier.lock().await;
        *notifier_lock = Some(notifier);
        info!("📨 策略 {} 设置停止通知发送器", self.id);
    }

    /// 启动优化策略 - 资源高效版本
    pub async fn run(&self) -> Result<()> {
        info!("🚀 启动优化交易策略: {}", self.id);
        info!("   🪙 代币地址: {}", self.mint);
        info!("   💰 买入金额: {:.4} SOL", self.config.buy_amount_lamports as f64 / 1_000_000_000.0);
        info!("   📊 最大滑点: {}%", self.config.max_slippage_bps as f64 / 100.0);
        info!("   ⏱️ 持仓时长: {}秒", self.config.holding_duration_seconds);
        info!("   🚀 使用优化架构: 原子操作 + 无锁并发");

        // 原子更新状态
        self.status.store(OptimizedStrategyStatus::Running as u8, Ordering::Release);
        let now_ms = chrono::Utc::now().timestamp_millis() as u64;
        self.start_time_ms.store(now_ms, Ordering::Release);
        
        self.performance_stats.state_changes.fetch_add(1, Ordering::Relaxed);

        // 创建取消通道
        let (cancel_tx, mut cancel_rx) = mpsc::unbounded_channel();
        {
            let mut cancel_sender = self.cancel_sender.lock().await;
            *cancel_sender = Some(cancel_tx);
        }

        // 发送买入信号
        self.send_buy_signal_atomic("优化版初始买入信号").await?;

        // 启动轻量级监控循环 - 共享资源，避免每策略独立timer
        let strategy_clone = Arc::new(OptimizedStrategyHandle {
            id: self.id.clone(),
            mint: self.mint,
            config: self.config.clone(),
            status: self.status.clone(),
            position: self.position.clone(),
            signal_sender: self.signal_sender.clone(),
            buy_completed_at_ms: self.buy_completed_at_ms.clone(),
            performance_stats: self.performance_stats.clone(),
            // 🔧 新增：传递价格信息
            current_price: self.current_price.clone(),
            price_source: self.price_source.clone(),
            // 🔧 修复：传递创建者地址
            creator: self.creator.clone(),
        });

        tokio::spawn(async move {
            let mut check_interval = tokio::time::interval(Duration::from_secs(1));
            
            loop {
                tokio::select! {
                    _ = check_interval.tick() => {
                        // 无锁检查策略状态
                        let current_status = OptimizedStrategyStatus::from(strategy_clone.status.load(Ordering::Acquire));
                        match current_status {
                            OptimizedStrategyStatus::Stopping | OptimizedStrategyStatus::Stopped => {
                                info!("⏹️ 优化策略 {} 正在停止", strategy_clone.id);
                                break;
                            }
                            OptimizedStrategyStatus::Running => {
                                // 无锁检查是否需要执行卖出
                                strategy_clone.check_sell_condition().await;
                            }
                            _ => continue,
                        }
                    }
                    _ = cancel_rx.recv() => {
                        info!("📨 优化策略 {} 收到取消信号", strategy_clone.id);
                        break;
                    }
                }
            }
            
            // 原子更新状态为已停止
            strategy_clone.status.store(OptimizedStrategyStatus::Stopped as u8, Ordering::Release);
            strategy_clone.performance_stats.state_changes.fetch_add(1, Ordering::Relaxed);
            info!("✅ 优化策略 {} 已停止", strategy_clone.id);
        });

        Ok(())
    }

    /// 停止优化策略 - 原子操作版本
    pub async fn stop(&self) -> Result<()> {
        info!("⏹️ 停止优化交易策略: {}", self.id);

        // 原子更新状态
        self.status.store(OptimizedStrategyStatus::Stopping as u8, Ordering::Release);
        self.performance_stats.state_changes.fetch_add(1, Ordering::Relaxed);

        // 发送取消信号
        {
            let cancel_sender = self.cancel_sender.lock().await;
            if let Some(sender) = cancel_sender.as_ref() {
                let _ = sender.send(());
            }
        }

        // 如果还有持仓，发送紧急卖出信号
        if self.position.has_position() {
            let status = self.position.get_status_snapshot();
            if matches!(status, OptimizedPositionStatus::Holding) {
                warn!("⚠️ 优化策略停止时仍有持仓，发送紧急卖出信号");
                
                let token_amount = self.position.token_amount.load(Ordering::Acquire);
                
                // 🔧 修改：策略停止时直接触发紧急卖出，不需要等待价格信息
                let emergency_signal = if let Some((price, source)) = self.get_current_price().await {
                    // ✅ 有价格信息时使用真实价格创建紧急卖出信号
                    info!("💰 使用真实价格创建紧急卖出信号: {:.9} SOL/token (来源: {})", price, source);
                    let mut signal = TradeSignal::emergency_sell_with_price(
                        self.id.clone(),
                        self.mint,
                        token_amount,
                        "优化策略停止时的紧急平仓".to_string(),
                        price,
                        source,
                    );
                    
                    // 🔧 修复：设置创建者地址
                    if let Some(creator) = self.get_creator().await {
                        signal = signal.with_creator(creator);
                    }
                    signal
                } else {
                    // ✅ 没有价格信息时直接创建无价格紧急卖出信号
                    warn!("⚠️ 策略停止时无法获取价格信息，创建无价格紧急卖出信号");
                    info!("   💡 将使用极高滑点容忍度确保紧急平仓执行");
                    let mut signal = TradeSignal::emergency_sell_without_price(
                        self.id.clone(),
                        self.mint,
                        token_amount,
                        "策略停止时无价格紧急平仓".to_string(),
                    );
                    
                    // 🔧 修复：设置创建者地址
                    if let Some(creator) = self.get_creator().await {
                        signal = signal.with_creator(creator);
                    }
                    signal
                };

                if let Err(e) = self.signal_sender.send(emergency_signal) {
                    error!("❌ 发送紧急卖出信号失败: {}", e);
                } else {
                    self.performance_stats.signals_sent.fetch_add(1, Ordering::Relaxed);
                }
            }
        }

        Ok(())
    }

    /// 销毁策略 (清理资源) - 优化版本
    pub async fn destroy(&self) -> Result<()> {
        info!("🗑️ 销毁优化交易策略: {}", self.id);

        // 首先停止策略
        self.stop().await?;

        // 等待一段时间确保停止完成
        tokio::time::sleep(Duration::from_millis(500)).await; // 缩短等待时间

        // 打印最终统计
        let start_time_ms = self.start_time_ms.load(Ordering::Acquire);
        let runtime = if start_time_ms > 0 {
            let now_ms = chrono::Utc::now().timestamp_millis() as u64;
            Some(Duration::from_millis(now_ms - start_time_ms))
        } else {
            None
        };

        self.position.print_summary(runtime);
        self.performance_stats.print();
        
        if !self.position.is_closed() {
            warn!("⚠️ 优化策略销毁时仓位未完全平仓!");
        }

        info!("✅ 优化策略 {} 已销毁", self.id);
        Ok(())
    }

    /// 接收交易执行结果事件 - 优化版本
    pub async fn handle_execution_result(&self, result: &ExecutionResult, is_buy: bool, sol_amount: u64, token_amount: u64) -> Result<()> {
        info!("📨 优化策略 {} 接收到交易执行结果", self.id);
        info!("   📝 签名: {}", result.signature);
        info!("   ✅ 成功: {}", result.success);
        info!("   📊 交易类型: {}", if is_buy { "买入" } else { "卖出" });

        self.performance_stats.execution_results_handled.fetch_add(1, Ordering::Relaxed);
        self.performance_stats.lock_free_operations.fetch_add(1, Ordering::Relaxed);

        if !result.success {
            warn!("❌ 交易执行失败，策略进入错误状态");
            self.status.store(OptimizedStrategyStatus::Error as u8, Ordering::Release);
            self.performance_stats.state_changes.fetch_add(1, Ordering::Relaxed);
            return Ok(());
        }

        // 使用原子操作更新仓位信息
        if is_buy {
            self.position.record_buy_atomic(sol_amount, token_amount, result);
            
            // 记录买入完成时间
            let now_ms = chrono::Utc::now().timestamp_millis() as u64;
            self.buy_completed_at_ms.store(now_ms, Ordering::Release);
            
            info!("✅ 优化版买入交易完成，开始计时持仓时长");
        } else {
            self.position.record_sell_atomic(sol_amount, token_amount, result);
            
            info!("✅ 优化版卖出交易完成");
            
            // 🔧 改进：更精确的平仓检查逻辑
            let remaining_tokens = self.position.token_amount.load(Ordering::Acquire);
            let position_status = self.position.get_status_snapshot();
            
            info!("📊 卖出后仓位检查: 剩余代币={}, 状态={:?}", remaining_tokens, position_status);
            
            // 如果完全平仓或仓位已关闭，策略可以结束
            if remaining_tokens == 0 || matches!(position_status, OptimizedPositionStatus::Closed) {
                info!("🎯 仓位已完全平仓，优化策略即将结束");
                
                // 异步触发策略停止
                let notifier = {
                    let notifier_lock = self.strategy_stop_notifier.lock().await;
                    notifier_lock.clone()
                };
                let strategy_handle = OptimizedStrategyStopHandle {
                    mint: self.mint,
                    status: self.status.clone(),
                    cancel_sender: self.cancel_sender.clone(),
                    strategy_stop_notifier: notifier,
                };
                
                tokio::spawn(async move {
                    tokio::time::sleep(Duration::from_secs(2)).await; // 等待2秒后自动停止
                    strategy_handle.trigger_stop().await;
                });
            } else {
                info!("📊 仍有部分仓位剩余: {} tokens", remaining_tokens);
            }
        }

        Ok(())
    }

    /// 接收代币事件 (用于价格监控等) - 优化版本
    pub async fn handle_token_event(&self, event: &TokenEvent) -> Result<()> {
        // 只处理与本策略代币相关的事件
        if let Some(event_mint) = &event.mint {
            if event_mint != &self.mint.to_string() {
                return Ok(()); // 不是本策略的代币，忽略
            }
        } else {
            return Ok(());
        }

        self.performance_stats.events_processed.fetch_add(1, Ordering::Relaxed);
        self.performance_stats.lock_free_operations.fetch_add(1, Ordering::Relaxed);

        debug!("📊 优化策略 {} 监控到相关代币事件", self.id);
        debug!("   🔍 事件类型: {:?}", event.transaction_type);
        if let Some(sol_amount) = event.sol_amount {
            debug!("   💰 涉及金额: {:.4} SOL", sol_amount as f64 / 1_000_000_000.0);
        }

        // 这里可以根据事件类型和金额大小做出反应
        // 例如：如果检测到大额卖出，可能触发紧急卖出
        // 由于是无锁架构，可以高频处理这类事件
        
        Ok(())
    }

    /// 发送买入信号 - 🔧 改进版：统一使用真实价格信息和创建者信息
    async fn send_buy_signal_atomic(&self, reason: &str) -> Result<()> {
        // 获取价格信息
        let price_info = self.get_current_price().await;
        // 获取创建者信息
        let creator = self.get_creator().await;

        let buy_signal = if let (Some((price, source)), Some(creator_addr)) = (&price_info, creator) {
            // ✅ 使用真实价格和创建者信息创建买入信号
            info!("💰 使用真实价格和创建者创建买入信号:");
            info!("   💰 价格: {:.9} SOL/token (来源: {})", price, source);
            info!("   👤 创建者: {}", creator_addr);
            TradeSignal::buy_with_price_and_creator(
                self.id.clone(),
                self.mint,
                self.config.buy_amount_lamports,
                self.config.max_slippage_bps,
                reason.to_string(),
                *price,
                source.clone(),
                creator_addr,
            )
        } else {
            // ❌ 缺少必要信息时拒绝创建买入信号
            error!("❌ 策略 {} 缺少必要信息，无法创建买入信号", self.id);
            if price_info.is_none() {
                error!("   ❌ 缺少价格信息");
            }
            if creator.is_none() {
                error!("   ❌ 缺少创建者地址");
            }
            error!("   💡 请确保策略管理器正确传递价格和创建者信息");
            return Err(anyhow::anyhow!("缺少价格或创建者信息，买入信号创建失败"));
        };

        // 设置为高优先级 - 新币狙击需要快速执行
        let buy_signal = buy_signal.with_priority(SignalPriority::High);

        self.signal_sender.send(buy_signal)
            .map_err(|e| anyhow::anyhow!("发送买入信号失败: {}", e))?;

        self.performance_stats.signals_sent.fetch_add(1, Ordering::Relaxed);
        self.performance_stats.lock_free_operations.fetch_add(1, Ordering::Relaxed);

        // 原子更新仓位状态
        self.position.set_status(OptimizedPositionStatus::Buying);

        info!("📤 优化版发送买入信号: {}", reason);
        Ok(())
    }

    /// 获取策略状态 - 原子操作，无锁
    pub fn get_status(&self) -> OptimizedStrategyStatus {
        OptimizedStrategyStatus::from(self.status.load(Ordering::Acquire))
    }

    /// 获取仓位信息 - 无锁访问
    pub fn get_position(&self) -> Arc<OptimizedPosition> {
        self.position.clone()
    }

    /// 获取策略运行时长 - 原子操作
    pub fn get_runtime(&self) -> Option<Duration> {
        let start_time_ms = self.start_time_ms.load(Ordering::Acquire);
        if start_time_ms == 0 {
            return None;
        }
        let now_ms = chrono::Utc::now().timestamp_millis() as u64;
        Some(Duration::from_millis(now_ms - start_time_ms))
    }

    /// 暂停策略 - 原子操作
    pub async fn pause(&self) -> Result<()> {
        info!("⏸️ 暂停优化交易策略: {}", self.id);
        self.status.store(OptimizedStrategyStatus::Paused as u8, Ordering::Release);
        self.performance_stats.state_changes.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }

    /// 恢复策略 - 原子操作
    pub async fn resume(&self) -> Result<()> {
        info!("▶️ 恢复优化交易策略: {}", self.id);
        self.status.store(OptimizedStrategyStatus::Running as u8, Ordering::Release);
        self.performance_stats.state_changes.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }

    /// 获取策略摘要信息 - 无锁操作
    pub async fn get_summary(&self) -> String {
        let status = self.get_status();
        let token_amount = self.position.token_amount.load(Ordering::Acquire);
        let runtime = self.get_runtime()
            .map(|d| format!("{:.1}秒", d.as_secs_f64()))
            .unwrap_or_else(|| "未运行".to_string());

        format!(
            "优化策略 {} | 状态: {:?} | 代币: {} | 持仓: {} | 运行时间: {}",
            self.id,
            status,
            self.mint.to_string()[..8].to_string(),
            token_amount,
            runtime
        )
    }
}

/// 优化策略句柄 - 用于异步任务中的策略控制
#[derive(Clone)]
struct OptimizedStrategyHandle {
    id: String,
    mint: Pubkey,
    config: StrategyConfig,
    status: Arc<AtomicU8>,
    position: Arc<OptimizedPosition>,
    signal_sender: mpsc::UnboundedSender<TradeSignal>,
    buy_completed_at_ms: Arc<AtomicU64>,
    performance_stats: Arc<OptimizedStrategyStats>,
    // 🔧 新增：价格信息访问
    current_price: Arc<tokio::sync::RwLock<Option<f64>>>,
    price_source: Arc<tokio::sync::RwLock<Option<String>>>,
    // 🔧 修复：新增创建者地址访问
    creator: Arc<tokio::sync::RwLock<Option<Pubkey>>>,
}

impl OptimizedStrategyHandle {
    /// 获取创建者地址
    async fn get_creator(&self) -> Option<Pubkey> {
        let creator = self.creator.read().await;
        *creator
    }

    /// 无锁检查卖出条件
    async fn check_sell_condition(&self) {
        let position_status = self.position.get_status_snapshot();
        if !matches!(position_status, OptimizedPositionStatus::Holding) {
            return;
        }

        let buy_completed_at_ms = self.buy_completed_at_ms.load(Ordering::Acquire);
        if buy_completed_at_ms == 0 {
            return;
        }

        let now_ms = chrono::Utc::now().timestamp_millis() as u64;
        let holding_duration_ms = now_ms - buy_completed_at_ms;
        let required_duration_ms = self.config.holding_duration_seconds * 1000;

        if holding_duration_ms >= required_duration_ms {
            info!("⏰ 优化策略 {} 达到持仓时长 {}秒，发送紧急卖出信号", 
                self.id, self.config.holding_duration_seconds);
            
            let token_amount = self.position.token_amount.load(Ordering::Acquire);
            
            // 🔧 简化：单点持仓检查 - 唯一的持仓验证点
            if token_amount == 0 {
                warn!("⚠️ 策略 {} 达到持仓时长但代币数量为0，跳过卖出", self.id);
                warn!("   💡 这是正常情况，可能买入失败或已经卖出完毕");
                return;
            }
            
            info!("📊 准备紧急卖出: {} tokens", token_amount);
            
            // 🔧 修改：持仓时间到期后直接触发紧急卖出，不需要等待价格信息
            let sell_signal = if let (Some(price), Some(source)) = self.get_current_price_info().await {
                // ✅ 有价格信息时使用真实价格创建紧急卖出信号
                info!("💰 使用真实价格创建紧急卖出信号: {:.9} SOL/token (来源: {})", price, source);
                let mut signal = TradeSignal::emergency_sell_with_price(
                    self.id.clone(),
                    self.mint,
                    token_amount,
                    format!("持仓{}秒后定时紧急卖出", self.config.holding_duration_seconds),
                    price,
                    source,
                );
                
                // 🔧 修复：设置创建者地址
                if let Some(creator) = self.get_creator().await {
                    signal = signal.with_creator(creator);
                }
                signal
            } else {
                // ✅ 没有价格信息时直接创建无价格紧急卖出信号
                warn!("⚠️ 策略 {} 缺少价格信息，创建无价格紧急卖出信号", self.id);
                info!("   💡 将使用极高滑点容忍度确保交易执行");
                let mut signal = TradeSignal::emergency_sell_without_price(
                    self.id.clone(),
                    self.mint,
                    token_amount,
                    format!("持仓{}秒后无价格紧急卖出", self.config.holding_duration_seconds),
                );
                
                // 🔧 修复：设置创建者地址
                if let Some(creator) = self.get_creator().await {
                    signal = signal.with_creator(creator);
                }
                signal
            };

            if let Err(e) = self.signal_sender.send(sell_signal) {
                error!("❌ 发送紧急卖出信号失败: {}", e);
            } else {
                self.performance_stats.signals_sent.fetch_add(1, Ordering::Relaxed);
                
                // 更新仓位状态为卖出中
                self.position.set_status(OptimizedPositionStatus::Selling);
            }
        }
    }

    /// 🔧 新增：获取当前价格信息
    async fn get_current_price_info(&self) -> (Option<f64>, Option<String>) {
        let price = {
            let current_price = self.current_price.read().await;
            *current_price
        };
        let source = {
            let price_source = self.price_source.read().await;
            price_source.clone()
        };
        
        (price, source)
    }
}

/// 优化策略停止句柄
struct OptimizedStrategyStopHandle {
    mint: Pubkey,
    status: Arc<AtomicU8>,
    cancel_sender: Arc<tokio::sync::Mutex<Option<mpsc::UnboundedSender<()>>>>,
    /// 策略停止通知发送器 - 用于通知策略管理器移除策略
    strategy_stop_notifier: Option<mpsc::UnboundedSender<Pubkey>>,
}

impl OptimizedStrategyStopHandle {
    async fn trigger_stop(&self) {
        self.status.store(OptimizedStrategyStatus::Stopping as u8, Ordering::Release);
        
        let cancel_sender = self.cancel_sender.lock().await;
        if let Some(sender) = cancel_sender.as_ref() {
            let _ = sender.send(());
        }
        
        // 🔧 修复：通知策略管理器移除策略
        if let Some(ref notifier) = self.strategy_stop_notifier {
            if let Err(e) = notifier.send(self.mint) {
                error!("❌ 发送策略停止通知失败: {}", e);
            } else {
                info!("📨 已通知策略管理器移除策略: {}", self.mint);
            }
        }
    }
}
