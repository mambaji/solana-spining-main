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
use crate::processors::{TokenEvent, TransactionType, LetsbonkDetector};

// Raydium Launchpad Program ID
const RAYDIUM_LAUNCHPAD_PROGRAM_ID: &str = "LanMV9sAd7wArD4vJFi2qDdfnVhFxYSUg6eADduJ3uj";

#[derive(Debug, Clone)]
pub struct LetsbonkMetrics {
    pub transactions_processed: u64,
    pub bonk_tokens_detected: u64,
    pub total_tokens_detected: u64,
    pub connection_errors: u64,
    pub reconnection_attempts: u64,
    pub last_connection_time: Option<Instant>,
}

#[derive(Debug, Clone)]
pub struct LetsbonkConnectionStatus {
    pub is_connected: bool,
    pub reconnection_attempts: u64,
    pub last_connection_time: Option<Instant>,
    pub connection_errors: u64,
}

impl Default for LetsbonkMetrics {
    fn default() -> Self {
        Self {
            transactions_processed: 0,
            bonk_tokens_detected: 0,
            total_tokens_detected: 0,
            connection_errors: 0,
            reconnection_attempts: 0,
            last_connection_time: None,
        }
    }
}

pub struct LetsbonkStream {
    config: StreamShyftConfig,
    metrics: Arc<RwLock<LetsbonkMetrics>>,
    zero_attempts: Arc<Mutex<bool>>,
}

impl LetsbonkStream {
    pub fn new(config: StreamShyftConfig) -> Self {
        Self { 
            config,
            metrics: Arc::new(RwLock::new(LetsbonkMetrics::default())),
            zero_attempts: Arc::new(Mutex::new(true)),
        }
    }
    
    pub async fn get_metrics(&self) -> LetsbonkMetrics {
        self.metrics.read().await.clone()
    }

    /// Get connection status information
    pub async fn get_connection_status(&self) -> LetsbonkConnectionStatus {
        let metrics = self.metrics.read().await;
        
        LetsbonkConnectionStatus {
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
        info!("🚀 Starting Letsbonk gRPC stream for Raydium Launchpad BONK tokens...");
        
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
                    info!("Retry to connect to the server for Letsbonk monitoring");
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
                info!("✅ Connected to Shyft gRPC for Letsbonk monitoring");

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
    metrics: Arc<RwLock<LetsbonkMetrics>>,
    callback: Arc<F>,
) -> Result<()>
where
    F: Fn(TokenEvent) -> Result<()> + Send + Sync,
{
    let processor = LetsbonkDetector::new()?;
    let (mut subscribe_tx, mut stream) = client.subscribe_with_request(Some(request)).await?;
    info!("✅ Letsbonk gRPC stream opened, listening for Raydium Launchpad BONK tokens...");

    while let Some(message) = stream.next().await {
        match message {
            Ok(msg) => match msg.update_oneof {
                Some(UpdateOneof::Transaction(update)) => {
                    if let Some(txn_info) = update.transaction {
                        // Update metrics
                        {
                            let mut metrics = metrics.write().await;
                            metrics.transactions_processed += 1;
                        }

                        // Use letsbonk detector to detect BONK token creation
                        if let Some(token_event) = processor.detect_bonk_token_creation(&txn_info, update.slot).await {
                            // Update metrics and call callback
                            {
                                let mut metrics = metrics.write().await;
                                metrics.total_tokens_detected += 1;
                                match token_event.transaction_type {
                                    TransactionType::TokenCreation => {
                                        metrics.bonk_tokens_detected += 1;
                                    }
                                    _ => {}
                                }
                            }

                            // Call callback function
                            if let Err(e) = callback(token_event) {
                                warn!("Callback error: {}", e);
                            }
                        }
                    }
                }
                Some(UpdateOneof::Ping(_)) => {
                    // Respond to ping
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
                
                // Update error metrics
                {
                    let mut metrics = metrics.write().await;
                    metrics.connection_errors += 1;
                }
                
                return Err(anyhow::anyhow!("Stream error: {:?}", error));
            }
        }
    }

    info!("Letsbonk stream closed");
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

/// Build subscription request for Raydium Launchpad transactions
fn build_subscribe_request(config: &StreamShyftConfig) -> Result<SubscribeRequest> {
    let mut transactions = HashMap::new();
    
    transactions.insert(
        "client".to_owned(),
        SubscribeRequestFilterTransactions {
            vote: Some(false),
            failed: Some(false),
            account_include: vec![RAYDIUM_LAUNCHPAD_PROGRAM_ID.to_string()],
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