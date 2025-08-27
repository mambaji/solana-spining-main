use anyhow::Result;
use log::{info, error};
use serde_json;
use std::fs::OpenOptions;
use std::io::Write;
use chrono::{DateTime, Utc};

use crate::processors::{TokenEvent, TransactionType};

pub struct EventLogger {
    log_file_path: String,
    enabled: bool,
}

impl EventLogger {
    pub fn new(log_file_path: Option<String>) -> Self {
        let log_file_path = log_file_path.unwrap_or_else(|| {
            let now = Utc::now();
            format!("token_events_{}.jsonl", now.format("%Y%m%d_%H%M%S"))
        });
        
        Self {
            log_file_path,
            enabled: true,
        }
    }

    /// 记录事件到JSON日志文件
    pub async fn log_event(&self, event: &TokenEvent) -> Result<()> {
        if !self.enabled {
            return Ok(());
        }

        let json_line = serde_json::to_string(event)?;
        
        // 异步写入文件以避免阻塞主线程
        let log_file_path = self.log_file_path.clone();
        tokio::task::spawn_blocking(move || -> Result<()> {
            let mut file = OpenOptions::new()
                .create(true)
                .append(true)
                .open(&log_file_path)?;
            
            writeln!(file, "{}", json_line)?;
            file.flush()?;
            Ok(())
        }).await??;

        Ok(())
    }

    /// 格式化并打印代币创建事件
    pub fn print_token_creation_event(&self, event: &TokenEvent) {
        info!("🎉 ===== 新代币创建检测 =====");
        info!("   🆔 交易签名: {}", event.signature);
        info!("   📍 Slot: {}", event.slot);
        
        if let Some(mint) = &event.mint {
            info!("   🪙 代币地址: {}", mint);
        }
        
        if let Some(creator) = &event.creator_wallet {
            info!("   👤 创建者钱包: {}", creator);
        }
        
        // 检查是否包含买入信息（创建后立即买入）
        let has_buy_info = event.sol_amount.is_some() && event.token_amount.is_some();
        
        if has_buy_info {
            info!("   💫 **创建后立即买入**");
            
            if let Some(sol_amount) = event.sol_amount {
                info!("   💰 买入金额: {:.4} SOL", sol_amount as f64 / 1_000_000_000.0);
            }
            
            if let Some(token_amount) = event.token_amount {
                info!("   🎯 最小代币输出: {} tokens", Self::format_large_number(token_amount));
            }
        } else {
            // 没有买入信息时，可能是创建成本信息
            if let Some(sol_amount) = event.sol_amount {
                info!("   💰 创建成本: {:.4} SOL", sol_amount as f64 / 1_000_000_000.0);
            }
            
            if let Some(token_amount) = event.token_amount {
                info!("   🪙 初始供应量: {} tokens", Self::format_large_number(token_amount));
            }
        }
        
        if let Some(timestamp) = event.timestamp {
            let dt = DateTime::from_timestamp(timestamp, 0).unwrap_or_else(|| Utc::now());
            info!("   ⏰ 时间: {}", dt.format("%Y-%m-%d %H:%M:%S UTC"));
        }
        
        info!("   🔍 检测方法: {}", event.detection_method);
        
        // 如果包含买入信息，提供额外的提示
        if has_buy_info {
            info!("   ⚡ 这是一个创建+买入组合交易！");
        }
        
        info!("=============================");
    }

    /// 格式化并打印买入事件
    pub fn print_buy_event(&self, event: &TokenEvent) {
        info!("💰 ===== 代币买入交易 =====");
        info!("   🆔 交易签名: {}", event.signature);
        
        if let Some(mint) = &event.mint {
            info!("   🪙 代币地址: {}", mint);
        }
        
        if let Some(trader) = &event.creator_wallet {
            info!("   👤 买入者钱包: {}", trader);
        }
        
        if let Some(sol_amount) = event.sol_amount {
            info!("   💸 买入金额: {:.9} SOL", sol_amount as f64 / 1_000_000_000.0);
        }
        
        if let Some(token_amount) = event.token_amount {
            info!("   🪙 获得代币: {} tokens", Self::format_large_number(token_amount));
        }
        
        if let Some(timestamp) = event.timestamp {
            let dt = DateTime::from_timestamp(timestamp, 0).unwrap_or_else(|| Utc::now());
            info!("   ⏰ 时间: {}", dt.format("%Y-%m-%d %H:%M:%S UTC"));
        }
        
        info!("   📍 Slot: {}", event.slot);
        info!("   🔍 检测方法: {}", event.detection_method);
        info!("============================");
    }

    /// 格式化并打印卖出事件
    pub fn print_sell_event(&self, event: &TokenEvent) {
        info!("💸 ===== 代币卖出交易 =====");
        info!("   🆔 交易签名: {}", event.signature);
        
        if let Some(mint) = &event.mint {
            info!("   🪙 代币地址: {}", mint);
        }
        
        if let Some(trader) = &event.creator_wallet {
            info!("   👤 卖出者钱包: {}", trader);
        }
        
        if let Some(token_amount) = event.token_amount {
            info!("   🪙 卖出代币: {} tokens", Self::format_large_number(token_amount));
        }
        
        if let Some(sol_amount) = event.sol_amount {
            info!("   💰 获得SOL: {:.4} SOL", sol_amount as f64 / 1_000_000_000.0);
        }
        
        if let Some(timestamp) = event.timestamp {
            let dt = DateTime::from_timestamp(timestamp, 0).unwrap_or_else(|| Utc::now());
            info!("   ⏰ 时间: {}", dt.format("%Y-%m-%d %H:%M:%S UTC"));
        }
        
        info!("   📍 Slot: {}", event.slot);
        info!("   🔍 检测方法: {}", event.detection_method);
        info!("============================");
    }

    /// 统一的事件处理入口
    pub async fn handle_event(&self, event: &TokenEvent) -> Result<()> {
        // 记录到JSON文件
        if let Err(e) = self.log_event(event).await {
            error!("❌ 记录事件到JSON文件失败: {}", e);
        }

        // 根据事件类型打印格式化信息
        match event.transaction_type {
            TransactionType::TokenCreation => {
                self.print_token_creation_event(event);
            }
            TransactionType::Buy => {
                // self.print_buy_event(event);
            }
            TransactionType::Sell => {
                // self.print_sell_event(event);
            }
            TransactionType::Unknown => {
                info!("🔍 检测到未知类型交易: {}", event.signature);
            }
        }

        Ok(())
    }

    /// 格式化大数字显示
    fn format_large_number(num: u64) -> String {
        if num >= 1_000_000_000 {
            format!("{:.2}B", num as f64 / 1_000_000_000.0)
        } else if num >= 1_000_000 {
            format!("{:.2}M", num as f64 / 1_000_000.0)
        } else if num >= 1_000 {
            format!("{:.2}K", num as f64 / 1_000.0)
        } else {
            num.to_string()
        }
    }

    /// 设置日志记录开关
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    /// 获取日志文件路径
    pub fn get_log_file_path(&self) -> &str {
        &self.log_file_path
    }
}

impl Default for EventLogger {
    fn default() -> Self {
        Self::new(None)
    }
}