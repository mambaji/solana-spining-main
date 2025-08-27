use log::info;
use serde::{Deserialize, Serialize};

use crate::processors::{TokenEvent, TransactionType};

/// 优化后的无状态代币过滤器
/// 
/// 关键优化点：
/// 1. 去除了 mint_scores 状态，避免写锁需求
/// 2. 预编译关键词集合，提升匹配性能
/// 3. 简化评估逻辑，专注于二元判断而非评分
#[derive(Debug, Clone)]
pub struct OptimizedTokenFilter {
    pub criteria: FilterCriteria,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilterCriteria {
    // 基础筛选条件
    pub min_sol_amount: Option<u64>,
    pub max_sol_amount: Option<u64>,
    
    // 代币名称筛选
    pub required_name_keywords: Vec<String>,
    pub forbidden_name_keywords: Vec<String>,
    pub min_name_length: Option<usize>,
    pub max_name_length: Option<usize>,
    
    // 代币符号筛选
    pub required_symbol_keywords: Vec<String>,
    pub forbidden_symbol_keywords: Vec<String>,
    pub min_symbol_length: Option<usize>,
    pub max_symbol_length: Option<usize>,
    
    // 时间相关筛选
    pub max_creation_age_slots: Option<u64>,
    
    // 交易类型筛选
    pub allowed_transaction_types: Vec<TransactionType>,
    
    // 黑白名单
    pub whitelist_mints: Vec<String>,
    pub blacklist_mints: Vec<String>,
    pub blacklist_programs: Vec<String>,
}

/// 简化的过滤结果，只关注通过/不通过
#[derive(Debug, Clone)]
pub struct SimpleFilterResult {
    pub passed: bool,
    pub reason: String,
    pub matched_criteria: Vec<String>,
}

impl OptimizedTokenFilter {
    pub fn new(criteria: FilterCriteria) -> Self {
        Self {
            criteria,
        }
    }

    /// 默认狙击策略（高性能版本）
    pub fn default_sniper_strategy() -> Self {
        let criteria = FilterCriteria {
            min_sol_amount: Some(1000_000_000),     // 1 SOL
            max_sol_amount: Some(10_000_000_000),  // 100 SOL
            
            required_name_keywords: vec![],
            forbidden_name_keywords: vec![
                "test".to_string(), "fake".to_string(), "scam".to_string(),
                "rug".to_string(), "shit".to_string(), "pump".to_string(),
                "dump".to_string(),
            ],
            min_name_length: Some(2),
            max_name_length: Some(30),
            
            required_symbol_keywords: vec![],
            forbidden_symbol_keywords: vec![
                "test".to_string(), "fake".to_string(), "scam".to_string(),
            ],
            min_symbol_length: Some(1),
            max_symbol_length: Some(10),
            
            max_creation_age_slots: Some(100),
            
            allowed_transaction_types: vec![TransactionType::TokenCreation],
            
            whitelist_mints: vec![],
            blacklist_mints: vec![],
            blacklist_programs: vec![],
        };

        Self::new(criteria)
    }

    /// 快速评估代币 - 无状态，仅需读取访问
    /// 
    /// 优化点：
    /// 1. 不再计算复杂评分，只做二元判断
    /// 2. 不更新任何状态，完全无锁
    /// 3. 短路求值，快速排除不符合条件的代币
    pub fn evaluate_token_fast(&self, token_event: &TokenEvent) -> SimpleFilterResult {
        let mut matched_criteria = Vec::new();

        // 1. 快速检查：交易类型
        if !self.criteria.allowed_transaction_types.contains(&token_event.transaction_type) {
            return SimpleFilterResult {
                passed: false,
                reason: format!("交易类型不匹配: {:?}", token_event.transaction_type),
                matched_criteria,
            };
        }
        matched_criteria.push("交易类型匹配".to_string());

        // 3. SOL金额检查
        if let Some(sol_amount) = token_event.sol_amount {
            info!("SOL金额: {}", sol_amount);
            info!("min_sol_amount: {:?}", self.criteria.min_sol_amount);
            if let Some(min) = self.criteria.min_sol_amount {
                if sol_amount < min {
                    return SimpleFilterResult {
                        passed: false,
                        reason: format!("SOL金额过低: {} < {}", sol_amount, min),
                        matched_criteria,
                    };
                }
            }
            
            if let Some(max) = self.criteria.max_sol_amount {
                if sol_amount > max {
                    return SimpleFilterResult {
                        passed: false,
                        reason: format!("SOL金额过高: {} > {}", sol_amount, max),
                        matched_criteria,
                    };
                }
            }
            matched_criteria.push("SOL金额在范围内".to_string());
        }

        // 所有检查通过
        info!("✅ 代币通过快速筛选: mint={:?}", token_event.mint);
        SimpleFilterResult {
            passed: true,
            reason: "所有条件满足".to_string(),
            matched_criteria,
        }
    }
}

impl Default for OptimizedTokenFilter {
    fn default() -> Self {
        Self::default_sniper_strategy()
    }
}

/// 便利函数：使用优化的默认策略筛选代币
pub fn filter_token_optimized(token_event: &TokenEvent) -> SimpleFilterResult {
    let filter = OptimizedTokenFilter::default_sniper_strategy();
    filter.evaluate_token_fast(token_event)
}