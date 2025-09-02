use serde::{Deserialize, Serialize};
use solana_sdk::pubkey::Pubkey;
use log::{info, warn};
use crate::executor::compute_budget::ComputeBudgetTier;

/// 交易信号类型
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TradeSignalType {
    /// 买入信号
    Buy,
    /// 卖出信号  
    Sell,
    /// 取消交易信号
    Cancel,
}

/// 交易信号优先级
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SignalPriority {
    /// 低优先级 - 普通交易
    Low,
    /// 中优先级 - 重要交易
    Medium,
    /// 高优先级 - 紧急交易
    High,
    /// 极高优先级 - 立即执行
    Critical,
}

/// 交易信号
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradeSignal {
    /// 策略ID
    pub strategy_id: String,
    /// 代币mint地址
    pub mint: Pubkey,
    /// 信号类型
    pub signal_type: TradeSignalType,
    /// SOL数量 (lamports)
    pub sol_amount: u64,
    /// 代币数量 (如果是卖出信号)
    pub token_amount: Option<u64>,
    /// 最大滑点 (基点, 100 = 1%)
    pub max_slippage_bps: u16,
    /// 信号优先级
    pub priority: SignalPriority,
    /// 过期时间戳 (秒)
    pub expires_at: Option<i64>,
    /// 信号原因/描述
    pub reason: String,
    /// 创建时间戳
    pub created_at: i64,
    /// 额外元数据
    pub metadata: std::collections::HashMap<String, String>,
    /// 🔧 新增：当前代币价格 (SOL per token) - 用于精确滑点计算
    pub current_price: Option<f64>,
    /// 🔧 新增：价格来源协议 (PumpFun/Raydium)
    pub price_source: Option<String>,
    /// 🔧 新增：代币创建者地址 - 用于 PumpFun creator_vault
    pub creator: Option<Pubkey>,
    
    // 🆕 新增计算预算字段
    /// 计算单元数 - 区分买入/卖出操作
    pub compute_units: u32,
    /// 优先费档位
    pub priority_fee_tier: ComputeBudgetTier,
    /// 自定义优先费 (micro-lamports per CU) - 如果设置，将覆盖档位设置
    pub custom_priority_fee: Option<u64>,
}

impl TradeSignal {
    /// 🔧 新增：创建带价格信息的买入信号
    pub fn buy_with_price(
        strategy_id: String,
        mint: Pubkey,
        sol_amount: u64,
        max_slippage_bps: u16,
        reason: String,
        current_price: f64,
        price_source: String,
    ) -> Self {
        Self {
            strategy_id,
            mint,
            signal_type: TradeSignalType::Buy,
            sol_amount,
            token_amount: None,
            max_slippage_bps,
            priority: SignalPriority::High,
            expires_at: Some(chrono::Utc::now().timestamp() + 300),
            reason,
            created_at: chrono::Utc::now().timestamp(),
            metadata: std::collections::HashMap::new(),
            current_price: Some(current_price),
            price_source: Some(price_source),
            creator: None, // 需要在外部设置
            // 默认计算预算设置 - 将由StrategyManager设置
            compute_units: 0, // 占位值，在strategy层设置
            priority_fee_tier: ComputeBudgetTier::default(),
            custom_priority_fee: None,
        }
    }

    /// 🔧 新增：创建带价格和创建者信息的买入信号
    pub fn buy_with_price_and_creator(
        strategy_id: String,
        mint: Pubkey,
        sol_amount: u64,
        max_slippage_bps: u16,
        reason: String,
        current_price: f64,
        price_source: String,
        creator: Pubkey,
    ) -> Self {
        Self {
            strategy_id,
            mint,
            signal_type: TradeSignalType::Buy,
            sol_amount,
            token_amount: None,
            max_slippage_bps,
            priority: SignalPriority::High,
            expires_at: Some(chrono::Utc::now().timestamp() + 300),
            reason,
            created_at: chrono::Utc::now().timestamp(),
            metadata: std::collections::HashMap::new(),
            current_price: Some(current_price),
            price_source: Some(price_source),
            creator: Some(creator),
            // 默认计算预算设置 - 将由StrategyManager设置
            compute_units: 0, // 占位值，在strategy层设置
            priority_fee_tier: ComputeBudgetTier::default(),
            custom_priority_fee: None,
        }
    }

    /// 🔧 新增：创建带价格信息的卖出信号
    pub fn sell_with_price(
        strategy_id: String,
        mint: Pubkey,
        token_amount: u64,
        max_slippage_bps: u16,
        reason: String,
        current_price: f64,
        price_source: String,
    ) -> Self {
        // 🔧 优化：卖出信号不预先计算SOL金额
        // 所有滑点保护计算统一在 to_trade_params() 中处理
        // sol_amount 字段对卖出信号无意义，设为0（将在to_trade_params中重新计算为min_sol_out）

        Self {
            strategy_id,
            mint,
            signal_type: TradeSignalType::Sell,
            sol_amount: 0, // 🔧 卖出信号时设为0，将在to_trade_params中重新计算为min_sol_out
            token_amount: Some(token_amount),
            max_slippage_bps,
            priority: SignalPriority::High,
            expires_at: Some(chrono::Utc::now().timestamp() + 300),
            reason,
            created_at: chrono::Utc::now().timestamp(),
            metadata: std::collections::HashMap::new(),
            current_price: Some(current_price),
            price_source: Some(price_source),
            creator: None, // 需要在外部设置
            // 默认计算预算设置 - 将由StrategyManager设置
            compute_units: 0, // 占位值，在strategy层设置
            priority_fee_tier: ComputeBudgetTier::default(),
            custom_priority_fee: None,
        }
    }

    /// 🔧 新增：创建带价格信息的紧急卖出信号
    pub fn emergency_sell_with_price(
        strategy_id: String,
        mint: Pubkey,
        token_amount: u64,
        reason: String,
        current_price: f64,
        price_source: String,
    ) -> Self {
        // 🔧 优化：紧急卖出不控制滑点，优先执行速度
        // 设置极高滑点容忍度确保交易能快速执行

        Self {
            strategy_id,
            mint,
            signal_type: TradeSignalType::Sell,
            sol_amount: 0, // 🔧 紧急卖出时设为0，将在to_trade_params中重新计算为min_sol_out
            token_amount: Some(token_amount),
            max_slippage_bps: 9999, // 99.99% 滑点容忍度，基本不限制
            priority: SignalPriority::Critical,
            expires_at: Some(chrono::Utc::now().timestamp() + 60),
            reason: format!("EMERGENCY: {}", reason),
            created_at: chrono::Utc::now().timestamp(),
            metadata: std::collections::HashMap::new(),
            current_price: Some(current_price),
            price_source: Some(price_source),
            creator: None, // 紧急卖出时可能没有创建者信息
            // 紧急卖出默认使用最高档位 - 将由StrategyManager设置
            compute_units: 0, // 占位值，在strategy层设置
            priority_fee_tier: ComputeBudgetTier::Lightning, // 紧急卖出优先使用闪电档
            custom_priority_fee: None,
        }
    }

    /// 🔧 新增：创建无价格信息的紧急卖出信号
    pub fn emergency_sell_without_price(
        strategy_id: String,
        mint: Pubkey,
        token_amount: u64,
        reason: String,
    ) -> Self {
        // 🔧 新增：无价格信息的紧急卖出
        // 使用极高滑点容忍度和最低价格保护确保交易能执行

        Self {
            strategy_id,
            mint,
            signal_type: TradeSignalType::Sell,
            sol_amount: 1, // 🔧 无价格紧急卖出时设为1 lamport作为最低保护，将在to_trade_params中重新计算
            token_amount: Some(token_amount),
            max_slippage_bps: 9999, // 99.99% 滑点容忍度，基本不限制
            priority: SignalPriority::Critical,
            expires_at: Some(chrono::Utc::now().timestamp() + 60),
            reason: format!("EMERGENCY_NO_PRICE: {}", reason),
            created_at: chrono::Utc::now().timestamp(),
            metadata: std::collections::HashMap::new(),
            current_price: None, // 明确标记无价格信息
            price_source: Some("NO_PRICE_EMERGENCY".to_string()),
            creator: None, // 紧急卖出时可能没有创建者信息
            // 紧急卖出默认使用最高档位 - 将由StrategyManager设置
            compute_units: 0, // 占位值，在strategy层设置
            priority_fee_tier: ComputeBudgetTier::Lightning, // 紧急卖出优先使用闪电档
            custom_priority_fee: None,
        }
    }

    /// 🆕 新增：设置计算预算参数
    pub fn with_compute_budget(
        mut self,
        compute_units: u32,
        priority_fee_tier: ComputeBudgetTier,
    ) -> Self {
        self.compute_units = compute_units;
        self.priority_fee_tier = priority_fee_tier;
        self
    }
    
    /// 🆕 新增：设置自定义优先费
    pub fn with_custom_priority_fee(mut self, custom_priority_fee: u64) -> Self {
        self.custom_priority_fee = Some(custom_priority_fee);
        self
    }

    /// 检查信号是否过期
    pub fn is_expired(&self) -> bool {
        if let Some(expires_at) = self.expires_at {
            chrono::Utc::now().timestamp() > expires_at
        } else {
            false // 没有过期时间，永不过期
        }
    }

    /// 添加元数据
    pub fn with_metadata(mut self, key: String, value: String) -> Self {
        self.metadata.insert(key, value);
        self
    }

    /// 设置过期时间
    pub fn with_expiry(mut self, expires_at: i64) -> Self {
        self.expires_at = Some(expires_at);
        self
    }

    /// 设置优先级
    pub fn with_priority(mut self, priority: SignalPriority) -> Self {
        self.priority = priority;
        self
    }

    /// 🔧 新增：设置创建者地址
    pub fn with_creator(mut self, creator: Pubkey) -> Self {
        self.creator = Some(creator);
        self
    }

    /// 🔧 新增：设置价格信息
    pub fn with_price(mut self, current_price: f64, price_source: String) -> Self {
        self.current_price = Some(current_price);
        self.price_source = Some(price_source);
        // 如果是卖出信号，重新计算SOL金额
        if matches!(self.signal_type, TradeSignalType::Sell) {
            if let Some(token_amount) = self.token_amount {
                let token_amount_f64 = token_amount as f64;
                let updated_sol_amount = token_amount_f64 * current_price;
                self.sol_amount = updated_sol_amount as u64;
            }
        }
        self
    }

    /// 🔧 简化：验证交易信号的合理性
    pub fn validate(&self) -> Result<(), String> {
        // 验证mint地址
        if self.mint == Pubkey::default() {
            return Err("无效的mint地址".to_string());
        }

        // 验证滑点范围 - 🔧 修复：紧急卖出允许更高滑点
        if self.max_slippage_bps > 9999 { // 99.99% - 允许紧急卖出使用极高滑点
            return Err("滑点过大，超过99.99%".to_string());
        }
        
        // 🔧 新增：对非紧急交易的额外滑点检查
        if self.max_slippage_bps > 5000 && self.priority != SignalPriority::Critical {
            return Err("非紧急交易滑点过大，超过50%".to_string());
        }

        // 验证交易类型特定的参数
        match self.signal_type {
            TradeSignalType::Buy => {
                if self.sol_amount == 0 {
                    return Err("买入交易的SOL金额不能为0".to_string());
                }
                if self.sol_amount < 1_000_000 { // 0.001 SOL
                    return Err("买入金额太小，最少0.001 SOL".to_string());
                }
                if self.sol_amount > 100_000_000_000 { // 100 SOL
                    return Err("买入金额太大，最多100 SOL".to_string());
                }
            }
            TradeSignalType::Sell => {
                if self.token_amount.is_none() || self.token_amount.unwrap() == 0 {
                    return Err("卖出交易的代币数量不能为0".to_string());
                }
                // 🔧 简化：移除持仓检查，由策略层面负责
            }
            TradeSignalType::Cancel => {
                // 取消信号无特殊验证
            }
        }

        // 验证过期时间
        if let Some(expires_at) = self.expires_at {
            let now = chrono::Utc::now().timestamp();
            if expires_at <= now {
                return Err("信号已过期".to_string());
            }
            if expires_at - now > 3600 { // 1小时
                return Err("过期时间过长，最长1小时".to_string());
            }
        }

        Ok(())
    }

    /// 转换为交易参数 - 🔧 改进版：使用真实价格进行精确滑点计算
    pub fn to_trade_params(&self) -> crate::executor::TradeParams {
        let sol_amount = if matches!(self.signal_type, TradeSignalType::Buy) {
            self.sol_amount
        } else {
            // 🔧 修复：卖出交易时sol_amount设为0，不需要输入SOL
            0
        };

        let min_tokens_out = if matches!(self.signal_type, TradeSignalType::Buy) {
            if let Some(current_price) = self.current_price {
                // ✅ 使用真实价格计算滑点保护
                let expected_tokens = (self.sol_amount as f64) / current_price;
                let slippage_factor = 1.0 - (self.max_slippage_bps as f64 / 10_000.0);
                let min_tokens = expected_tokens * slippage_factor;
                
                info!("💰 精确滑点计算 | 价格: {:.9} SOL/token | 期望: {:.0} tokens | 最小: {:.0} tokens | 滑点: {}%", 
                      current_price, expected_tokens, min_tokens, self.max_slippage_bps as f64 / 100.0);
                
                min_tokens as u64
            } else {
                // ⚠️ 没有价格信息时的错误处理：拒绝执行而不是使用估算
                warn!("❌ 买入信号缺少价格信息，无法进行精确滑点保护！建议使用 buy_with_price 创建信号");
                // 返回一个很大的值来阻止交易执行，强制要求提供价格
                u64::MAX 
            }
        } else {
            // 🔧 修复：卖出时min_tokens_out设为0，不相关
            0
        };

        let min_sol_out = if matches!(self.signal_type, TradeSignalType::Buy) {
            None // 买入交易不需要最小SOL输出
        } else {
            // 🔧 修复：卖出交易需要设置最小SOL输出
            if let Some(current_price) = self.current_price {
                if let Some(token_amount) = self.token_amount {
                    // ✅ 使用真实价格计算最小SOL输出（滑点保护）
                    let expected_sol = token_amount as f64 * current_price;
                    let slippage_factor = 1.0 - (self.max_slippage_bps as f64 / 10_000.0);
                    let min_sol = expected_sol * slippage_factor;
                    
                    info!("💸 卖出滑点计算: 代币={}, 价格={:.9}, 期望SOL={:.4}, 最小SOL={:.4}, 滑点={}%", 
                          token_amount, current_price, expected_sol / 1_000_000_000.0, 
                          min_sol / 1_000_000_000.0, self.max_slippage_bps as f64 / 100.0);
                    
                    Some(min_sol as u64)
                } else {
                    warn!("⚠️ 卖出信号缺少token_amount，无法计算滑点保护");
                    Some(1) // 1 lamport 最低保护
                }
            } else {
                // 无价格信息时的处理
                if self.reason.starts_with("EMERGENCY_NO_PRICE:") {
                    warn!("🚨 无价格紧急卖出，使用最低保护价格");
                    Some(1) // 1 lamport 最低保护
                } else {
                    warn!("⚠️ 卖出信号缺少价格信息，使用最低保护价格");
                    Some(1) // 1 lamport 最低保护
                }
            }
        };

        // 记录价格来源信息
        if let Some(ref price_source) = self.price_source {
            if price_source == "NO_PRICE_EMERGENCY" {
                warn!("🚨 紧急卖出模式：无价格信息，使用最低保护执行");
            } else {
                info!("📊 价格来源: {} | 当前价格: {:.9} SOL/token", 
                      price_source, self.current_price.unwrap_or(0.0));
            }
        }
        
        // 📊 记录计算预算信息
        info!("⚡ 计算预算: CU={}, 档位={}, 自定义费={:?}", 
              self.compute_units, self.priority_fee_tier.as_str(), self.custom_priority_fee);

        // 🔧 调试：记录卖出交易的参数信息
        if matches!(self.signal_type, TradeSignalType::Sell) {
            info!("🔍 卖出信号参数检查:");
            info!("   🪙 token_amount: {:?}", self.token_amount);
            info!("   💰 current_price: {:?}", self.current_price);
            info!("   📊 max_slippage_bps: {}", self.max_slippage_bps);
            info!("   👤 creator: {:?}", self.creator);
        }

        crate::executor::TradeParams {
            mint: self.mint,
            sol_amount,
            min_tokens_out,
            token_amount: if matches!(self.signal_type, TradeSignalType::Buy) {
                None // 买入交易不需要代币数量
            } else {
                // 🔧 修复：卖出交易需要设置代币数量
                let token_amount = self.token_amount;
                if token_amount.is_none() {
                    warn!("⚠️ 卖出信号缺少token_amount，这可能导致交易失败");
                }
                token_amount
            },
            min_sol_out,
            max_slippage_bps: self.max_slippage_bps,
            is_buy: matches!(self.signal_type, TradeSignalType::Buy),
            creator: self.creator, // ✅ 传递创建者地址
        }
    }
}

impl Default for SignalPriority {
    fn default() -> Self {
        SignalPriority::Medium
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use solana_sdk::pubkey::Pubkey;

    #[test]
    fn test_buy_signal_creation() {
        let mint = Pubkey::new_unique();
        let signal = TradeSignal::buy_with_price(
            "test-strategy".to_string(),
            mint,
            1000000000, // 1 SOL
            300, // 3%
            "Test buy signal".to_string(),
            0.000001, // 1 token = 0.000001 SOL
            "PumpFun-Buy".to_string(),
        );

        assert_eq!(signal.signal_type, TradeSignalType::Buy);
        assert_eq!(signal.mint, mint);
        assert_eq!(signal.sol_amount, 1000000000);
        assert_eq!(signal.max_slippage_bps, 300);
        assert!(signal.token_amount.is_none());
        assert_eq!(signal.current_price, Some(0.000001));
        assert_eq!(signal.price_source, Some("PumpFun-Buy".to_string()));
    }

    #[test]
    fn test_sell_signal_creation() {
        let mint = Pubkey::new_unique();
        let signal = TradeSignal::sell_with_price(
            "test-strategy".to_string(),
            mint,
            1000000, // 1M tokens
            300, // 3%
            "Test sell signal".to_string(),
            0.000001, // 1 token = 0.000001 SOL
            "PumpFun-Sell".to_string(),
        );

        assert_eq!(signal.signal_type, TradeSignalType::Sell);
        assert_eq!(signal.mint, mint);
        assert_eq!(signal.sol_amount, 0); // 🔧 新设计：卖出信号不预设SOL金额
        assert_eq!(signal.token_amount, Some(1000000));
        assert_eq!(signal.max_slippage_bps, 300);
        assert_eq!(signal.current_price, Some(0.000001));
        assert_eq!(signal.price_source, Some("PumpFun-Sell".to_string()));
    }

    #[test]
    fn test_sell_signal_with_price_creation() {
        let mint = Pubkey::new_unique();
        let signal = TradeSignal::sell_with_price(
            "test-strategy".to_string(),
            mint,
            1000000, // 1M tokens
            300, // 3%
            "Test sell signal with price".to_string(),
            0.000001, // 1 token = 0.000001 SOL
            "PumpFun-Buy".to_string(),
        );

        assert_eq!(signal.signal_type, TradeSignalType::Sell);
        assert_eq!(signal.mint, mint);
        assert_eq!(signal.sol_amount, 0); // 🔧 新设计：卖出信号不预设SOL金额
        assert_eq!(signal.token_amount, Some(1000000));
        assert_eq!(signal.max_slippage_bps, 300);
        assert_eq!(signal.current_price, Some(0.000001));
        assert_eq!(signal.price_source, Some("PumpFun-Buy".to_string()));
    }

    #[test]
    fn test_emergency_sell_signal_without_price_creation() {
        let mint = Pubkey::new_unique();
        let signal = TradeSignal::emergency_sell_without_price(
            "test-strategy".to_string(),
            mint,
            1000000, // 1M tokens
            "Emergency exit without price".to_string(),
        );

        assert_eq!(signal.signal_type, TradeSignalType::Sell);
        assert_eq!(signal.mint, mint);
        assert_eq!(signal.sol_amount, 1); // 1 lamport 最低保护
        assert_eq!(signal.token_amount, Some(1000000));
        assert_eq!(signal.max_slippage_bps, 9999); // 不限制滑点
        assert_eq!(signal.priority, SignalPriority::Critical);
        assert_eq!(signal.current_price, None); // 无价格信息
        assert_eq!(signal.price_source, Some("NO_PRICE_EMERGENCY".to_string()));
        assert!(signal.reason.starts_with("EMERGENCY_NO_PRICE:"));
    }

    #[test]
    fn test_emergency_sell_signal_with_price_creation() {
        let mint = Pubkey::new_unique();
        let signal = TradeSignal::emergency_sell_with_price(
            "test-strategy".to_string(),
            mint,
            1000000, // 1M tokens
            "Emergency exit".to_string(),
            0.000001, // 1 token = 0.000001 SOL
            "PumpFun-Buy".to_string(),
        );

        assert_eq!(signal.signal_type, TradeSignalType::Sell);
        assert_eq!(signal.mint, mint);
        assert_eq!(signal.sol_amount, 0); // 🔧 新设计：紧急卖出也不预设SOL金额
        assert_eq!(signal.token_amount, Some(1000000));
        assert_eq!(signal.max_slippage_bps, 9999); // 不限制滑点
        assert_eq!(signal.priority, SignalPriority::Critical);
        assert_eq!(signal.current_price, Some(0.000001));
        assert_eq!(signal.price_source, Some("PumpFun-Buy".to_string()));
        assert!(signal.reason.starts_with("EMERGENCY:"));
    }

    #[test]
    fn test_signal_expiry() {
        let mut signal = TradeSignal::buy_with_price(
            "test-strategy".to_string(),
            Pubkey::new_unique(),
            1000000000,
            300,
            "Test signal".to_string(),
            0.000001,
            "PumpFun-Buy".to_string(),
        );

        // 设置为已过期的时间
        signal.expires_at = Some(chrono::Utc::now().timestamp() - 1);
        assert!(signal.is_expired());

        // 设置为未过期的时间
        signal.expires_at = Some(chrono::Utc::now().timestamp() + 3600);
        assert!(!signal.is_expired());

        // 无过期时间
        signal.expires_at = None;
        assert!(!signal.is_expired());
    }

    #[test]
    fn test_signal_metadata() {
        let signal = TradeSignal::buy_with_price(
            "test-strategy".to_string(),
            Pubkey::new_unique(),
            1000000000,
            300,
            "Test signal".to_string(),
            0.000001,
            "PumpFun-Buy".to_string(),
        )
        .with_metadata("test_key".to_string(), "test_value".to_string())
        .with_priority(SignalPriority::Critical);

        assert_eq!(signal.metadata.get("test_key"), Some(&"test_value".to_string()));
        assert_eq!(signal.priority, SignalPriority::Critical);
    }

    #[test]
    fn test_to_trade_params_buy_with_price() {
        let mint = Pubkey::new_unique();
        let signal = TradeSignal::buy_with_price(
            "test-strategy".to_string(),
            mint,
            1000000000, // 1 SOL
            300, // 3%
            "Test buy with price".to_string(),
            0.000001, // 1 token = 0.000001 SOL
            "PumpFun-Buy".to_string(),
        );

        let params = signal.to_trade_params();
        
        assert_eq!(params.mint, mint);
        assert_eq!(params.sol_amount, 1000000000); // 输入的SOL金额
        assert_eq!(params.is_buy, true);
        
        // 验证滑点保护计算
        let expected_tokens = 1000000000.0 / 0.000001; // 1 SOL / 0.000001 SOL/token
        let min_tokens_expected = expected_tokens * 0.97; // 97% (3%滑点)
        assert_eq!(params.min_tokens_out, min_tokens_expected as u64);
    }

    #[test]
    fn test_to_trade_params_sell_with_price() {
        let mint = Pubkey::new_unique();
        let signal = TradeSignal::sell_with_price(
            "test-strategy".to_string(),
            mint,
            1000000, // 1M tokens
            300, // 3%
            "Test sell with price".to_string(),
            0.000001, // 1 token = 0.000001 SOL
            "PumpFun-Buy".to_string(),
        );

        let params = signal.to_trade_params();
        
        assert_eq!(params.mint, mint);
        assert_eq!(params.min_tokens_out, 0); // 要卖出的代币数量
        assert_eq!(params.is_buy, false);
        
        // 验证滑点保护计算
        let expected_sol = 1000000.0 * 0.000001; // 1M tokens * 0.000001 SOL/token  
        let min_sol_expected = expected_sol * 0.97; // 97% (3%滑点)
        assert_eq!(params.sol_amount, 0); // 卖出时sol_amount为0
        assert_eq!(params.min_sol_out, Some(min_sol_expected as u64));
    }
}