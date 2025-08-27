use anyhow::Result;
use backoff::future::retry;
use backoff::ExponentialBackoff;
use futures_util::StreamExt;
use futures_util::SinkExt;
use log::{debug, info, warn, error};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{Mutex, RwLock};
use tonic::transport::ClientTlsConfig;
use yellowstone_grpc_client::GeyserGrpcClient;
use yellowstone_grpc_proto::geyser::{
    SubscribeRequest, SubscribeRequestFilterTransactions, SubscribeRequestPing
};
use yellowstone_grpc_proto::prelude::{
    subscribe_update::UpdateOneof, CommitmentLevel
};

use crate::config::StreamShyftConfig;
use crate::processors::{TransactionProcessor, TokenEvent, TransactionType};

// Program IDs
const PUMPFUN_PROGRAM_ID: &str = "6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P";

#[derive(Debug, Clone)]
pub struct ShyftMetrics {
    pub transactions_processed: u64,
    pub tokens_detected: u64,
    pub connection_errors: u64,
    pub reconnection_attempts: u64,
    pub last_connection_time: Option<Instant>,
}

#[derive(Debug, Clone)]
pub struct ConnectionStatus {
    pub is_connected: bool,
    pub reconnection_attempts: u64,
    pub last_connection_time: Option<Instant>,
    pub connection_errors: u64,
}

impl Default for ShyftMetrics {
    fn default() -> Self {
        Self {
            transactions_processed: 0,
            tokens_detected: 0,
            connection_errors: 0,
            reconnection_attempts: 0,
            last_connection_time: None,
        }
    }
}

pub struct ShyftStream {
    config: StreamShyftConfig,
    metrics: Arc<RwLock<ShyftMetrics>>,
    zero_attempts: Arc<Mutex<bool>>,
}

impl ShyftStream {
    pub fn new(config: StreamShyftConfig) -> Self {
        Self { 
            config,
            metrics: Arc::new(RwLock::new(ShyftMetrics::default())),
            zero_attempts: Arc::new(Mutex::new(true)),
        }
    }
    
    pub async fn get_metrics(&self) -> ShyftMetrics {
        self.metrics.read().await.clone()
    }

    /// Get connection status information
    pub async fn get_connection_status(&self) -> ConnectionStatus {
        let metrics = self.metrics.read().await;
        
        ConnectionStatus {
            is_connected: false, // We don't maintain persistent connection
            reconnection_attempts: metrics.reconnection_attempts,
            last_connection_time: metrics.last_connection_time,
            connection_errors: metrics.connection_errors,
        }
    }
    
    pub async fn start_streaming<F>(&self, callback: F) -> Result<()>
    where
        F: Fn(TokenEvent) -> Result<()> + Send + Sync + 'static,
    {
        info!("🚀 Starting Shyft gRPC stream for PumpFun new tokens...");
        
        let callback = Arc::new(callback);
        let config = self.config.clone();
        
        retry(ExponentialBackoff::default(), || {
            let config = config.clone();
            let metrics = Arc::clone(&self.metrics);
            let zero_attempts = Arc::clone(&self.zero_attempts);
            let callback = Arc::clone(&callback);
            
            async move {
                let mut zero_attempts = zero_attempts.lock().await;
                if *zero_attempts {
                    *zero_attempts = false;
                } else {
                    info!("Retry to connect to the server");
                }
                drop(zero_attempts);

                // Create a fresh client for each connection attempt
                let mut client = {
                    // Update metrics before attempting connection
                    {
                        let mut metrics = metrics.write().await;
                        metrics.reconnection_attempts += 1;
                        metrics.last_connection_time = Some(Instant::now());
                    }
                    connect_client(&config).await.map_err(backoff::Error::transient)?
                };
                info!("Connected to Shyft gRPC");

                let request = build_subscribe_request(&config).map_err(backoff::Error::Permanent)?;

                geyser_subscribe(&mut client, request, metrics, callback)
                    .await
                    .map_err(backoff::Error::transient)?;

                Ok::<(), backoff::Error<anyhow::Error>>(())
            }
        })
        .await
        .map_err(Into::into)
    }
}

async fn geyser_subscribe<F>(
    client: &mut yellowstone_grpc_client::GeyserGrpcClient<impl yellowstone_grpc_client::Interceptor>,
    request: yellowstone_grpc_proto::geyser::SubscribeRequest,
    metrics: Arc<RwLock<ShyftMetrics>>,
    callback: Arc<F>,
) -> Result<()>
where
    F: Fn(TokenEvent) -> Result<()> + Send + Sync,
{
    let processor = TransactionProcessor::new()?;
    let (mut subscribe_tx, mut stream) = client.subscribe_with_request(Some(request)).await?;
    info!("✅ Shyft gRPC stream opened, listening for new PumpFun tokens...");

    while let Some(message) = stream.next().await {
        match message {
            Ok(msg) => match msg.update_oneof {
                Some(UpdateOneof::Transaction(update)) => {
                    if let Some(txn_info) = update.transaction {
                        // 调试：打印所有接收到的交易
                        let signature = if !txn_info.signature.is_empty() {
                            bs58::encode(&txn_info.signature).into_string()
                        } else {
                            "unknown".to_string()
                        };
                        // 调试：只在DEBUG级别显示接收到的交易
                        debug!("📦 收到交易: {} (Slot: {})", signature, update.slot);
                        
                        // 只在DEBUG级别检查和显示程序信息
                        if log::log_enabled!(log::Level::Debug) {
                            if let Some(transaction) = &txn_info.transaction {
                                if let Some(message) = &transaction.message {
                                    let account_keys: Vec<String> = message.account_keys.iter()
                                        .map(|key| bs58::encode(key).into_string())
                                        .collect();
                                    
                                    let has_pumpfun = account_keys.iter().any(|key| key == PUMPFUN_PROGRAM_ID);
                                    
                                    if has_pumpfun {
                                        debug!("🎯 发现包含PumpFun程序的交易: {}", signature);
                                        debug!("   账户数量: {}, 指令数量: {}", account_keys.len(), message.instructions.len());
                                    }
                                }
                            }
                        }

                        // 更新指标
                        {
                            let mut metrics = metrics.write().await;
                            metrics.transactions_processed += 1;
                            
                            // 每处理500个交易打印一次简化的统计信息
                            if metrics.transactions_processed % 500 == 0 {
                                info!("📊 已处理 {} 笔交易，发现 {} 个新代币", 
                                     metrics.transactions_processed, 
                                     metrics.tokens_detected);
                            }
                        }

                        // 使用新的处理器检测代币创建
                        if let Some(token_event) = processor.process_transaction(&txn_info, update.slot).await {
                            match token_event.transaction_type {
                                TransactionType::TokenCreation => {
                                    info!("🚀 NEW TOKEN | {} | Slot: {}", 
                                         token_event.mint.as_deref().unwrap_or("Unknown"), 
                                         token_event.slot);
                                    info!("   Signature: {}", token_event.signature);
                                    if let Some(sol_amount) = token_event.sol_amount {
                                        info!("   Creation Cost: {:.4} SOL", sol_amount as f64 / 1_000_000_000.0);
                                    }
                                    
                                    // 只在DEBUG模式显示详细信息
                                    if log::log_enabled!(log::Level::Debug) {
                                        debug!("   Detection Method: {}", token_event.detection_method);
                                        debug!("   Accounts: {}", token_event.account_keys.len());
                                        debug!("   完整事件: {:#?}", token_event);
                                    }
                                }
                                TransactionType::Buy => {
                                    info!("💰 BUY | {} | Slot: {}", 
                                         token_event.mint.as_deref().unwrap_or("Unknown"),
                                         token_event.slot);
                                    if let Some(sol_amount) = token_event.sol_amount {
                                        info!("   Max SOL: {:.4} SOL", sol_amount as f64 / 1_000_000_000.0);
                                    }
                                    if let Some(token_amount) = token_event.token_amount {
                                        info!("   Tokens: {}", token_amount);
                                    }
                                    debug!("   Signature: {}", token_event.signature);
                                }
                                TransactionType::Sell => {
                                    info!("💸 SELL | {} | Slot: {}", 
                                         token_event.mint.as_deref().unwrap_or("Unknown"),
                                         token_event.slot);
                                    if let Some(sol_amount) = token_event.sol_amount {
                                        info!("   Min SOL: {:.4} SOL", sol_amount as f64 / 1_000_000_000.0);
                                    }
                                    if let Some(token_amount) = token_event.token_amount {
                                        info!("   Tokens: {}", token_amount);
                                    }
                                    debug!("   Signature: {}", token_event.signature);
                                }
                                _ => {
                                    // 完全忽略其他交易类型
                                    debug!("其他交易类型: {:?}", token_event.transaction_type);
                                }
                            }
                            
                            // 更新指标并调用回调
                            {
                                let mut metrics = metrics.write().await;
                                match token_event.transaction_type {
                                    TransactionType::TokenCreation => {
                                        metrics.tokens_detected += 1;
                                    }
                                    _ => {}
                                }
                            }

                            // 调用回调函数
                            if let Err(e) = callback(token_event) {
                                warn!("Callback error: {}", e);
                            }
                        }
                    }
                }
                Some(UpdateOneof::Ping(_)) => {
                    // 响应ping
                    if let Err(e) = subscribe_tx
                        .send(yellowstone_grpc_proto::geyser::SubscribeRequest {
                            ping: Some(SubscribeRequestPing { id: 1 }),
                            ..Default::default()
                        })
                        .await 
                    {
                        error!("Failed to send pong: {}", e);
                    }
                }
                Some(UpdateOneof::Pong(_)) => {
                    debug!("Received pong from server");
                }
                None => {
                    error!("Update not found in the message");
                    break;
                }
                _ => {
                    debug!("Received other update type");
                }
            },
            Err(error) => {
                error!("Stream error: {:?}", error);
                
                // 更新错误指标
                {
                    let mut metrics = metrics.write().await;
                    metrics.connection_errors += 1;
                }
                
                return Err(anyhow::anyhow!("Stream error: {:?}", error));
            }
        }
    }

    info!("Stream closed");
    Ok(())
}

/// Extract log message for event parsing
pub fn extract_log_message(logs: &[String]) -> Option<String> {
    logs.iter()
        .find_map(|message| {
            if message.starts_with("Program data: ") {
                let encoded = message.trim_start_matches("Program data: ").trim();
                Some(encoded.to_string())
            } else {
                None
            }
        })
}

/// Create a gRPC client connection from config
async fn connect_client(config: &StreamShyftConfig) -> Result<GeyserGrpcClient<impl yellowstone_grpc_client::Interceptor>> {
    GeyserGrpcClient::build_from_shared(config.endpoint.clone())?
        .x_token(Some(config.x_token.clone()))?
        .connect_timeout(Duration::from_secs(config.timeout_seconds))
        .timeout(Duration::from_secs(config.timeout_seconds))
        .tls_config(ClientTlsConfig::new().with_native_roots())?
        .max_decoding_message_size(1024 * 1024 * 1024)  // 1GB max
        .connect()
        .await
        .map_err(Into::into)
}

/// Build subscription request for PumpFun transactions
fn build_subscribe_request(config: &StreamShyftConfig) -> Result<SubscribeRequest> {
    let mut transactions = HashMap::new();
    
    transactions.insert(
        "client".to_owned(),
        SubscribeRequestFilterTransactions {
            vote: Some(false),
            failed: Some(false),
            account_include: vec![PUMPFUN_PROGRAM_ID.to_string()],
            account_exclude: vec![],
            account_required: vec![],
            signature: None,
        },
    );
    
    let commitment = match config.commitment_level.as_str() {
        "processed" => CommitmentLevel::Processed,
        "confirmed" => CommitmentLevel::Confirmed,
        "finalized" => CommitmentLevel::Finalized,
        _ => CommitmentLevel::Processed,
    };

    Ok(SubscribeRequest {
        accounts: HashMap::default(),
        slots: HashMap::default(),
        transactions,
        transactions_status: HashMap::default(),
        blocks: HashMap::default(),
        blocks_meta: HashMap::default(),
        entry: HashMap::default(),
        commitment: Some(commitment as i32),
        accounts_data_slice: Vec::default(),
        ping: None,
        from_slot: None,
    })
}