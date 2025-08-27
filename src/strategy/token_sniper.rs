use anyhow::Result;
use log::{info, warn};
use yellowstone_grpc_proto::geyser::SubscribeUpdateTransactionInfo;

use crate::processors::{TransactionProcessor, TokenEvent};
use crate::strategy::{TokenFilter, FilterResult};

/// 集成示例：将数据解析和选币策略结合使用
pub struct TokenSniper {
    processor: TransactionProcessor,
    filter: TokenFilter,
}

impl TokenSniper {
    /// 创建新的代币狙击器，使用默认策略
    pub fn new() -> Result<Self> {
        let processor = TransactionProcessor::new()?;
        let filter = TokenFilter::default_sniper_strategy();
        
        info!("🎯 TokenSniper初始化完成，使用默认狙击策略");
        
        Ok(Self { processor, filter })
    }

    /// 创建新的代币狙击器，使用自定义策略
    pub fn new_with_filter(filter: TokenFilter) -> Result<Self> {
        let processor = TransactionProcessor::new()?;
        
        info!("🎯 TokenSniper初始化完成，使用自定义策略");
        
        Ok(Self { processor, filter })
    }

    /// 处理单个交易，返回符合条件的代币事件
    pub async fn process_transaction(
        &mut self,
        txn_info: &SubscribeUpdateTransactionInfo,
        slot: u64,
    ) -> Option<(TokenEvent, FilterResult)> {
        // 步骤1: 使用processor解析交易数据
        if let Some(token_event) = self.processor.process_transaction(txn_info, slot).await {
            info!("🔍 检测到代币事件: {:?}, mint: {:?}", 
                token_event.transaction_type, token_event.mint);

            // 步骤2: 使用选币策略进行筛选
            let filter_result = self.filter.evaluate_token(&token_event);
            
            if filter_result.passed {
                info!("🎯 代币通过选币策略筛选!");
                info!("   Mint: {:?}", token_event.mint);
                info!("   得分: {:.2}", filter_result.score);
                info!("   匹配条件: {:?}", filter_result.matched_criteria);
                info!("---");
                return Some((token_event, filter_result));
            } else {
                warn!("❌ 代币未通过选币策略筛选: {}", filter_result.reason);
                warn!("   Mint: {:?}", token_event.mint);
                warn!("   得分: {:.2}", filter_result.score);
                warn!("   未通过条件: {:?}", filter_result.failed_criteria);
                warn!("---");
                
                // 根据需要，可以选择是否返回未通过的结果
                // return Some((token_event, filter_result));
            }
        }

        None
    }

    /// 批量处理交易
    pub async fn process_transactions(
        &mut self,
        transactions: Vec<(SubscribeUpdateTransactionInfo, u64)>,
    ) -> Vec<(TokenEvent, FilterResult)> {
        let mut results = Vec::new();
        
        for (txn_info, slot) in transactions {
            if let Some(result) = self.process_transaction(&txn_info, slot).await {
                results.push(result);
            }
        }
        
        results
    }

    /// 更新筛选策略
    pub fn update_filter(&mut self, new_filter: TokenFilter) {
        self.filter = new_filter;
        info!("✅ 选币策略已更新");
    }

    /// 获取当前筛选策略的引用
    pub fn get_filter(&self) -> &TokenFilter {
        &self.filter
    }

    /// 获取当前筛选策略的可变引用
    pub fn get_filter_mut(&mut self) -> &mut TokenFilter {
        &mut self.filter
    }

    /// 添加mint到黑名单
    pub fn blacklist_mint(&mut self, mint: String) {
        self.filter.add_to_blacklist(mint);
        info!("🚫 mint已添加到黑名单");
    }

    /// 添加mint到白名单
    pub fn whitelist_mint(&mut self, mint: String) {
        self.filter.add_to_whitelist(mint);
        info!("✅ mint已添加到白名单");
    }

    /// 获取指定mint的历史评分
    pub fn get_mint_score(&self, mint: &str) -> Option<f64> {
        self.filter.get_mint_score(mint)
    }
}

impl Default for TokenSniper {
    fn default() -> Self {
        Self::new().expect("Failed to create TokenSniper")
    }
}

/// 示例使用方式
pub async fn example_token_sniper_usage() -> Result<()> {
    info!("🚀 TokenSniper使用示例");
    
    // 创建TokenSniper实例
    let mut sniper = TokenSniper::new()?;
    
    // 可以动态调整黑名单
    sniper.blacklist_mint("ScamTokenMint123...".to_string());
    sniper.whitelist_mint("GoodTokenMint456...".to_string());
    
    // 模拟处理交易流程
    info!("📡 开始监控交易流...");
    
    // 在实际使用中，这里会是来自Jito/Shyft/Letsbonk流的真实交易数据
    // if let Some((token_event, filter_result)) = sniper.process_transaction(&txn_info, slot).await {
    //     if filter_result.passed {
    //         // 执行狙击逻辑
    //         execute_snipe_logic(&token_event).await?;
    //     }
    // }
    
    info!("✅ TokenSniper示例完成");
    Ok(())
}

/// 示例：不同策略的TokenSniper
pub async fn example_different_strategies() -> Result<()> {
    info!("🔧 不同策略的TokenSniper示例");
    
    // 1. 激进策略 - 用于快速狙击新币
    let _aggressive_sniper = {
        let mut criteria = TokenFilter::default_sniper_strategy().criteria;
        criteria.min_sol_amount = Some(50_000_000); // 降低到0.05 SOL
        criteria.max_creation_age_slots = Some(10); // 只要10个slot内的极新币
        let filter = TokenFilter::new(criteria);
        TokenSniper::new_with_filter(filter)?
    };
    
    // 2. 保守策略 - 用于稳健投资
    let _conservative_sniper = {
        let filter = TokenFilter::conservative_strategy();
        TokenSniper::new_with_filter(filter)?
    };
    
    // 3. AI专门策略 - 只关注AI相关代币
    let _ai_focused_sniper = {
        let mut criteria = TokenFilter::default_sniper_strategy().criteria;
        criteria.required_name_keywords = vec!["AI".to_string(), "GPT".to_string(), "BOT".to_string()];
        criteria.min_sol_amount = Some(1_000_000_000); // 1 SOL
        let filter = TokenFilter::new(criteria);
        TokenSniper::new_with_filter(filter)?
    };
    
    info!("✅ 已创建3种不同策略的TokenSniper:");
    info!("   1. 激进策略 - 快速狙击新币");
    info!("   2. 保守策略 - 稳健投资");  
    info!("   3. AI专门策略 - AI相关代币");
    
    // 在实际使用中，可以根据市场情况选择不同的策略
    // let selected_sniper = match market_condition {
    //     MarketCondition::Bull => &mut aggressive_sniper,
    //     MarketCondition::Bear => &mut conservative_sniper,
    //     MarketCondition::AITrend => &mut ai_focused_sniper,
    // };
    
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_token_sniper_creation() {
        let sniper = TokenSniper::new();
        assert!(sniper.is_ok());
    }

    #[tokio::test] 
    async fn test_token_sniper_filter_update() {
        let mut sniper = TokenSniper::new().unwrap();
        let new_filter = TokenFilter::conservative_strategy();
        sniper.update_filter(new_filter);
        // Test passes if no panic
    }
}