use solana_sdk::{
    instruction::Instruction,
    compute_budget::ComputeBudgetInstruction,
    pubkey::Pubkey,
};
use solana_client::rpc_client::RpcClient;
use std::time::{Duration, Instant};
use std::sync::{Arc, Mutex};
use log::{debug, info, warn};
use tokio::time::interval;
use std::str::FromStr;
use serde::{Deserialize, Serialize};

/// PumpFun 固定计算单元配置 - 将被配置文件覆盖
pub const PUMPFUN_BUY_CU: u32 = 68888;
pub const PUMPFUN_SELL_CU: u32 = 58888;

/// 计算预算档位枚举
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ComputeBudgetTier {
    /// 经济档 - P25分位数
    Economy,
    /// 标准档 - P50分位数（中位数）
    Standard,
    /// 优先档 - P75分位数
    Priority,
    /// 快速档 - P90分位数
    Express,
    /// 闪电档 - P95分位数（抢跑）
    Lightning,
}

impl ComputeBudgetTier {
    /// 从字符串解析档位
    pub fn from_str(s: &str) -> Result<Self, String> {
        match s.to_lowercase().as_str() {
            "economy" => Ok(ComputeBudgetTier::Economy),
            "standard" => Ok(ComputeBudgetTier::Standard),
            "priority" => Ok(ComputeBudgetTier::Priority),
            "express" => Ok(ComputeBudgetTier::Express),
            "lightning" => Ok(ComputeBudgetTier::Lightning),
            _ => Err(format!("未知的计算预算档位: {}", s)),
        }
    }
    
    /// 转为字符串
    pub fn as_str(&self) -> &'static str {
        match self {
            ComputeBudgetTier::Economy => "economy",
            ComputeBudgetTier::Standard => "standard",
            ComputeBudgetTier::Priority => "priority",
            ComputeBudgetTier::Express => "express",
            ComputeBudgetTier::Lightning => "lightning",
        }
    }
}

impl Default for ComputeBudgetTier {
    fn default() -> Self {
        ComputeBudgetTier::Standard
    }
}

/// 计算预算配置 - 从config.toml读取
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComputeBudgetConfigFromFile {
    pub enabled: bool,
    pub fee_refresh_interval_seconds: u64,
    pub fee_history_duration_seconds: u64,
    pub base_priority_fee: u64,
    pub max_priority_fee: u64,
    pub pumpfun_buy: PumpFunTxConfig,
    pub pumpfun_sell: PumpFunTxConfig,
    pub defaults: DefaultTiers,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PumpFunTxConfig {
    pub compute_units: u32,
    pub fee_tiers: FeeTiers,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeeTiers {
    pub economy: u8,    // P25
    pub standard: u8,   // P50
    pub priority: u8,   // P75
    pub express: u8,    // P90
    pub lightning: u8,  // P95
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DefaultTiers {
    pub buy_default_tier: String,
    pub sell_default_tier: String,
    pub emergency_sell_tier: String,
}

/// 优先费分位数缓存结构
#[derive(Debug, Clone)]
pub struct PriorityFeeTierCache {
    /// 各档位的费用值（micro-lamports per CU）
    pub economy: u64,      // P25
    pub standard: u64,     // P50
    pub priority: u64,     // P75
    pub express: u64,      // P90
    pub lightning: u64,    // P95
    /// 缓存更新时间
    pub updated_at: Instant,
    /// 数据有效性
    pub is_valid: bool,
}

/// PumpFun 相关的固定账户列表
#[derive(Debug, Clone)]
pub struct PumpFunAccounts {
    /// PumpFun 程序 ID
    pub program_id: Pubkey,
    /// 全局账户
    pub global: Pubkey,
    /// 费用接收账户 (买入)
    pub fee_recipient_buy: Pubkey,
    /// 费用接收账户 (卖出)
    pub fee_recipient_sell: Pubkey,
    /// 系统程序
    pub system_program: Pubkey,
    /// Token 程序
    pub token_program: Pubkey,
    /// 事件权限账户
    pub event_authority: Pubkey,
}

impl Default for PumpFunAccounts {
    fn default() -> Self {
        Self {
            program_id: Pubkey::from_str("6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P")
                .expect("Invalid PumpFun program ID"),
            global: Pubkey::from_str("4wTV1YmiEkRvAtNtsSGPtUrqRYQMe5SKy2uB4Jjaxnjf")
                .expect("Invalid global account"),
            fee_recipient_buy: Pubkey::from_str("AVmoTthdrX6tKt4nDjco2D775W2YK3sDhxPcMmzUAmTY")
                .expect("Invalid buy fee recipient"),
            fee_recipient_sell: Pubkey::from_str("CebN5WGQ4jvEPvsVU4EoHEpgzq1VV7AbicfhtW4xC9iM")
                .expect("Invalid sell fee recipient"),
            system_program: Pubkey::from_str("11111111111111111111111111111111")
                .expect("Invalid system program"),
            token_program: Pubkey::from_str("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA")
                .expect("Invalid token program"),
            event_authority: Pubkey::from_str("Ce6TQqeHC9p8KetsN6JsjHK7UTZk7nasjjnr7XxXp9F1")
                .expect("Invalid event authority"),
        }
    }
}

impl PumpFunAccounts {
    /// 获取买入交易相关的固定账户列表
    pub fn get_buy_accounts(&self) -> Vec<Pubkey> {
        vec![
            self.program_id,
            self.global,
            self.fee_recipient_buy,
            self.system_program,
            self.token_program,
            self.event_authority,
        ]
    }

    /// 获取卖出交易相关的固定账户列表
    pub fn get_sell_accounts(&self) -> Vec<Pubkey> {
        vec![
            self.program_id,
            self.global,
            self.fee_recipient_sell,
            self.system_program,
            self.token_program,
            self.event_authority,
        ]
    }
}

/// 计算预算配置 - 运行时配置
#[derive(Debug, Clone)]
pub struct ComputeBudgetConfig {
    /// 基础优先费用 (micro-lamports per CU)
    pub base_priority_fee: u64,
    /// 最大优先费用 (micro-lamports per CU)
    pub max_priority_fee: u64,
    /// 费用数据刷新间隔 (秒)
    pub fee_refresh_interval: u64,
    /// 费用历史保留时间 (秒)
    pub fee_history_duration: u64,
    /// PumpFun买入交易CU
    pub pumpfun_buy_cu: u32,
    /// PumpFun卖出交易CU
    pub pumpfun_sell_cu: u32,
    /// 买入费用档位配置
    pub buy_fee_tiers: FeeTiers,
    /// 卖出费用档位配置
    pub sell_fee_tiers: FeeTiers,
    /// 默认档位选择
    pub buy_default_tier: ComputeBudgetTier,
    pub sell_default_tier: ComputeBudgetTier,
    pub emergency_sell_tier: ComputeBudgetTier,
}

impl Default for ComputeBudgetConfig {
    fn default() -> Self {
        Self {
            base_priority_fee: 10000,       // 基础10k micro-lamports/CU
            max_priority_fee: 100000000,    // 最大100M micro-lamports/CU (0.1 SOL/CU)
            fee_refresh_interval: 30,       // 每30秒获取一次
            fee_history_duration: 300,      // 保留5分钟历史
            pumpfun_buy_cu: PUMPFUN_BUY_CU,
            pumpfun_sell_cu: PUMPFUN_SELL_CU,
            buy_fee_tiers: FeeTiers {
                economy: 25,
                standard: 50,
                priority: 75,
                express: 90,
                lightning: 95,
            },
            sell_fee_tiers: FeeTiers {
                economy: 25,
                standard: 50,
                priority: 75,
                express: 90,
                lightning: 95,
            },
            buy_default_tier: ComputeBudgetTier::Priority,
            sell_default_tier: ComputeBudgetTier::Express,
            emergency_sell_tier: ComputeBudgetTier::Lightning,
        }
    }
}

impl ComputeBudgetConfig {
    /// 从配置文件创建
    pub fn from_config_file(config: ComputeBudgetConfigFromFile) -> Result<Self, String> {
        let buy_default_tier = ComputeBudgetTier::from_str(&config.defaults.buy_default_tier)?;
        let sell_default_tier = ComputeBudgetTier::from_str(&config.defaults.sell_default_tier)?;
        let emergency_sell_tier = ComputeBudgetTier::from_str(&config.defaults.emergency_sell_tier)?;
        
        Ok(Self {
            base_priority_fee: config.base_priority_fee,
            max_priority_fee: config.max_priority_fee,
            fee_refresh_interval: config.fee_refresh_interval_seconds,
            fee_history_duration: config.fee_history_duration_seconds,
            pumpfun_buy_cu: config.pumpfun_buy.compute_units,
            pumpfun_sell_cu: config.pumpfun_sell.compute_units,
            buy_fee_tiers: config.pumpfun_buy.fee_tiers,
            sell_fee_tiers: config.pumpfun_sell.fee_tiers,
            buy_default_tier,
            sell_default_tier,
            emergency_sell_tier,
        })
    }
}

/// 网络费用级别
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FeeLevel {
    /// 低费用 - 非紧急交易
    Low,
    /// 标准费用 - 普通交易
    Standard,
    /// 高费用 - 紧急交易
    High,
    /// 极高费用 - 抢跑交易
    Urgent,
}

impl FeeLevel {
    pub fn multiplier(&self) -> f64 {
        match self {
            FeeLevel::Low => 0.8,
            FeeLevel::Standard => 1.0,
            FeeLevel::High => 1.5,
            FeeLevel::Urgent => 3.0,
        }
    }
}

/// 优先费用历史记录
#[derive(Debug, Clone)]
struct FeeRecord {
    timestamp: Instant,
    fee: u64,
}

/// 动态计算预算管理器
pub struct DynamicComputeBudgetManager {
    pub config: ComputeBudgetConfig,
    pumpfun_accounts: PumpFunAccounts,
    rpc_client: Option<Arc<RpcClient>>,
    /// 买入交易费用历史
    buy_fee_history: Arc<Mutex<Vec<FeeRecord>>>,
    /// 卖出交易费用历史
    sell_fee_history: Arc<Mutex<Vec<FeeRecord>>>,
    /// 费用获取任务是否运行中
    fee_task_running: Arc<Mutex<bool>>,
    /// 买入交易费用分位数缓存
    buy_tier_cache: Arc<Mutex<PriorityFeeTierCache>>,
    /// 卖出交易费用分位数缓存
    sell_tier_cache: Arc<Mutex<PriorityFeeTierCache>>,
}

impl DynamicComputeBudgetManager {
    pub fn new(config: ComputeBudgetConfig, rpc_client: Option<RpcClient>) -> Self {
        let base_fee = config.base_priority_fee;
        Self {
            config,
            pumpfun_accounts: PumpFunAccounts::default(),
            rpc_client: rpc_client.map(Arc::new),
            buy_fee_history: Arc::new(Mutex::new(Vec::new())),
            sell_fee_history: Arc::new(Mutex::new(Vec::new())),
            fee_task_running: Arc::new(Mutex::new(false)),
            buy_tier_cache: Arc::new(Mutex::new(PriorityFeeTierCache {
                economy: base_fee,
                standard: base_fee,
                priority: base_fee,
                express: base_fee,
                lightning: base_fee,
                updated_at: std::time::Instant::now(),
                is_valid: false,
            })),
            sell_tier_cache: Arc::new(Mutex::new(PriorityFeeTierCache {
                economy: base_fee,
                standard: base_fee,
                priority: base_fee,
                express: base_fee,
                lightning: base_fee,
                updated_at: std::time::Instant::now(),
                is_valid: false,
            })),
        }
    }

    /// 启动自动费用获取任务
    pub async fn start_fee_monitoring(&self) -> Result<(), crate::executor::errors::ExecutionError> {
        let mut task_running = self.fee_task_running.lock().unwrap();
        if *task_running {
            info!("📊 费用监控任务已在运行");
            return Ok(());
        }
        *task_running = true;
        drop(task_running);

        if let Some(rpc_client) = &self.rpc_client {
            let client = Arc::clone(rpc_client);
            let buy_history = Arc::clone(&self.buy_fee_history);
            let sell_history = Arc::clone(&self.sell_fee_history);
            let accounts = self.pumpfun_accounts.clone();
            let config = self.config.clone();
            let task_running = Arc::clone(&self.fee_task_running);

            tokio::spawn(async move {
                let mut interval = interval(Duration::from_secs(config.fee_refresh_interval));
                
                info!("🚀 启动 PumpFun 费用监控任务，间隔: {}秒", config.fee_refresh_interval);
                
                loop {
                    interval.tick().await;
                    
                    // 检查任务是否应该继续运行
                    {
                        let should_continue = *task_running.lock().unwrap();
                        if !should_continue {
                            info!("⏹️ 费用监控任务停止");
                            break;
                        }
                    }

                    // 获取买入交易费用
                    if let Ok(buy_fee) = Self::fetch_priority_fees(&client, &accounts.get_buy_accounts()).await {
                        let record = FeeRecord {
                            timestamp: Instant::now(),
                            fee: buy_fee,
                        };
                        
                        let mut buy_hist = buy_history.lock().unwrap();
                        buy_hist.push(record);
                        Self::cleanup_old_records(&mut buy_hist, config.fee_history_duration);
                        
                        debug!("💰 买入费用更新: {} micro-lamports/CU", buy_fee);
                    }

                    // 获取卖出交易费用
                    if let Ok(sell_fee) = Self::fetch_priority_fees(&client, &accounts.get_sell_accounts()).await {
                        let record = FeeRecord {
                            timestamp: Instant::now(),
                            fee: sell_fee,
                        };
                        
                        let mut sell_hist = sell_history.lock().unwrap();
                        sell_hist.push(record);
                        Self::cleanup_old_records(&mut sell_hist, config.fee_history_duration);
                        
                        debug!("💰 卖出费用更新: {} micro-lamports/CU", sell_fee);
                    }
                }
            });

            info!("✅ PumpFun 费用监控任务已启动");
        } else {
            warn!("⚠️ 无 RPC 客户端，无法启动费用监控");
        }

        Ok(())
    }

    /// 停止费用监控任务
    pub fn stop_fee_monitoring(&self) {
        let mut task_running = self.fee_task_running.lock().unwrap();
        *task_running = false;
        info!("🛑 费用监控任务已停止");
    }

    /// 获取指定账户的优先费用
    async fn fetch_priority_fees(
        client: &RpcClient,
        accounts: &[Pubkey],
    ) -> Result<u64, crate::executor::errors::ExecutionError> {
        // 使用 get_recent_prioritization_fees 方法
        match client.get_recent_prioritization_fees(accounts) {
            Ok(fees) => {
                if fees.is_empty() {
                    return Ok(10000); // 默认费用
                }

                // 计算最近费用的平均值，过滤掉0费用
                let valid_fees: Vec<u64> = fees.iter()
                    .map(|f| f.prioritization_fee)
                    .filter(|&fee| fee > 0)
                    .collect();
                
                if valid_fees.is_empty() {
                    return Ok(10000); // 如果所有费用都是0，使用默认值
                }
                
                let total_fee: u64 = valid_fees.iter().sum();
                let avg_fee = total_fee / valid_fees.len() as u64;
                
                debug!("📈 获取到 {} 条有效费用记录，平均: {} micro-lamports/CU", 
                       valid_fees.len(), avg_fee);
                
                Ok(avg_fee)
            },
            Err(e) => {
                warn!("⚠️ 获取优先费用失败: {}", e);
                Err(crate::executor::errors::ExecutionError::Internal(
                    format!("Failed to fetch priority fees: {}", e)
                ))
            }
        }
    }

    /// 清理过期的费用记录
    fn cleanup_old_records(records: &mut Vec<FeeRecord>, max_age_seconds: u64) {
        let cutoff = Instant::now() - Duration::from_secs(max_age_seconds);
        records.retain(|record| record.timestamp > cutoff);
    }

    /// 获取当前优先费用 (买入)
    pub fn get_current_buy_priority_fee(&self, fee_level: FeeLevel) -> u64 {
        let base_fee = {
            let history = self.buy_fee_history.lock().unwrap();
            if history.is_empty() {
                self.config.base_priority_fee
            } else {
                // 使用最近5条记录的平均值
                let recent_count = std::cmp::min(5, history.len());
                let recent_fees: Vec<u64> = history
                    .iter()
                    .rev()
                    .take(recent_count)
                    .map(|r| r.fee)
                    .collect();
                
                recent_fees.iter().sum::<u64>() / recent_fees.len() as u64
            }
        };

        let adjusted_fee = (base_fee as f64 * fee_level.multiplier()) as u64;
        std::cmp::min(adjusted_fee, self.config.max_priority_fee)
    }

    /// 获取当前优先费用 (卖出)
    pub fn get_current_sell_priority_fee(&self, fee_level: FeeLevel) -> u64 {
        let base_fee = {
            let history = self.sell_fee_history.lock().unwrap();
            if history.is_empty() {
                self.config.base_priority_fee
            } else {
                // 使用最近5条记录的平均值
                let recent_count = std::cmp::min(5, history.len());
                let recent_fees: Vec<u64> = history
                    .iter()
                    .rev()
                    .take(recent_count)
                    .map(|r| r.fee)
                    .collect();
                
                recent_fees.iter().sum::<u64>() / recent_fees.len() as u64
            }
        };

        let adjusted_fee = (base_fee as f64 * fee_level.multiplier()) as u64;
        std::cmp::min(adjusted_fee, self.config.max_priority_fee)
    }

    /// 计算交易所需的计算单元 (现在固定)
    pub fn calculate_compute_units(
        &self,
        _instruction_count: usize,
        _account_count: usize,
        transaction_type: &str,
    ) -> u32 {
        match transaction_type {
            "pumpfun_buy" => PUMPFUN_BUY_CU,
            "pumpfun_sell" => PUMPFUN_SELL_CU,
            _ => PUMPFUN_BUY_CU, // 默认使用买入的CU
        }
    }

    /// 构建计算预算指令
    pub fn build_compute_budget_instructions(
        &self,
        instruction_count: usize,
        account_count: usize,
        transaction_type: &str,
        fee_level: FeeLevel,
    ) -> Vec<Instruction> {
        let compute_units = self.calculate_compute_units(instruction_count, account_count, transaction_type);
        
        let priority_fee = match transaction_type {
            "pumpfun_buy" => self.get_current_buy_priority_fee(fee_level),
            "pumpfun_sell" => self.get_current_sell_priority_fee(fee_level),
            _ => self.get_current_buy_priority_fee(fee_level),
        };
        
        info!("📊 预算配置: CU={}, 优先费={} micro-lamports/CU, 类型={}, 级别={:?}", 
              compute_units, priority_fee, transaction_type, fee_level);
        
        vec![
            ComputeBudgetInstruction::set_compute_unit_limit(compute_units),
            ComputeBudgetInstruction::set_compute_unit_price(priority_fee),
        ]
    }

    /// PumpFun买入交易的预算配置
    pub fn for_pumpfun_buy(
        &self,
        _instruction_count: usize,
        _account_count: usize,
        fee_level: FeeLevel,
        _endpoint: Option<&str>,
    ) -> Result<Vec<Instruction>, crate::executor::errors::ExecutionError> {
        Ok(self.build_compute_budget_instructions(
            0, 0, "pumpfun_buy", fee_level,
        ))
    }

    /// PumpFun卖出交易的预算配置
    pub fn for_pumpfun_sell(
        &self,
        _instruction_count: usize,
        _account_count: usize,
        fee_level: FeeLevel,
        _endpoint: Option<&str>,
    ) -> Result<Vec<Instruction>, crate::executor::errors::ExecutionError> {
        Ok(self.build_compute_budget_instructions(
            0, 0, "pumpfun_sell", fee_level,
        ))
    }

    /// 获取费用统计信息
    pub fn get_fee_stats(&self) -> (usize, usize, Option<u64>, Option<u64>) {
        let buy_history = self.buy_fee_history.lock().unwrap();
        let sell_history = self.sell_fee_history.lock().unwrap();
        
        let buy_avg = if buy_history.is_empty() {
            None
        } else {
            Some(buy_history.iter().map(|r| r.fee).sum::<u64>() / buy_history.len() as u64)
        };
        
        let sell_avg = if sell_history.is_empty() {
            None
        } else {
            Some(sell_history.iter().map(|r| r.fee).sum::<u64>() / sell_history.len() as u64)
        };
        
        (buy_history.len(), sell_history.len(), buy_avg, sell_avg)
    }
    
    /// 根据档位获取买入交易优先费用
    pub fn get_buy_priority_fee_by_tier(&self, tier: ComputeBudgetTier) -> u64 {
        let cache = self.buy_tier_cache.lock().unwrap();
        if !cache.is_valid {
            // 缓存无效，使用基础费用
            return self.config.base_priority_fee;
        }
        
        let fee = match tier {
            ComputeBudgetTier::Economy => cache.economy,
            ComputeBudgetTier::Standard => cache.standard,
            ComputeBudgetTier::Priority => cache.priority,
            ComputeBudgetTier::Express => cache.express,
            ComputeBudgetTier::Lightning => cache.lightning,
        };
        
        // 确保费用在合理范围内
        fee.clamp(self.config.base_priority_fee, self.config.max_priority_fee)
    }
    
    /// 根据档位获取卖出交易优先费用
    pub fn get_sell_priority_fee_by_tier(&self, tier: ComputeBudgetTier) -> u64 {
        let cache = self.sell_tier_cache.lock().unwrap();
        if !cache.is_valid {
            // 缓存无效，使用基础费用
            return self.config.base_priority_fee;
        }
        
        let fee = match tier {
            ComputeBudgetTier::Economy => cache.economy,
            ComputeBudgetTier::Standard => cache.standard,
            ComputeBudgetTier::Priority => cache.priority,
            ComputeBudgetTier::Express => cache.express,
            ComputeBudgetTier::Lightning => cache.lightning,
        };
        
        // 确保费用在合理范围内
        fee.clamp(self.config.base_priority_fee, self.config.max_priority_fee)
    }
    
    /// 获取默认买入档位
    pub fn get_default_buy_tier(&self) -> ComputeBudgetTier {
        self.config.buy_default_tier
    }
    
    /// 获取默认卖出档位
    pub fn get_default_sell_tier(&self) -> ComputeBudgetTier {
        self.config.sell_default_tier
    }
    
    /// 获取紧急卖出档位
    pub fn get_emergency_sell_tier(&self) -> ComputeBudgetTier {
        self.config.emergency_sell_tier
    }
    
    /// 构建指定档位的计算预算指令
    pub fn build_compute_budget_instructions_with_tier(
        &self,
        is_buy: bool,
        tier: ComputeBudgetTier,
    ) -> Vec<Instruction> {
        let compute_units = if is_buy {
            self.config.pumpfun_buy_cu
        } else {
            self.config.pumpfun_sell_cu
        };
        
        let priority_fee = if is_buy {
            self.get_buy_priority_fee_by_tier(tier)
        } else {
            self.get_sell_priority_fee_by_tier(tier)
        };
        
        info!("📊 预算配置 [{}档]: CU={}, 优先费={} micro-lamports/CU, 操作={}", 
              tier.as_str(), compute_units, priority_fee, if is_buy { "买入" } else { "卖出" });
        
        vec![
            ComputeBudgetInstruction::set_compute_unit_limit(compute_units),
            ComputeBudgetInstruction::set_compute_unit_price(priority_fee),
        ]
    }
    
    /// 获取缓存状态
    pub fn get_cache_status(&self) -> (bool, bool) {
        let buy_cache = self.buy_tier_cache.lock().unwrap();
        let sell_cache = self.sell_tier_cache.lock().unwrap();
        (buy_cache.is_valid, sell_cache.is_valid)
    }
    
    /// 获取所有档位费用（用于调试）
    pub fn get_all_tier_fees(&self) -> (PriorityFeeTierCache, PriorityFeeTierCache) {
        let buy_cache = self.buy_tier_cache.lock().unwrap();
        let sell_cache = self.sell_tier_cache.lock().unwrap();
        (buy_cache.clone(), sell_cache.clone())
    }
}