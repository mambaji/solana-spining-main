use anyhow::Result;
use log::info;
use serde::{Deserialize, Serialize};
use yellowstone_grpc_proto::geyser::SubscribeUpdateTransactionInfo;

use crate::idl::IdlTransactionProcessor;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TransactionType {
    TokenCreation,
    Buy,
    Sell,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenEvent {
    pub signature: String,
    pub slot: u64,
    pub mint: Option<String>,
    pub transaction_type: TransactionType,
    pub detection_method: String,
    pub program_logs: Vec<String>,
    pub account_keys: Vec<String>,
    pub sol_amount: Option<u64>,
    pub token_amount: Option<u64>,
    /// 创建者/交易者钱包地址
    pub creator_wallet: Option<String>,
    /// 交易时间戳
    pub timestamp: Option<i64>,
    /// 完整的原始事件数据
    pub raw_data: Option<serde_json::Value>,
}

#[derive(Debug, Clone)]
pub struct TransactionProcessor {
    pub idl_processor: Option<IdlTransactionProcessor>,
}

impl TransactionProcessor {
    pub fn new() -> Result<Self> {
        // 尝试创建IDL处理器
        let idl_processor = match IdlTransactionProcessor::new() {
            Ok(processor) => {
                info!("✅ IDL处理器初始化成功，将使用IDL解析方式");
                Some(processor)
            }
            Err(e) => {
                info!("⚠️  IDL处理器初始化失败，将使用传统解析方式: {}", e);
                None
            }
        };

        Ok(Self {
            idl_processor,
        })
    }

    /// Process a transaction and detect token events using IDL
    pub async fn process_transaction(
        &self,
        txn_info: &SubscribeUpdateTransactionInfo,
        slot: u64,
    ) -> Option<TokenEvent> {
        // 使用IDL解析
        if let Some(ref idl_processor) = self.idl_processor {
            return idl_processor.process_transaction_with_idl(txn_info, slot).await;
        }

        None
    }
}

impl Default for TransactionProcessor {
    fn default() -> Self {
        Self::new().expect("Failed to create TransactionProcessor")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_processor_creation() {
        let processor = TransactionProcessor::new();
        assert!(processor.is_ok());
    }
}