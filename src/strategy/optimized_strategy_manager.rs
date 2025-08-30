use anyhow::Result;
use dashmap::DashMap;
use log::{info, warn, error};
use solana_sdk::{pubkey::Pubkey, signature::Signature, signer::Signer};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::str::FromStr;
use tokio::sync::mpsc;

use crate::processors::TokenEvent;
use crate::executor::ExecutionResult;
use crate::executor::optimized_executor_manager::OptimizedExecutorManager;
use crate::executor::traits::TransactionExecutor;
use crate::executor::compute_budget::{DynamicComputeBudgetManager, ComputeBudgetTier};
use crate::utils::TokenBalanceClient;
use super::optimized_token_filter::OptimizedTokenFilter;
use super::StrategyConfig;
use super::{TradeSignal, TradeSignalType};
use super::optimized_trading_strategy::{OptimizedTradingStrategy, OptimizedPosition};

/// 优化后的策略管理器
/// 
/// 关键优化点：
/// 1. 使用 DashMap 替代 RwLock<HashMap>，实现无锁并发访问
/// 2. 使用原子计数器跟踪策略数量，避免锁竞争
/// 3. TokenFilter 变为无状态，只需要 Arc 包装
/// 4. 细粒度的并发控制，提升吞吐量
/// 5. 集成动态计算预算管理器
pub struct OptimizedStrategyManager {
    /// 策略存储 - 使用 DashMap 实现无锁并发访问
    strategies: Arc<DashMap<Pubkey, Arc<OptimizedTradingStrategy>>>,
    
    /// 策略计数器 - 原子操作，无锁
    strategy_count: Arc<AtomicUsize>,
    
    /// 交易信号发送器
    signal_sender: mpsc::UnboundedSender<TradeSignal>,
    
    /// 默认策略配置
    default_config: StrategyConfig,
    
    /// 最大并发策略数量
    max_concurrent_strategies: usize,
    
    /// 无状态代币过滤器（无需锁保护）
    token_filter: Arc<OptimizedTokenFilter>,
    
    /// 代币余额查询客户端 - 用于获取准确的代币数量
    token_balance_client: Option<Arc<TokenBalanceClient>>,
    
    /// 🆕 新增：动态计算预算管理器
    compute_budget_manager: Option<Arc<DynamicComputeBudgetManager>>,
    
    /// 🔧 修复：策略停止通知发送器 - 用于接收策略自动停止通知
    strategy_stop_sender: mpsc::UnboundedSender<Pubkey>,
}

impl OptimizedStrategyManager {
    /// 创建优化的策略管理器
    pub fn new(
        executor_manager: Option<Arc<OptimizedExecutorManager>>,
        default_config: Option<StrategyConfig>,
        max_concurrent_strategies: Option<usize>,
        token_filter: OptimizedTokenFilter,
        compute_budget_manager: Option<Arc<DynamicComputeBudgetManager>>, // 🆕 新增参数
    ) -> Arc<Self> {
        let (signal_tx, mut signal_rx) = mpsc::unbounded_channel();
        
        // 🔧 修复：创建策略停止通知通道
        let (strategy_stop_tx, mut strategy_stop_rx) = mpsc::unbounded_channel();
        
        // 尝试创建代币余额查询客户端
        let token_balance_client = match TokenBalanceClient::from_env() {
            Ok(client) => {
                info!("✅ 代币余额查询客户端初始化成功");
                Some(Arc::new(client))
            }
            Err(e) => {
                warn!("⚠️ 代币余额查询客户端初始化失败: {}", e);
                warn!("   将使用占位值作为代币数量，可能影响策略准确性");
                None
            }
        };
        
        // 🆕 记录计算预算管理器状态
        if let Some(ref cb_manager) = compute_budget_manager {
            info!("✅ 计算预算管理器已集成到策略管理器");
            let (buy_valid, sell_valid) = cb_manager.get_cache_status();
            info!("   缓存状态: 买入={}, 卖出={}", buy_valid, sell_valid);
        } else {
            warn!("⚠️ 未提供计算预算管理器，将使用默认预算设置");
        }
        
        let manager = Arc::new(Self {
            strategies: Arc::new(DashMap::new()),
            strategy_count: Arc::new(AtomicUsize::new(0)),
            signal_sender: signal_tx,
            default_config: default_config.unwrap_or_default(),
            max_concurrent_strategies: max_concurrent_strategies.unwrap_or(10),
            token_filter: Arc::new(token_filter),
            token_balance_client,
            compute_budget_manager, // 🆕 设置计算预算管理器
            strategy_stop_sender: strategy_stop_tx, // 🔧 修复：设置策略停止通知发送器
        });
        
        // 启动信号处理循环
        let manager_clone: Arc<OptimizedStrategyManager> = manager.clone();
        tokio::spawn(async move {
            info!("🔄 启动优化的交易信号处理循环");
            while let Some(signal) = signal_rx.recv().await {
                // 🆕 在处理信号前应用计算预算设置
                let enhanced_signal = manager_clone.apply_compute_budget_to_signal(signal);
                
                if let Err(e) = Self::process_trade_signal(
                    enhanced_signal,
                    executor_manager.clone(),
                    Some(manager_clone.clone())
                ).await {
                    error!("❌ 处理交易信号失败: {}", e);
                }
            }
            info!("🔚 交易信号处理循环已结束");
        });
        
        // 🔧 修复：启动策略停止通知处理循环
        let manager_clone_for_stop = manager.clone();
        tokio::spawn(async move {
            info!("🔄 启动策略停止通知处理循环");
            while let Some(mint) = strategy_stop_rx.recv().await {
                info!("📨 收到策略停止通知: mint={}", mint);
                if let Err(e) = manager_clone_for_stop.stop_strategy(&mint).await {
                    error!("❌ 处理策略停止通知失败: {}", e);
                }
            }
            info!("🔚 策略停止通知处理循环已结束");
        });
        
        manager
    }
    
    /// 🆕 为TradeSignal设置计算预算参数
    pub fn apply_compute_budget_to_signal(&self, mut signal: TradeSignal) -> TradeSignal {
        if let Some(ref cb_manager) = self.compute_budget_manager {
            let is_buy = matches!(signal.signal_type, TradeSignalType::Buy);
            
            // 设置计算单元数
            let compute_units = if is_buy {
                cb_manager.config.pumpfun_buy_cu
            } else {
                cb_manager.config.pumpfun_sell_cu
            };
            
            // 根据信号优先级和类型选择费用档位
            let priority_fee_tier = match (&signal.priority, is_buy) {
                (crate::strategy::SignalPriority::Critical, _) => {
                    // 紧急信号使用紧急卖出档位配置
                    cb_manager.get_emergency_sell_tier()
                }
                (_, true) => {
                    // 买入信号使用默认买入档位
                    cb_manager.get_default_buy_tier()
                }
                (_, false) => {
                    // 卖出信号使用默认卖出档位
                    cb_manager.get_default_sell_tier()
                }
            };
            
            info!("⚡ 应用计算预算: 操作={}, CU={}, 档位={}, 信号优先级={:?}", 
                  if is_buy { "买入" } else { "卖出" }, 
                  compute_units, 
                  priority_fee_tier.as_str(),
                  signal.priority);
            
            // 更新signal的计算预算字段
            signal = signal.with_compute_budget(compute_units, priority_fee_tier);
        } else {
            warn!("⚠️ 未配置计算预算管理器，使用默认设置");
            // 使用默认设置
            let is_buy = matches!(signal.signal_type, TradeSignalType::Buy);
            let compute_units = if is_buy { 68888 } else { 58888 };
            let tier = if is_buy { ComputeBudgetTier::Priority } else { ComputeBudgetTier::Express };
            signal = signal.with_compute_budget(compute_units, tier);
        }
        
        signal
    }

    /// 停止特定代币的策略 - 优化版本
    pub async fn stop_strategy(&self, mint: &Pubkey) -> Result<()> {
        if let Some((_, strategy_arc)) = self.strategies.remove(mint) {
            // 原子减少计数器
            self.strategy_count.fetch_sub(1, Ordering::Release);
            
            // 停止策略
            info!("⏹️ 停止优化策略: mint={}", mint);
            if let Err(e) = strategy_arc.stop().await {
                error!("❌ 停止策略失败: {}", e);
            }
            
            info!("✅ 策略已停止并移除");
        } else {
            warn!("⚠️ 未找到代币 {} 的活跃策略", mint);
        }

        Ok(())
    }

    /// 处理代币事件 - 高性能版本
    /// 
    /// 优化点：
    /// 1. 无锁读取现有策略
    /// 2. 无状态代币评估
    /// 3. 快速路径优化
    /// 4. 🔧 新增：提取真实价格信息
    pub async fn handle_token_event(&self, event: &TokenEvent) -> Result<()> {
        let mint = if let Some(mint_str) = &event.mint {
            mint_str.parse::<Pubkey>()?
        } else {
            return Ok(()); // 没有mint信息，跳过
        };

        // 🔧 新增：从事件中提取价格信息
        let price_info = self.extract_price_from_event(event);
        info!("接收到代币事件: {:?}", event.mint);

        // 快速检查：是否已有该代币的策略
        if let Some(strategy_arc) = self.strategies.get(&mint) {
            // 将事件传递给对应的策略（无锁访问）
            if let Err(e) = strategy_arc.handle_token_event(event).await {
                error!("❌ 策略处理代币事件失败: {}", e);
            }
            info!("📨 事件已转发给现有优化策略: mint={}", mint);
            return Ok(());
        }

        // 仅处理代币创建事件
        if !matches!(event.transaction_type, crate::processors::TransactionType::TokenCreation) {
            return Ok(());
        }

        // 使用无状态过滤器进行快速评估
        let filter_result = self.token_filter.evaluate_token_fast(event);
        
        if filter_result.passed {
            info!("🎯 ✅ 代币通过优化筛选!");
            info!("   ✅ 匹配条件: {:?}", filter_result.matched_criteria);
            
            info!("🚀 符合狙击条件 - 创建优化交易策略!");
            
            // 🔧 改进：创建包含价格信息的策略配置
            let strategy_config = self.default_config.clone();
            
            // 如果有价格信息，可以基于价格动态调整买入策略
            if let Some((price, _)) = &price_info {
                // 基于价格调整买入金额（可选的风险管理）
                let sol_amount_f64 = strategy_config.buy_amount_lamports as f64 / 1_000_000_000.0;
                info!("💡 基于价格 {:.9} SOL/token 调整策略，买入金额: {:.4} SOL", price, sol_amount_f64);
            }
            
            // 🔧 新增：提取创建者地址
            let creator_addr = if let Some(creator_str) = &event.creator_wallet {
                match creator_str.parse::<Pubkey>() {
                    Ok(addr) => {
                        info!("👤 找到代币创建者: {}", creator_str);
                        Some(addr)
                    }
                    Err(e) => {
                        warn!("⚠️ 解析创建者地址失败: {} - {}", creator_str, e);
                        None
                    }
                }
            } else {
                warn!("⚠️ 事件中缺少创建者地址信息");
                None
            };
            
            // 克隆 price_info 用于后续使用
            let price_info_clone = price_info.clone();
            
            match self.create_strategy_for_token(mint, Some(strategy_config), price_info_clone.clone(), creator_addr).await {
                Ok(_) => {
                    info!("🎉 ✅ 优化交易策略创建成功!");
                    info!("   🪙 代币地址: {}", mint);
                    info!("   🤖 策略将自动处理买入和卖出交易");
                    if let Some((price, source)) = &price_info_clone {
                        info!("   💰 创建时价格: {:.9} SOL/token (来源: {})", price, source);
                    }
                }
                Err(e) => {
                    error!("❌ 为代币 {} 创建优化策略失败: {}", mint, e);
                    if e.to_string().contains("已有活跃策略") {
                        warn!("   💡 该代币已有活跃策略，跳过创建");
                    } else if e.to_string().contains("并发策略数量限制") {
                        warn!("   💡 已达到最大并发策略数量，请等待现有策略完成");
                    }
                }
            }
        } else {
            info!("❌ 代币未通过优化筛选: mint={}, 原因={}", mint, filter_result.reason);
        }
        Ok(())
    }

    /// 🔧 新增：从 TokenEvent 中提取价格信息
    fn extract_price_from_event(&self, event: &TokenEvent) -> Option<(f64, String)> {
        if let (Some(sol_amount), Some(token_amount)) = (event.sol_amount, event.token_amount) {
            if token_amount > 0 {
                let raw_price = sol_amount as f64 / token_amount as f64;
                
                // 根据检测方法和交易类型调整价格
                let (adjusted_price, source) = match event.detection_method.as_str() {
                    // PumpFun 协议
                    s if s.contains("pumpfun") => {
                        let price = match event.transaction_type {
                            crate::processors::TransactionType::Buy => raw_price * 0.95,    // max_cost 的95%
                            crate::processors::TransactionType::Sell => raw_price * 1.05,   // min_output 的105%
                            _ => raw_price
                        };
                        (price, format!("PumpFun-{:?}", event.transaction_type))
                    }
                    // LetsBonk (Raydium Launchpad) 协议
                    s if s.contains("Raydium Launchpad") => {
                        let price = match event.transaction_type {
                            crate::processors::TransactionType::Buy => {
                                if s.contains("Exact In") {
                                    raw_price * 1.02  // exact_in 稍微上调
                                } else {
                                    raw_price * 0.98  // exact_out 稍微下调
                                }
                            }
                            crate::processors::TransactionType::Sell => {
                                if s.contains("Exact In") {
                                    raw_price * 0.98  // exact_in 稍微下调
                                } else {
                                    raw_price * 1.02  // exact_out 稍微上调
                                }
                            }
                            _ => raw_price
                        };
                        (price, format!("Raydium-{:?}", event.transaction_type))
                    }
                    _ => (raw_price, "Unknown".to_string())
                };
                
                Some((adjusted_price, source))
            } else {
                None
            }
        } else {
            None
        }
    }

    /// 🔧 新增：创建带价格和创建者信息的策略
    pub async fn create_strategy_for_token(
        &self,
        mint: Pubkey,
        config: Option<StrategyConfig>,
        price_info: Option<(f64, String)>,
        creator: Option<Pubkey>,
    ) -> Result<String> {
        // 原子检查策略数量限制，无锁操作
        let current_count = self.strategy_count.load(Ordering::Acquire);
        if current_count >= self.max_concurrent_strategies {
            warn!("⚠️ 已达到最大并发策略数量限制 ({})", self.max_concurrent_strategies);
            return Err(anyhow::anyhow!("超过最大并发策略数量限制"));
        }

        // 检查是否已有该代币的策略 - DashMap 的无锁读取
        if self.strategies.contains_key(&mint) {
            warn!("⚠️ 代币 {} 已有活跃策略", mint);
            return Err(anyhow::anyhow!("该代币已有活跃策略"));
        }

        // 创建新策略
        let strategy_config = config.unwrap_or_else(|| self.default_config.clone());
        
        // 克隆 price_info 和 creator 用于显示日志
        let price_info_display = price_info.clone();
        let creator_display = creator;
        
        let strategy = OptimizedTradingStrategy::new_with_price_and_creator(
            mint,
            strategy_config,
            self.signal_sender.clone(),
            price_info,
            creator,
        );

        let strategy_id = strategy.id.clone();
        
        info!("🎯 创建新的优化交易策略: {}", strategy_id);
        info!("   🪙 代币地址: {}", mint);
        if let Some((price, source)) = price_info_display {
            info!("   💰 初始价格: {:.9} SOL/token (来源: {})", price, source);
        }
        if let Some(creator_addr) = creator_display {
            info!("   👤 创建者地址: {}", creator_addr);
        }

        // 🔧 修复：为策略设置停止通知发送器
        strategy.set_strategy_stop_notifier(self.strategy_stop_sender.clone()).await;

        // 启动策略
        strategy.run().await?;

        // 原子性地添加策略
        match self.strategies.insert(mint, Arc::new(strategy)) {
            Some(_existing) => {
                warn!("⚠️ 覆盖已存在的策略: {:?}", mint);
                info!("✅ 优化策略 {} 已更新并启动", strategy_id);
            }
            None => {
                // 成功插入，增加计数器
                self.strategy_count.fetch_add(1, Ordering::Release);
                info!("✅ 优化策略 {} 已创建并启动", strategy_id);
            }
        }
        
        Ok(strategy_id)
    }
    pub async fn create_strategy_for_token_with_price(
        &self,
        mint: Pubkey,
        config: Option<StrategyConfig>,
        price_info: Option<(f64, String)>,
    ) -> Result<String> {
        // 原子检查策略数量限制，无锁操作
        let current_count = self.strategy_count.load(Ordering::Acquire);
        if current_count >= self.max_concurrent_strategies {
            warn!("⚠️ 已达到最大并发策略数量限制 ({})", self.max_concurrent_strategies);
            return Err(anyhow::anyhow!("超过最大并发策略数量限制"));
        }

        // 检查是否已有该代币的策略 - DashMap 的无锁读取
        if self.strategies.contains_key(&mint) {
            warn!("⚠️ 代币 {} 已有活跃策略", mint);
            return Err(anyhow::anyhow!("该代币已有活跃策略"));
        }

        // 创建新策略
        let strategy_config = config.unwrap_or_else(|| self.default_config.clone());
        
        // 克隆 price_info 用于显示日志
        let price_info_display = price_info.clone();
        
        let strategy = OptimizedTradingStrategy::new_with_price(
            mint,
            strategy_config,
            self.signal_sender.clone(),
            price_info,
        );

        let strategy_id = strategy.id.clone();
        
        info!("🎯 创建新的优化交易策略: {}", strategy_id);
        info!("   🪙 代币地址: {}", mint);
        if let Some((price, source)) = price_info_display {
            info!("   💰 初始价格: {:.9} SOL/token (来源: {})", price, source);
        }

        // 🔧 修复：为策略设置停止通知发送器
        strategy.set_strategy_stop_notifier(self.strategy_stop_sender.clone()).await;

        // 启动策略
        strategy.run().await?;

        // 原子性地添加策略
        match self.strategies.insert(mint, Arc::new(strategy)) {
            Some(_existing) => {
                warn!("⚠️ 覆盖已存在的策略: {:?}", mint);
                info!("✅ 优化策略 {} 已更新并启动", strategy_id);
            }
            None => {
                // 成功插入，增加计数器
                self.strategy_count.fetch_add(1, Ordering::Release);
                info!("✅ 优化策略 {} 已创建并启动", strategy_id);
            }
        }
        
        Ok(strategy_id)
    }

    /// 获取活跃策略数量 - 原子操作，无锁
    pub fn get_active_strategy_count(&self) -> usize {
        self.strategy_count.load(Ordering::Acquire)
    }

    /// 获取所有活跃策略的摘要 - 优化版本
    pub async fn get_active_strategies_summary(&self) -> Vec<String> {
        let mut summaries = Vec::new();

        // DashMap 的并发迭代，无需锁
        for entry in self.strategies.iter() {
            let (_mint, strategy_arc) = entry.pair();
            let summary = strategy_arc.get_summary().await;
            summaries.push(summary);
        }

        summaries
    }

    /// 获取特定代币的仓位信息 - 无锁读取
    pub fn get_position(&self, mint: &Pubkey) -> Option<Arc<OptimizedPosition>> {
        if let Some(strategy_arc) = self.strategies.get(mint) {
            Some(strategy_arc.get_position().clone())
        } else {
            None
        }
    }

    /// 打印系统状态 - 优化版本
    pub async fn print_status(&self) {
        let strategy_count = self.get_active_strategy_count();

        info!("📊 优化策略管理器状态报告");
        info!("   🎯 活跃策略数量: {}/{}", strategy_count, self.max_concurrent_strategies);
        info!("   💰 默认买入金额: {:.4} SOL", self.default_config.buy_amount_lamports as f64 / 1_000_000_000.0);
        info!("   ⏱️ 默认持仓时长: {}秒", self.default_config.holding_duration_seconds);
        info!("   🚀 使用优化架构: DashMap + 无状态过滤器");

        if strategy_count > 0 {
            info!("   📋 活跃策略列表:");
            let mut index = 1;
            for entry in self.strategies.iter() {
                let mint = entry.key();
                info!("   {}. 策略 mint: {}", index, mint);
                index += 1;
            }
        } else {
            info!("   📭 当前没有活跃策略");
        }
    }

    /// 处理交易信号 - 复用原有逻辑，但使用优化的架构
    pub async fn process_trade_signal(
        signal: TradeSignal,
        executor_manager: Option<Arc<OptimizedExecutorManager>>,
        strategy_manager: Option<Arc<OptimizedStrategyManager>>,
    ) -> Result<()> {
        info!("📨 处理优化交易信号: {:?} - {}", signal.signal_type, signal.reason);
        info!("   🪙 代币: {}", signal.mint);
        info!("   💰 金额: {:.4} SOL", signal.sol_amount as f64 / 1_000_000_000.0);
        info!("   ⏰ 优先级: {:?}", signal.priority);

        // 🔧 简化：统一使用基础验证，策略层面已做持仓检查
        if let Err(validation_error) = signal.validate() {
            error!("❌ 交易信号验证失败: {}", validation_error);
            return Err(anyhow::anyhow!("信号验证失败: {}", validation_error));
        }

        // 检查信号是否过期
        if signal.is_expired() {
            warn!("⚠️ 交易信号已过期，跳过执行");
            return Ok(());
        }

        // 如果没有执行器，只记录信号但不执行
        let Some(executor) = executor_manager else {
            info!("🔍 只读模式 - 记录交易信号但不执行实际交易");
            return Ok(());
        };

        // 执行交易
        let trade_params = signal.to_trade_params();
        let execution_strategy = executor.create_executor();
        let is_buy = matches!(signal.signal_type, TradeSignalType::Buy);

        match executor.execute_trade(trade_params, execution_strategy).await {
            Ok(result) => {
                info!("✅ 优化交易信号执行成功");
                info!("   📝 签名: {}", result.signature);
                info!("   💸 费用: {} lamports", result.actual_fee_paid);
                info!("   ⏱️ 延迟: {}ms", result.execution_latency_ms);

                // 将交易结果反馈给对应的策略
                if let Some(strategy_manager) = strategy_manager {
                    let token_amount = if is_buy {
                        // 🔧 重构：移除固定汇率回退，强制使用真实数据
                        match strategy_manager.get_token_amount_from_buy_result(&result, &signal.mint, &executor).await {
                            Ok(actual_tokens) => {
                                info!("✅ 获取实际代币数量成功: {} tokens", actual_tokens);
                                actual_tokens
                            }
                            Err(e) => {
                                error!("❌ 获取实际代币数量失败: {}", e);
                                error!("   💡 建议检查余额客户端配置或钱包私钥设置");
                                // 🔧 简化：交易成功但无法确认代币数量，记录警告并使用0
                                // 策略层面会根据实际情况处理这种状态
                                warn!("   ⚠️ 使用0作为代币数量，请注意检查实际交易结果");
                                0
                            }
                        }
                    } else {
                        // 卖出交易：直接使用信号中的代币数量
                        signal.token_amount.unwrap_or(0)
                    };

                    if let Err(e) = strategy_manager.handle_execution_result(
                        &result, 
                        &signal.mint, 
                        is_buy, 
                        signal.sol_amount, 
                        token_amount
                    ).await {
                        error!("❌ 策略处理执行结果失败: {}", e);
                    }
                }
            }
            Err(e) => {
                error!("❌ 优化交易信号执行失败: {}", e);
            }
        }

        Ok(())
    }

    /// 处理交易执行结果 - 优化版本
    pub async fn handle_execution_result(
        &self, 
        result: &ExecutionResult, 
        mint: &Pubkey, 
        is_buy: bool, 
        sol_amount: u64, 
        token_amount: u64
    ) -> Result<()> {
        if let Some(strategy_arc) = self.strategies.get(mint) {
            if let Err(e) = strategy_arc.handle_execution_result(result, is_buy, sol_amount, token_amount).await {
                error!("❌ 策略处理执行结果失败: {}", e);
            }
            info!("📊 执行结果已转发给优化策略: mint={}", mint);
        } else {
            warn!("⚠️ 收到交易结果，但未找到对应的优化策略: {}", mint);
        }

        Ok(())
    }

    /// 停止所有策略 - 优化版本
    pub async fn stop_all_strategies(&self) -> Result<()> {
        info!("⏹️ 停止所有优化策略");
        
        let strategy_count = self.get_active_strategy_count();
        if strategy_count == 0 {
            info!("📭 没有活跃策略需要停止");
            return Ok(());
        }

        info!("🛑 正在停止 {} 个优化策略", strategy_count);

        // 收集所有策略的引用
        let strategies_to_stop: Vec<_> = self.strategies.iter()
            .map(|entry| (*entry.key(), entry.value().clone()))
            .collect();

        // 清空策略映射
        self.strategies.clear();
        self.strategy_count.store(0, Ordering::Release);

        // 并发停止所有策略
        let mut stop_tasks = Vec::new();
        
        for (mint, strategy_arc) in strategies_to_stop {
            let stop_task = tokio::spawn(async move {
                info!("⏹️ 停止优化策略: mint={}", mint);
                if let Err(e) = strategy_arc.stop().await {
                    error!("❌ 停止策略失败: {}", e);
                }
                if let Err(e) = strategy_arc.destroy().await {
                    error!("❌ 销毁策略失败: {}", e);
                }
                mint
            });
            stop_tasks.push(stop_task);
        }

        // 等待所有策略停止
        for task in stop_tasks {
            match task.await {
                Ok(mint) => info!("✅ 优化策略 {} 已停止", mint),
                Err(e) => error!("❌ 策略停止任务失败: {}", e),
            }
        }

        info!("✅ 所有优化策略已停止");
        Ok(())
    }

    /// 获取钱包公钥的辅助方法
    async fn get_wallet_pubkey(&self, _executor: &Arc<OptimizedExecutorManager>) -> Option<Pubkey> {
        // 🔧 修复：统一从配置管理器获取钱包公钥，确保一致性
        if let Ok(private_key_str) = std::env::var("WALLET_PRIVATE_KEY") {
            if let Ok(private_key_bytes) = bs58::decode(&private_key_str).into_vec() {
                if let Ok(keypair) = solana_sdk::signature::Keypair::from_bytes(&private_key_bytes) {
                    return Some(keypair.pubkey());
                }
            }
            
            // 🔧 新增：支持JSON数组格式的私钥
            if private_key_str.starts_with('[') && private_key_str.ends_with(']') {
                if let Ok(bytes) = serde_json::from_str::<Vec<u8>>(&private_key_str) {
                    if bytes.len() == 64 {
                        if let Ok(keypair) = solana_sdk::signature::Keypair::from_bytes(&bytes) {
                            return Some(keypair.pubkey());
                        }
                    }
                }
            }
        }
        
        warn!("⚠️ 无法获取钱包公钥，请检查 WALLET_PRIVATE_KEY 环境变量");
        None
    }

    /// 🔧 重构：简化代币数量获取，移除固定汇率回退
    /// 现在所有价格信息都来自真实的链上数据，不再需要估算回退
    async fn get_token_amount_from_buy_result(
        &self, 
        result: &ExecutionResult, 
        mint: &Pubkey, 
        executor: &Arc<OptimizedExecutorManager>
    ) -> Result<u64> {
        // 优先从余额客户端获取真实数量
        if let Some(balance_client) = &self.token_balance_client {
            if let Some(wallet_pubkey) = self.get_wallet_pubkey(executor).await {
                info!("🔍 查询买入交易的实际代币数量...");
                return balance_client.get_tokens_acquired_from_buy_transaction(
                    &Signature::from_str(&result.signature.to_string()).unwrap_or_default(),
                    mint,
                    &wallet_pubkey,
                ).await.map_err(|e| anyhow::anyhow!("余额查询失败: {}", e));
            }
        }
        
        // ❌ 移除了固定汇率估算回退逻辑
        // 现在如果无法获取真实数量，直接返回错误，强制使用真实价格
        Err(anyhow::anyhow!("余额客户端不可用或钱包公钥获取失败，无法确定真实代币数量"))
    }
}

/// 优化的策略管理器统计信息
#[derive(Debug, Default)]
pub struct OptimizedStrategyManagerStats {
    pub total_strategies: usize,
    pub memory_efficiency_improvement: f64,  // 内存效率提升比例
    pub lock_contention_reduction: f64,      // 锁竞争减少比例
    pub throughput_improvement: f64,         // 吞吐量提升比例
}

impl OptimizedStrategyManagerStats {
    pub fn print(&self) {
        info!("📊 优化策略管理器统计:");
        info!("   🎯 总策略数: {}", self.total_strategies);
        info!("   🚀 内存效率提升: {:.1}%", self.memory_efficiency_improvement * 100.0);
        info!("   🔓 锁竞争减少: {:.1}%", self.lock_contention_reduction * 100.0);
        info!("   📈 吞吐量提升: {:.1}%", self.throughput_improvement * 100.0);
    }
}