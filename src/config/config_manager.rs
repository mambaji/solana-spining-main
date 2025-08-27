use std::path::Path;
use crate::{
    config::AppConfig,
    executor::errors::ExecutionError,
};
use solana_sdk::{signature::Keypair, signer::Signer};

/// 配置管理器 - 统一配置加载入口
/// 
/// 职责分工：
/// - AppConfig: 处理TOML文件、数据结构、验证、非敏感环境变量
/// - ConfigManager: 处理敏感信息(API keys, 私钥)、完整配置组装、安全验证
/// 
/// 使用方式：
/// ```rust
/// let config_manager = ConfigManager::load_from_file("config.toml")?;
/// let app_config = &config_manager.app_config;  // 访问应用配置
/// let api_key = config_manager.get_shyft_api_key()?;  // 访问敏感信息
/// ```
pub struct ConfigManager {
    pub app_config: AppConfig,
    pub shyft_api_key: Option<String>,
    pub shyft_rpc_api_key: Option<String>,
    pub zeroshot_api_key: Option<String>,
    pub wallet_keypair: Option<Keypair>,
}

impl ConfigManager {
    /// 从配置文件和环境变量加载完整配置
    pub fn load_from_file<P: AsRef<Path>>(config_path: P) -> Result<Self, ExecutionError> {
        // 1. 加载主配置文件（处理非敏感环境变量覆盖）
        let app_config = AppConfig::load_with_env_override(config_path)?;
        
        // 2. 加载敏感信息
        let manager = Self {
            app_config,
            shyft_api_key: std::env::var("SHYFT_API_KEY").ok(),
            shyft_rpc_api_key: std::env::var("SHYFT_RPC_API_KEY").ok(),
            zeroshot_api_key: std::env::var("ZEROSHOT_API_KEY").ok(),
            wallet_keypair: Self::load_wallet_keypair()?,
        };

        // 3. 验证必需的配置
        manager.validate_required_config()?;
        
        log::info!("✅ Configuration loaded successfully");
        Ok(manager)
    }

    /// 加载钱包密钥
    fn load_wallet_keypair() -> Result<Option<Keypair>, ExecutionError> {
        if let Ok(private_key_str) = std::env::var("WALLET_PRIVATE_KEY") {
            Ok(Some(Self::parse_wallet_keypair(&private_key_str)?))
        } else {
            Ok(None)
        }
    }

    /// 解析钱包密钥
    fn parse_wallet_keypair(private_key_str: &str) -> Result<Keypair, ExecutionError> {
        // 支持多种格式的私钥
        if private_key_str.starts_with('[') && private_key_str.ends_with(']') {
            // JSON数组格式: [1,2,3,...]
            let bytes: Vec<u8> = serde_json::from_str(private_key_str)
                .map_err(|e| ExecutionError::Configuration(format!("Invalid wallet key JSON format: {}", e)))?;
            
            if bytes.len() != 64 {
                return Err(ExecutionError::Configuration("Wallet private key must be 64 bytes".to_string()));
            }
            
            Keypair::from_bytes(&bytes)
                .map_err(|e| ExecutionError::Configuration(format!("Invalid wallet keypair: {}", e)))
        } else {
            // Base58格式
            let decoded = bs58::decode(private_key_str)
                .into_vec()
                .map_err(|e| ExecutionError::Configuration(format!("Invalid base58 wallet key: {}", e)))?;
            
            if decoded.len() != 64 {
                return Err(ExecutionError::Configuration("Wallet private key must be 64 bytes".to_string()));
            }
            
            Keypair::from_bytes(&decoded)
                .map_err(|e| ExecutionError::Configuration(format!("Invalid wallet keypair: {}", e)))
        }
    }

    /// 验证必需的配置
    fn validate_required_config(&self) -> Result<(), ExecutionError> {
        // 检查钱包密钥
        if self.wallet_keypair.is_none() {
            return Err(ExecutionError::Configuration(
                "WALLET_PRIVATE_KEY environment variable is required".to_string()
            ));
        }

        // 检查启用的服务是否有对应的API key
        if self.app_config.shyft.enabled && self.shyft_api_key.is_none() {
            return Err(ExecutionError::Configuration(
                "SHYFT_API_KEY environment variable is required when Shyft is enabled".to_string()
            ));
        }

        if self.app_config.zeroshot.enabled && self.zeroshot_api_key.is_none() {
            return Err(ExecutionError::Configuration(
                "ZEROSHOT_API_KEY environment variable is required when ZeroSlot is enabled".to_string()
            ));
        }

        log::info!("✅ Required configuration validation passed");
        Ok(())
    }

    /// 获取Shyft API key (for gRPC)
    pub fn get_shyft_api_key(&self) -> Result<&str, ExecutionError> {
        self.shyft_api_key.as_ref()
            .map(|s| s.as_str())
            .ok_or_else(|| ExecutionError::Configuration("Shyft API key not available".to_string()))
    }

    /// 获取Shyft RPC API key (for RPC calls)
    pub fn get_shyft_rpc_api_key(&self) -> Result<&str, ExecutionError> {
        // 优先使用专门的RPC API key，如果没有则回退到通用的API key
        self.shyft_rpc_api_key.as_ref()
            .or(self.shyft_api_key.as_ref())
            .map(|s| s.as_str())
            .ok_or_else(|| ExecutionError::Configuration("Shyft RPC API key not available".to_string()))
    }

    /// 获取ZeroSlot API key
    pub fn get_zeroshot_api_key(&self) -> Result<&str, ExecutionError> {
        self.zeroshot_api_key.as_ref()
            .map(|s| s.as_str())
            .ok_or_else(|| ExecutionError::Configuration("ZeroSlot API key not available".to_string()))
    }

    /// 获取钱包密钥
    pub fn get_wallet_keypair(&self) -> Result<&Keypair, ExecutionError> {
        self.wallet_keypair.as_ref()
            .ok_or_else(|| ExecutionError::Configuration("Wallet keypair not available".to_string()))
    }

    /// 克隆钱包密钥（用于多线程场景）
    pub fn clone_wallet_keypair(&self) -> Result<Keypair, ExecutionError> {
        let keypair = self.get_wallet_keypair()?;
        Ok(Keypair::from_bytes(&keypair.to_bytes())
            .map_err(|e| ExecutionError::Configuration(format!("Failed to clone wallet keypair: {}", e)))?)
    }

    /// 获取钱包公钥
    pub fn get_wallet_pubkey(&self) -> Result<solana_sdk::pubkey::Pubkey, ExecutionError> {
        Ok(self.get_wallet_keypair()?.pubkey())
    }

    /// 检查服务是否启用
    pub fn is_pumpfun_enabled(&self) -> bool {
        self.app_config.shyft.enabled && self.shyft_api_key.is_some()
    }

    pub fn is_zeroshot_enabled(&self) -> bool {
        self.app_config.zeroshot.enabled && self.zeroshot_api_key.is_some()
    }

    /// 生成默认配置文件
    pub fn generate_default_config_file<P: AsRef<Path>>(path: P) -> Result<(), ExecutionError> {
        let default_config = AppConfig::default();
        default_config.save_to_file(path)?;
        log::info!("✅ Default configuration file generated");
        Ok(())
    }

    /// 获取配置摘要（不包含敏感信息）
    pub fn get_config_summary(&self) -> String {
        format!(
            "ConfigManager Summary:\n\
            - Shyft gRPC: {} (API key: {})\n\
            - Shyft RPC: {} (API key: {})\n\
            - ZeroSlot: {} (API key: {})\n\
            - Wallet: {}\n\
            - Log level: {}\n\
            - Blockhash cache interval: {}ms",
            if self.app_config.shyft.enabled { "enabled" } else { "disabled" },
            if self.shyft_api_key.is_some() { "set" } else { "missing" },
            if self.app_config.shyft.enabled { "enabled" } else { "disabled" },
            if self.shyft_rpc_api_key.is_some() || self.shyft_api_key.is_some() { "set" } else { "missing" },
            if self.app_config.zeroshot.enabled { "enabled" } else { "disabled" },
            if self.zeroshot_api_key.is_some() { "set" } else { "missing" },
            if self.wallet_keypair.is_some() { "loaded" } else { "missing" },
            self.app_config.logging.level,
            self.app_config.blockhash_cache.update_interval_ms
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_wallet_keypair_base58() {
        // 测试Base58格式
        let keypair = Keypair::new();
        let base58_key = bs58::encode(keypair.to_bytes()).into_string();
        
        let parsed = ConfigManager::parse_wallet_keypair(&base58_key).unwrap();
        assert_eq!(keypair.to_bytes(), parsed.to_bytes());
    }

    #[test]
    fn test_parse_wallet_keypair_json() {
        // 测试JSON数组格式
        let keypair = Keypair::new();
        let bytes = keypair.to_bytes();
        let json_key = serde_json::to_string(&bytes.to_vec()).unwrap();
        
        let parsed = ConfigManager::parse_wallet_keypair(&json_key).unwrap();
        assert_eq!(keypair.to_bytes(), parsed.to_bytes());
    }

    #[test]
    fn test_config_validation() {
        // 测试配置验证
        let app_config = AppConfig::default();
        assert!(app_config.validate().is_ok());
    }
}