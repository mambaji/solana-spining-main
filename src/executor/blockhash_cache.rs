use std::sync::{Arc, RwLock, atomic::{AtomicBool, Ordering}};
use std::time::{Duration, Instant};
use solana_sdk::hash::Hash;
use solana_client::rpc_client::RpcClient;
use tokio::time::sleep;
use log::{info, warn, error, debug};

use crate::executor::errors::ExecutionError;

/// 区块哈希缓存，用于后台获取最新区块哈希
pub struct BlockhashCache {
    /// 缓存的区块哈希和获取时间
    cached_blockhash: Arc<RwLock<Option<(Hash, Instant)>>>,
    /// RPC客户端
    rpc_client: RpcClient,
    /// 运行状态标志
    running: Arc<AtomicBool>,
    /// 后台任务句柄
    task_handle: Option<tokio::task::JoinHandle<()>>,
}

impl BlockhashCache {
    /// 创建新的区块哈希缓存
    /// rpc_endpoint_with_auth: 包含认证信息的完整RPC端点URL
    pub fn new(rpc_endpoint_with_auth: String) -> Self {
        let rpc_client = RpcClient::new(rpc_endpoint_with_auth);
        
        Self {
            cached_blockhash: Arc::new(RwLock::new(None)),
            rpc_client,
            running: Arc::new(AtomicBool::new(false)),
            task_handle: None,
        }
    }

    /// 启动后台更新任务
    pub fn start(&mut self) -> Result<(), ExecutionError> {
        if self.running.load(Ordering::Relaxed) {
            warn!("BlockhashCache already running");
            return Ok(());
        }

        self.running.store(true, Ordering::Relaxed);
        
        let cached_blockhash = Arc::clone(&self.cached_blockhash);
        let rpc_endpoint = self.rpc_client.url();
        let running = Arc::clone(&self.running);

        let handle = tokio::spawn(async move {
            info!("🚀 BlockhashCache background task started");
            
            // 在异步任务中创建RPC客户端
            let rpc_client = RpcClient::new(rpc_endpoint);
            
            while running.load(Ordering::Relaxed) {
                match Self::fetch_latest_blockhash(&rpc_client).await {
                    Ok(blockhash) => {
                        let now = Instant::now();
                        
                        // 更新缓存
                        if let Ok(mut cache) = cached_blockhash.write() {
                            *cache = Some((blockhash, now));
                            debug!("✅ Updated cached blockhash: {}", blockhash);
                        } else {
                            warn!("⚠️ Failed to acquire write lock for blockhash cache");
                        }
                    }   
                    Err(e) => {
                        error!("❌ Failed to fetch latest blockhash: {}", e);
                        // 获取失败时不更新缓存，继续使用旧的（如果有的话）
                    }
                }

                // 等待100ms后再次尝试
                sleep(Duration::from_millis(100)).await;
            }
            
            info!("🛑 BlockhashCache background task stopped");
        });

        self.task_handle = Some(handle);
        Ok(())
    }

    /// 停止后台更新任务
    pub async fn stop(&mut self) {
        self.running.store(false, Ordering::Relaxed);
        
        if let Some(handle) = self.task_handle.take() {
            if let Err(e) = handle.await {
                error!("Failed to stop blockhash cache task: {}", e);
            }
        }
        
        info!("BlockhashCache stopped");
    }

    /// 获取缓存的区块哈希
    pub fn get_cached_blockhash(&self) -> Result<Hash, ExecutionError> {
        let cache = self.cached_blockhash.read()
            .map_err(|_| ExecutionError::Configuration("Failed to read blockhash cache".to_string()))?;

        match cache.as_ref() {
            Some((blockhash, timestamp)) => {
                // 检查缓存是否过期（超过10秒）
                if timestamp.elapsed().as_secs() > 10 {
                    warn!("⚠️ Cached blockhash is stale ({:.1}s old), but using it anyway", 
                          timestamp.elapsed().as_secs_f64());
                }
                Ok(*blockhash)
            }
            None => {
                // 如果缓存为空，尝试同步获取一次
                warn!("📋 Cache empty, attempting sync fetch");
                Err(ExecutionError::ServiceUnavailable {
                    service: "BlockhashCache".to_string(),
                    reason: "No cached blockhash available".to_string(),
                })
            }
        }
    }

    /// 强制同步获取最新区块哈希（备用方案）
    pub async fn get_fresh_blockhash(&self) -> Result<Hash, ExecutionError> {
        Self::fetch_latest_blockhash(&self.rpc_client).await
    }

    /// 获取缓存状态信息
    pub fn get_cache_info(&self) -> Result<CacheInfo, ExecutionError> {
        let cache = self.cached_blockhash.read()
            .map_err(|_| ExecutionError::Configuration("Failed to read blockhash cache".to_string()))?;

        let info = match cache.as_ref() {
            Some((blockhash, timestamp)) => CacheInfo {
                has_cache: true,
                blockhash: Some(*blockhash),
                age_seconds: timestamp.elapsed().as_secs_f64(),
                is_stale: timestamp.elapsed().as_secs() > 10,
            },
            None => CacheInfo {
                has_cache: false,
                blockhash: None,
                age_seconds: 0.0,
                is_stale: true,
            },
        };

        Ok(info)
    }

    /// 内部方法：获取最新区块哈希
    async fn fetch_latest_blockhash(rpc_client: &RpcClient) -> Result<Hash, ExecutionError> {
        rpc_client.get_latest_blockhash()
            .map_err(|e| ExecutionError::Network(format!("Failed to fetch latest blockhash: {}", e)))
    }

    /// 检查是否正在运行
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::Relaxed)
    }
}

impl Drop for BlockhashCache {
    fn drop(&mut self) {
        self.running.store(false, Ordering::Relaxed);
    }
}

/// 缓存状态信息
#[derive(Debug, Clone)]
pub struct CacheInfo {
    pub has_cache: bool,
    pub blockhash: Option<Hash>,
    pub age_seconds: f64,
    pub is_stale: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_cache_creation() {
        let cache = BlockhashCache::new("https://api.mainnet-beta.solana.com".to_string());
        assert!(!cache.is_running());
        
        let info = cache.get_cache_info().unwrap();
        assert!(!info.has_cache);
        assert!(info.is_stale);
    }

    #[tokio::test]
    async fn test_cache_start_stop() {
        let mut cache = BlockhashCache::new("https://api.mainnet-beta.solana.com".to_string());
        
        // 启动
        cache.start().unwrap();
        assert!(cache.is_running());
        
        // 等待一小段时间让后台任务运行
        tokio::time::sleep(Duration::from_millis(200)).await;
        
        // 停止
        cache.stop().await;
        assert!(!cache.is_running());
    }
}