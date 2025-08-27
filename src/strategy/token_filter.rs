use log::{debug, info};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::processors::{TokenEvent, TransactionType};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilterCriteria {
    // 基础筛选条件
    pub min_sol_amount: Option<u64>,           // 最小SOL交易量
    pub max_sol_amount: Option<u64>,           // 最大SOL交易量
    pub min_token_amount: Option<u64>,         // 最小代币交易量
    pub max_token_amount: Option<u64>,         // 最大代币交易量
    
    // 代币名称筛选
    pub required_name_keywords: Vec<String>,   // 代币名称必须包含的关键词
    pub forbidden_name_keywords: Vec<String>,  // 代币名称禁止包含的关键词
    pub min_name_length: Option<usize>,        // 代币名称最小长度
    pub max_name_length: Option<usize>,        // 代币名称最大长度
    
    // 代币符号筛选
    pub required_symbol_keywords: Vec<String>, // 代币符号必须包含的关键词
    pub forbidden_symbol_keywords: Vec<String>, // 代币符号禁止包含的关键词
    pub min_symbol_length: Option<usize>,      // 代币符号最小长度
    pub max_symbol_length: Option<usize>,      // 代币符号最大长度
    
    // 时间相关筛选
    pub max_creation_age_slots: Option<u64>,   // 代币创建后最大slot数（新鲜度检查）
    
    // 交易类型筛选
    pub allowed_transaction_types: Vec<TransactionType>, // 允许的交易类型
    
    // 黑白名单
    pub whitelist_mints: Vec<String>,          // 白名单mint地址
    pub blacklist_mints: Vec<String>,          // 黑名单mint地址
    pub whitelist_programs: Vec<String>,       // 白名单程序地址
    pub blacklist_programs: Vec<String>,       // 黑名单程序地址
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilterResult {
    pub passed: bool,
    pub reason: String,
    pub score: f64,                            // 评分 (0.0-1.0)
    pub matched_criteria: Vec<String>,         // 匹配的条件
    pub failed_criteria: Vec<String>,          // 未通过的条件
}

#[derive(Debug, Clone)]
pub struct TokenFilter {
    pub criteria: FilterCriteria,
    mint_scores: HashMap<String, f64>,         // mint地址的历史评分
}

impl TokenFilter {
    pub fn new(criteria: FilterCriteria) -> Self {
        Self {
            criteria,
            mint_scores: HashMap::new(),
        }
    }

    /// 创建默认的筛选策略（适合新项目狙击）
    pub fn default_sniper_strategy() -> Self {
        let criteria = FilterCriteria {
            // SOL交易量筛选：寻找中等规模的交易
            min_sol_amount: Some(100_000_000),     // 最少0.1 SOL
            max_sol_amount: Some(10_000_000_000),  // 最多10 SOL
            
            min_token_amount: None,
            max_token_amount: None,
            
            // 代币名称筛选：避免明显的垃圾币
            required_name_keywords: vec![],
            forbidden_name_keywords: vec![
                "test".to_string(),
                "fake".to_string(), 
                "scam".to_string(),
                "rug".to_string(),
                "shit".to_string(),
                "pump".to_string(),
                "dump".to_string(),
            ],
            min_name_length: Some(2),
            max_name_length: Some(30),
            
            // 代币符号筛选
            required_symbol_keywords: vec![],
            forbidden_symbol_keywords: vec![
                "test".to_string(),
                "fake".to_string(),
                "scam".to_string(),
            ],
            min_symbol_length: Some(1),
            max_symbol_length: Some(10),
            
            // 时间筛选：只关注新创建的代币
            max_creation_age_slots: Some(100),     // 100个slot内的新币
            
            // 交易类型：只关注代币创建事件
            allowed_transaction_types: vec![
                TransactionType::TokenCreation,
            ],
            
            // 黑白名单（初始为空）
            whitelist_mints: vec![],
            blacklist_mints: vec![],
            whitelist_programs: vec![],
            blacklist_programs: vec![],
        };

        Self::new(criteria)
    }

    /// 创建保守的筛选策略（适合稳健投资）
    pub fn conservative_strategy() -> Self {
        let criteria = FilterCriteria {
            min_sol_amount: Some(1_000_000_000),   // 最少1 SOL
            max_sol_amount: Some(100_000_000_000), // 最多100 SOL
            
            min_token_amount: None,
            max_token_amount: None,
            
            required_name_keywords: vec![],
            forbidden_name_keywords: vec![
                "test".to_string(), "fake".to_string(), "scam".to_string(),
                "rug".to_string(), "shit".to_string(), "meme".to_string(),
                "pump".to_string(), "dump".to_string(), "moon".to_string(),
            ],
            min_name_length: Some(3),
            max_name_length: Some(20),
            
            required_symbol_keywords: vec![],
            forbidden_symbol_keywords: vec![
                "test".to_string(), "fake".to_string(), "scam".to_string(),
                "xxx".to_string(), "shit".to_string(),
            ],
            min_symbol_length: Some(2),
            max_symbol_length: Some(8),
            
            max_creation_age_slots: Some(1000),
            
            allowed_transaction_types: vec![
                TransactionType::TokenCreation,
                TransactionType::Buy,
            ],
            
            whitelist_mints: vec![],
            blacklist_mints: vec![],
            whitelist_programs: vec![],
            blacklist_programs: vec![],
        };

        Self::new(criteria)
    }

    /// 主要的筛选方法
    pub fn evaluate_token(&mut self, token_event: &TokenEvent) -> FilterResult {
        let mut score = 0.0;
        let mut max_score = 0.0;
        let mut matched_criteria = Vec::new();
        let mut failed_criteria = Vec::new();
        let mut reason = String::new();

        debug!("🔍 开始评估代币: mint={:?}, type={:?}", 
            token_event.mint, token_event.transaction_type);

        // 1. 检查交易类型
        max_score += 1.0;
        if self.criteria.allowed_transaction_types.contains(&token_event.transaction_type) {
            score += 1.0;
            matched_criteria.push("交易类型匹配".to_string());
        } else {
            failed_criteria.push(format!("交易类型不匹配: {:?}", token_event.transaction_type));
        }

        // 2. 检查mint地址黑白名单
        if let Some(ref mint) = token_event.mint {
            max_score += 1.0;
            
            // 白名单检查
            if !self.criteria.whitelist_mints.is_empty() {
                if self.criteria.whitelist_mints.contains(mint) {
                    score += 1.0;
                    matched_criteria.push("mint在白名单中".to_string());
                } else {
                    failed_criteria.push("mint不在白名单中".to_string());
                }
            } else {
                // 黑名单检查
                if self.criteria.blacklist_mints.contains(mint) {
                    failed_criteria.push("mint在黑名单中".to_string());
                    reason = "mint在黑名单中".to_string();
                    return FilterResult {
                        passed: false,
                        reason,
                        score: 0.0,
                        matched_criteria,
                        failed_criteria,
                    };
                } else {
                    score += 1.0;
                    matched_criteria.push("mint不在黑名单中".to_string());
                }
            }
        }

        // 3. 检查SOL交易量
        if let Some(sol_amount) = token_event.sol_amount {
            max_score += 1.0;
            
            let mut amount_valid = true;
            
            if let Some(min) = self.criteria.min_sol_amount {
                if sol_amount < min {
                    amount_valid = false;
                    failed_criteria.push(format!("SOL金额过低: {} < {}", sol_amount, min));
                }
            }
            
            if let Some(max) = self.criteria.max_sol_amount {
                if sol_amount > max {
                    amount_valid = false;
                    failed_criteria.push(format!("SOL金额过高: {} > {}", sol_amount, max));
                }
            }
            
            if amount_valid {
                score += 1.0;
                matched_criteria.push("SOL金额在范围内".to_string());
            }
        }

        // 4. 检查代币名称和符号（通过程序日志提取）
        if let Some(token_info) = self.extract_token_info_from_logs(&token_event.program_logs) {
            max_score += 2.0; // 名称和符号各1分
            
            // 名称检查
            if let Some(ref name) = token_info.name {
                let name_lower = name.to_lowercase();
                let mut name_valid = true;
                
                // 长度检查
                if let Some(min_len) = self.criteria.min_name_length {
                    if name.len() < min_len {
                        name_valid = false;
                        failed_criteria.push(format!("代币名称过短: {} < {}", name.len(), min_len));
                    }
                }
                
                if let Some(max_len) = self.criteria.max_name_length {
                    if name.len() > max_len {
                        name_valid = false;
                        failed_criteria.push(format!("代币名称过长: {} > {}", name.len(), max_len));
                    }
                }
                
                // 必须包含关键词检查
                for keyword in &self.criteria.required_name_keywords {
                    if !name_lower.contains(&keyword.to_lowercase()) {
                        name_valid = false;
                        failed_criteria.push(format!("名称缺少必需关键词: {}", keyword));
                    }
                }
                
                // 禁止关键词检查
                for keyword in &self.criteria.forbidden_name_keywords {
                    if name_lower.contains(&keyword.to_lowercase()) {
                        name_valid = false;
                        failed_criteria.push(format!("名称包含禁止关键词: {}", keyword));
                        reason = format!("名称包含禁止关键词: {}", keyword);
                    }
                }
                
                if name_valid {
                    score += 1.0;
                    matched_criteria.push("代币名称符合要求".to_string());
                }
            }
            
            // 符号检查
            if let Some(ref symbol) = token_info.symbol {
                let symbol_lower = symbol.to_lowercase();
                let mut symbol_valid = true;
                
                // 长度检查
                if let Some(min_len) = self.criteria.min_symbol_length {
                    if symbol.len() < min_len {
                        symbol_valid = false;
                        failed_criteria.push(format!("符号过短: {} < {}", symbol.len(), min_len));
                    }
                }
                
                if let Some(max_len) = self.criteria.max_symbol_length {
                    if symbol.len() > max_len {
                        symbol_valid = false;
                        failed_criteria.push(format!("符号过长: {} > {}", symbol.len(), max_len));
                    }
                }
                
                // 必须包含关键词检查
                for keyword in &self.criteria.required_symbol_keywords {
                    if !symbol_lower.contains(&keyword.to_lowercase()) {
                        symbol_valid = false;
                        failed_criteria.push(format!("符号缺少必需关键词: {}", keyword));
                    }
                }
                
                // 禁止关键词检查
                for keyword in &self.criteria.forbidden_symbol_keywords {
                    if symbol_lower.contains(&keyword.to_lowercase()) {
                        symbol_valid = false;
                        failed_criteria.push(format!("符号包含禁止关键词: {}", keyword));
                        reason = format!("符号包含禁止关键词: {}", keyword);
                    }
                }
                
                if symbol_valid {
                    score += 1.0;
                    matched_criteria.push("代币符号符合要求".to_string());
                }
            }
        }

        // 计算最终得分
        let final_score = if max_score > 0.0 {
            score / max_score
        } else {
            0.0
        };

        // 更新历史评分
        if let Some(ref mint) = token_event.mint {
            self.mint_scores.insert(mint.clone(), final_score);
        }

        // 决定是否通过
        let passed = final_score >= 0.7 && failed_criteria.is_empty(); // 70%以上得分且无硬性失败条件

        if passed {
            info!("✅ 代币通过筛选: mint={:?}, 得分={:.2}", token_event.mint, final_score);
        } else {
            debug!("❌ 代币未通过筛选: mint={:?}, 得分={:.2}, 原因={}", 
                token_event.mint, final_score, 
                if reason.is_empty() { "得分不足" } else { &reason });
        }

        FilterResult {
            passed,
            reason: if reason.is_empty() {
                if passed {
                    "所有条件满足".to_string()
                } else {
                    format!("得分不足: {:.2}/1.0", final_score)
                }
            } else {
                reason
            },
            score: final_score,
            matched_criteria,
            failed_criteria,
        }
    }

    /// 从程序日志中提取代币信息
    fn extract_token_info_from_logs(&self, logs: &[String]) -> Option<TokenInfo> {
        let mut name = None;
        let mut symbol = None;
        let mut _uri = None;

        for log in logs {
            // PumpFun日志格式示例
            if log.contains("name:") {
                if let Some(start) = log.find("name:") {
                    let name_part = &log[start + 5..];
                    if let Some(end) = name_part.find(',') {
                        name = Some(name_part[..end].trim().trim_matches('"').to_string());
                    } else {
                        name = Some(name_part.trim().trim_matches('"').to_string());
                    }
                }
            }
            
            if log.contains("symbol:") {
                if let Some(start) = log.find("symbol:") {
                    let symbol_part = &log[start + 7..];
                    if let Some(end) = symbol_part.find(',') {
                        symbol = Some(symbol_part[..end].trim().trim_matches('"').to_string());
                    } else {
                        symbol = Some(symbol_part.trim().trim_matches('"').to_string());
                    }
                }
            }
            
            if log.contains("uri:") {
                if let Some(start) = log.find("uri:") {
                    let uri_part = &log[start + 4..];
                    if let Some(end) = uri_part.find(',') {
                        _uri = Some(uri_part[..end].trim().trim_matches('"').to_string());
                    } else {
                        _uri = Some(uri_part.trim().trim_matches('"').to_string());
                    }
                }
            }
        }

        if name.is_some() || symbol.is_some() {
            Some(TokenInfo { name, symbol })
        } else {
            None
        }
    }

    /// 获取mint的历史评分
    pub fn get_mint_score(&self, mint: &str) -> Option<f64> {
        self.mint_scores.get(mint).copied()
    }

    /// 更新筛选条件
    pub fn update_criteria(&mut self, new_criteria: FilterCriteria) {
        self.criteria = new_criteria;
    }

    /// 添加到黑名单
    pub fn add_to_blacklist(&mut self, mint: String) {
        if !self.criteria.blacklist_mints.contains(&mint) {
            self.criteria.blacklist_mints.push(mint);
        }
    }

    /// 添加到白名单
    pub fn add_to_whitelist(&mut self, mint: String) {
        if !self.criteria.whitelist_mints.contains(&mint) {
            self.criteria.whitelist_mints.push(mint);
        }
    }
}

#[derive(Debug, Clone)]
struct TokenInfo {
    name: Option<String>,
    symbol: Option<String>,
}

impl Default for TokenFilter {
    fn default() -> Self {
        Self::default_sniper_strategy()
    }
}

/// 便利函数：使用默认策略筛选代币
pub fn filter_token_with_default_strategy(token_event: &TokenEvent) -> FilterResult {
    let mut filter = TokenFilter::default_sniper_strategy();
    filter.evaluate_token(token_event)
}

/// 便利函数：使用保守策略筛选代币
pub fn filter_token_with_conservative_strategy(token_event: &TokenEvent) -> FilterResult {
    let mut filter = TokenFilter::conservative_strategy();
    filter.evaluate_token(token_event)
}
