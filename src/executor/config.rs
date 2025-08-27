use serde::{Serialize, Deserialize};
use solana_sdk::pubkey::Pubkey;
use crate::executor::errors::ExecutionError;

/// 执行器配置管理
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutorConfig {
    /// Shyft配置  
    pub shyft: ShyftExecutorConfig,
    /// ZeroSlot配置
    pub zeroshot: ZeroShotConfig,
    /// 钱包配置
    pub wallet: WalletConfig,
    /// 通用配置
    pub general: GeneralConfig,
}

/// Jito执行器配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JitoConfig {
    /// Block Engine端点
    pub block_engine_endpoint: String,
    /// 可用区域
    pub available_regions: Vec<String>,
    /// 默认区域
    pub default_region: String,
    /// 默认tip金额 (lamports)
    pub default_tip_lamports: u64,
    /// 最大tip金额 (lamports)  
    pub max_tip_lamports: u64,
    /// Tip接收账户列表
    pub tip_accounts: Vec<String>,
    /// 连接超时 (秒)
    pub timeout_seconds: u64,
    /// 是否启用
    pub enabled: bool,
}

impl Default for JitoConfig {
    fn default() -> Self {
        Self {
            block_engine_endpoint: "https://mainnet.block-engine.jito.wtf".to_string(),
            available_regions: vec![
                "ny".to_string(), 
                "fra".to_string(), 
                "tyo".to_string(), 
                "ams".to_string()
            ],
            default_region: "ny".to_string(),
            default_tip_lamports: 10_000, // 0.00001 SOL
            max_tip_lamports: 1_000_000, // 0.001 SOL
            tip_accounts: vec![
                "96gYZGLnJYVFmbjzopPSU6QiEV5fGqZNyN9nmNhvrZU5".to_string(),
                "HFqU5x63VTqvQss8hp11i4wVV8bD44PvwucfZ2bU7gRe".to_string(),
                "Cw8CFyM9FkoMi7K7Crf6HNQqf4uEMzpKw6QNghXLvLkY".to_string(),
                "ADaUMid9yfUytqMBgopwjb2DTLSokTSzL1zt6iGPaS49".to_string(),
                "DfXygSm4jCyNCybVYYK6DwvWqjKee8pbDmJGcLWNDXjh".to_string(),
                "ADuUkR4vqLUMWXxW9gh6D6L8pMSawimctcNZ5pGwDcEt".to_string(),
                "DttWaMuVvTiduZRnguLF7jNxTgiMBZ1hyAumKUiL2KRL".to_string(),
                "3AVi9Tg9Uo68tJfuvoKvqKNWKkC5wPdSSdeBnizKZ6jT".to_string(),
            ],
            timeout_seconds: 30,
            enabled: true,
        }
    }
}

/// Shyft执行器配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShyftExecutorConfig {
    /// RPC端点
    pub rpc_endpoint: String,
    /// gRPC端点 (用于流监听)
    pub grpc_endpoint: String,
    /// API密钥
    pub api_key: String,
    /// 连接超时 (秒)
    pub timeout_seconds: u64,
    /// 是否启用
    pub enabled: bool,
}

impl Default for ShyftExecutorConfig {
    fn default() -> Self {
        Self {
            rpc_endpoint: "https://rpc.shyft.to".to_string(),
            grpc_endpoint: "https://mainnet.solana.shyft.to".to_string(),
            api_key: "".to_string(), // 需要从环境变量获取
            timeout_seconds: 30,
            enabled: true,
        }
    }
}

/// ZeroSlot执行器配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZeroShotConfig {
    /// 基础端点 (不包含区域)
    pub base_endpoint: String,
    /// 可用区域及其端点
    pub regional_endpoints: std::collections::HashMap<String, String>,
    /// 默认区域
    pub default_region: String,
    /// API密钥
    pub api_key: String,
    /// 默认tip金额 (lamports)
    pub default_tip_lamports: u64,
    /// 最大tip金额 (lamports)
    pub max_tip_lamports: u64,
    /// 小费接收地址列表
    pub tip_accounts: Vec<String>,
    /// 连接超时 (秒)
    pub timeout_seconds: u64,
    /// 是否启用
    pub enabled: bool,
}

impl Default for ZeroShotConfig {
    fn default() -> Self {
        let mut regional_endpoints = std::collections::HashMap::new();
        regional_endpoints.insert("ny".to_string(), "https://ny.0slot.trade".to_string());
        regional_endpoints.insert("de".to_string(), "https://de.0slot.trade".to_string());
        regional_endpoints.insert("ams".to_string(), "https://ams.0slot.trade".to_string());
        regional_endpoints.insert("jp".to_string(), "https://jp.0slot.trade".to_string());
        regional_endpoints.insert("la".to_string(), "https://la.0slot.trade".to_string());

        Self {
            base_endpoint: "https://ny.0slot.trade".to_string(), // 默认使用ny端点
            regional_endpoints,
            default_region: "ny".to_string(),
            api_key: "".to_string(), // 需要从环境变量获取
            default_tip_lamports: 1_000_000, // 0.001 SOL (最低要求)
            max_tip_lamports: 100_000_000, // 0.1 SOL
            tip_accounts: vec![
                "4HiwLEP2Bzqj3hM2ENxJuzhcPCdsafwiet3oGkMkuQY4".to_string(),
                "7toBU3inhmrARGngC7z6SjyP85HgGMmCTEwGNRAcYnEK".to_string(),
                "8mR3wB1nh4D6J9RUCugxUpc6ya8w38LPxZ3ZjcBhgzws".to_string(),
                "6SiVU5WEwqfFapRuYCndomztEwDjvS5xgtEof3PLEGm9".to_string(),
                "TpdxgNJBWZRL8UXF5mrEsyWxDWx9HQexA9P1eTWQ42p".to_string(),
                "D8f3WkQu6dCF33cZxuAsrKHrGsqGP2yvAHf8mX6RXnwf".to_string(),
                "GQPFicsy3P3NXxB5piJohoxACqTvWE9fKpLgdsMduoHE".to_string(),
                "Ey2JEr8hDkgN8qKJGrLf2yFjRhW7rab99HVxwi5rcvJE".to_string(),
                "4iUgjMT8q2hNZnLuhpqZ1QtiV8deFPy2ajvvjEpKKgsS".to_string(),
                "3Rz8uD83QsU8wKvZbgWAPvCNDU6Fy8TSZTMcPm3RB6zt".to_string(),
            ],
            timeout_seconds: 10, // 更短的超时，适合0slot快速确认
            enabled: false, // 默认禁用，需要手动配置
        }
    }
}

/// 钱包配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WalletConfig {
    /// 私钥 (Base58编码) - 注意：生产环境应该使用更安全的密钥管理
    pub private_key: String,
    /// 公钥 (自动从私钥派生)
    #[serde(skip)]
    pub pubkey: Option<Pubkey>,
}

impl Default for WalletConfig {
    fn default() -> Self {
        Self {
            private_key: "".to_string(), // 必须从环境变量设置
            pubkey: None,
        }
    }
}

/// 通用配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneralConfig {
    /// 默认滑点 (基点, 100 = 1%)
    pub default_slippage_bps: u16,
    /// 最大滑点 (基点)
    pub max_slippage_bps: u16,
    /// 默认重试次数
    pub default_max_retries: u32,
    /// 重试延迟基数 (毫秒)
    pub retry_base_delay_ms: u64,
    /// 网络超时 (毫秒)
    pub network_timeout_ms: u64,
    /// 是否启用详细日志
    pub verbose_logging: bool,
}

impl Default for GeneralConfig {
    fn default() -> Self {
        Self {
            default_slippage_bps: 300, // 3%
            max_slippage_bps: 1000, // 10%
            default_max_retries: 3,
            retry_base_delay_ms: 1000,
            network_timeout_ms: 30000,
            verbose_logging: false,
        }
    }
}

impl ExecutorConfig {
    /// 从环境变量创建配置
    pub fn from_env() -> Result<Self, ExecutionError> {
        let mut config = Self::default();

        // Shyft配置
        if let Ok(rpc_endpoint) = std::env::var("SHYFT_RPC_ENDPOINT") {
            config.shyft.rpc_endpoint = rpc_endpoint;
        }
        if let Ok(api_key) = std::env::var("SHYFT_API_KEY") {
            config.shyft.api_key = api_key;
        } else {
            return Err(ExecutionError::Configuration("SHYFT_API_KEY is required".to_string()));
        }

        // ZeroSlot配置
        if let Ok(api_key) = std::env::var("ZEROSHOT_API_KEY") {
            config.zeroshot.api_key = api_key;
            config.zeroshot.enabled = true; // 只有在有API key时才启用
        }
        if let Ok(region) = std::env::var("ZEROSHOT_DEFAULT_REGION") {
            config.zeroshot.default_region = region;
        }

        // 钱包配置
        if let Ok(private_key) = std::env::var("WALLET_PRIVATE_KEY") {
            config.wallet.private_key = private_key;
        } else {
            return Err(ExecutionError::Configuration("WALLET_PRIVATE_KEY is required".to_string()));
        }

        // 通用配置
        if let Ok(slippage) = std::env::var("DEFAULT_SLIPPAGE_BPS") {
            config.general.default_slippage_bps = slippage.parse()
                .map_err(|e| ExecutionError::Configuration(format!("Invalid DEFAULT_SLIPPAGE_BPS: {}", e)))?;
        }
        if let Ok(verbose) = std::env::var("VERBOSE_LOGGING") {
            config.general.verbose_logging = verbose.to_lowercase() == "true";
        }

        Ok(config)
    }

    /// 保存配置到文件
    pub fn save_to_file<P: AsRef<std::path::Path>>(&self, path: P) -> Result<(), ExecutionError> {
        let json = serde_json::to_string_pretty(self)?;
        std::fs::write(path, json)
            .map_err(|e| ExecutionError::Configuration(format!("Failed to save config: {}", e)))
    }

    /// 从文件加载配置
    pub fn load_from_file<P: AsRef<std::path::Path>>(path: P) -> Result<Self, ExecutionError> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| ExecutionError::Configuration(format!("Failed to read config file: {}", e)))?;
        serde_json::from_str(&content)
            .map_err(|e| ExecutionError::Configuration(format!("Failed to parse config: {}", e)))
    }

    /// 验证配置
    pub fn validate(&self) -> Result<(), ExecutionError> {
        // 验证钱包私钥
        if self.wallet.private_key.is_empty() {
            return Err(ExecutionError::Configuration("Wallet private key is required".to_string()));
        }

        // 尝试解析私钥以验证格式
        bs58::decode(&self.wallet.private_key)
            .into_vec()
            .map_err(|_| ExecutionError::Configuration("Invalid wallet private key format".to_string()))?;

        // 验证Shyft API key
        if self.shyft.enabled && self.shyft.api_key.is_empty() {
            return Err(ExecutionError::Configuration("Shyft API key is required when Shyft is enabled".to_string()));
        }

        // 验证ZeroSlot API key
        if self.zeroshot.enabled && self.zeroshot.api_key.is_empty() {
            return Err(ExecutionError::Configuration("ZeroSlot API key is required when ZeroSlot is enabled".to_string()));
        }

        // 验证滑点设置
        if self.general.default_slippage_bps > self.general.max_slippage_bps {
            return Err(ExecutionError::Configuration("Default slippage cannot be greater than max slippage".to_string()));
        }

        Ok(())
    }
}

impl Default for ExecutorConfig {
    fn default() -> Self {
        Self {
            shyft: ShyftExecutorConfig::default(),
            zeroshot: ZeroShotConfig::default(),
            wallet: WalletConfig::default(),
            general: GeneralConfig::default(),
        }
    }
}