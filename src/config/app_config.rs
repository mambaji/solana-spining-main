use serde::{Serialize, Deserialize};
use crate::executor::errors::ExecutionError;
use crate::executor::compute_budget::ComputeBudgetConfigFromFile;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamShyftConfig {
    pub endpoint: String,
    pub x_token: String,
    pub timeout_seconds: u64,
    pub commitment_level: String,
}

impl StreamShyftConfig {
    pub fn new(endpoint: String, token: String) -> Self {
        Self {
            endpoint,
            x_token: token,
            timeout_seconds: 10,
            commitment_level: "processed".to_string(),
        }
    }

    pub fn with_timeout(mut self, timeout_seconds: u64) -> Self {
        self.timeout_seconds = timeout_seconds;
        self
    }

    pub fn with_commitment(mut self, commitment: String) -> Self {
        self.commitment_level = commitment;
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub general: GeneralConfig,
    pub regions: RegionsConfig,
    pub blockhash_cache: BlockhashCacheConfig,
    pub shyft: ShyftConfig,
    pub zeroshot: ZeroShotConfig,
    pub pumpfun: PumpFunConfig,
    pub raydium: RaydiumConfig,
    pub strategy: StrategyConfig,
    pub monitoring: MonitoringConfig,
    pub logging: LoggingConfig,
    #[serde(default)]
    pub compute_budget: Option<ComputeBudgetConfigFromFile>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneralConfig {
    pub default_slippage_bps: u16,
    pub max_slippage_bps: u16,
    pub default_max_retries: u32,
    pub retry_base_delay_ms: u64,
    pub network_timeout_ms: u64,
    pub verbose_logging: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegionsConfig {
    pub shyft_rpc: String,
    pub shyft_grpc: String,
    pub zeroshot: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockhashCacheConfig {
    pub update_interval_ms: u64,
    pub max_age_seconds: u64,
    pub fallback_timeout_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShyftConfig {
    pub rpc_endpoint: String,
    pub grpc_endpoint: String,
    pub rpc_regions: ShyftRpcRegions,
    pub grpc_regions: ShyftGrpcRegions,
    pub default_priority_fee: u64,
    pub max_priority_fee: u64,
    pub default_commitment: String,
    pub timeout_seconds: u64,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShyftRpcRegions {
    pub ny: String,
    pub va: String,
    pub ams: String,
    pub fra: String,
    pub default: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShyftGrpcRegions {
    pub ny: String,
    pub va: String,
    pub us: String,
    pub eu: String,
    pub ams: String,
    pub fra: String,
    pub default: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZeroShotConfig {
    pub default_tip_lamports: u64,
    pub max_tip_lamports: u64,
    pub timeout_seconds: u64,
    pub enabled: bool,
    pub regions: ZeroShotRegions,
    pub tip_accounts: TipAccounts,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZeroShotRegions {
    pub ny: String,
    pub de: String,
    pub ams: String,
    pub jp: String,
    pub la: String,
    pub default: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TipAccounts {
    pub accounts: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PumpFunConfig {
    pub program_id: String,
    pub global_account: String,
    pub fee_recipient: String,
    pub default_slippage_bps: u16,
    pub min_sol_amount: u64,
    pub max_sol_amount: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RaydiumConfig {
    pub amm_program_id: String,
    pub launchpad_program_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrategyConfig {
    pub token_filter: TokenFilterConfig,
    pub trading: TradingConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenFilterConfig {
    pub min_liquidity_sol: u64,
    pub max_market_cap: u64,
    pub min_volume_24h: u64,
    pub max_holders: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradingConfig {
    pub buy_amount_lamports: u64,
    pub take_profit_percent: f64,
    pub stop_loss_percent: f64,
    pub max_positions: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitoringConfig {
    pub metrics_enabled: bool,
    pub metrics_interval_seconds: u64,
    pub alert_on_errors: bool,
    pub max_error_rate_percent: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingConfig {
    pub level: String,
    pub file_enabled: bool,
    pub console_enabled: bool,
    pub max_file_size_mb: u64,
    pub max_files: u32,
}

impl AppConfig {
    pub fn from_file<P: AsRef<std::path::Path>>(path: P) -> Result<Self, ExecutionError> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| ExecutionError::Configuration(format!("Failed to read config file: {}", e)))?;
        
        toml::from_str(&content)
            .map_err(|e| ExecutionError::Configuration(format!("Failed to parse config file: {}", e)))
    }

    pub fn load_with_env_override<P: AsRef<std::path::Path>>(config_path: P) -> Result<Self, ExecutionError> {
        let mut config = Self::from_file(config_path)?;
        config.apply_env_overrides()?;
        config.validate()?;
        Ok(config)
    }

    fn apply_env_overrides(&mut self) -> Result<(), ExecutionError> {
        // 只处理非敏感配置的环境变量覆盖
        if let Ok(endpoint) = std::env::var("SHYFT_RPC_ENDPOINT") {
            self.shyft.rpc_endpoint = endpoint;
        }
        
        if let Ok(endpoint) = std::env::var("SHYFT_GRPC_ENDPOINT") {
            self.shyft.grpc_endpoint = endpoint;
        }
        
        if let Ok(enabled) = std::env::var("ZEROSHOT_ENABLED") {
            self.zeroshot.enabled = enabled.to_lowercase() == "true";
        }
        
        if let Ok(tip) = std::env::var("ZEROSHOT_DEFAULT_TIP_LAMPORTS") {
            self.zeroshot.default_tip_lamports = tip.parse()
                .map_err(|e| ExecutionError::Configuration(format!("Invalid ZEROSHOT_DEFAULT_TIP_LAMPORTS: {}", e)))?;
        }
        
        if let Ok(level) = std::env::var("LOG_LEVEL") {
            self.logging.level = level;
        }
        
        if let Ok(verbose) = std::env::var("VERBOSE_LOGGING") {
            self.general.verbose_logging = verbose.to_lowercase() == "true";
        }

        Ok(())
    }

    pub fn validate(&self) -> Result<(), ExecutionError> {
        if self.general.default_slippage_bps > self.general.max_slippage_bps {
            return Err(ExecutionError::Configuration(
                "Default slippage cannot be greater than max slippage".to_string()
            ));
        }

        if self.general.max_slippage_bps > 10000 {
            return Err(ExecutionError::Configuration(
                "Max slippage cannot exceed 100%".to_string()
            ));
        }

        if self.general.network_timeout_ms < 1000 {
            return Err(ExecutionError::Configuration(
                "Network timeout must be at least 1000ms".to_string()
            ));
        }

        if self.blockhash_cache.update_interval_ms < 50 {
            return Err(ExecutionError::Configuration(
                "Blockhash cache update interval must be at least 50ms".to_string()
            ));
        }

        if self.blockhash_cache.max_age_seconds > 30 {
            return Err(ExecutionError::Configuration(
                "Blockhash max age should not exceed 30 seconds".to_string()
            ));
        }

        if self.zeroshot.default_tip_lamports > self.zeroshot.max_tip_lamports {
            return Err(ExecutionError::Configuration(
                "ZeroSlot default tip cannot exceed max tip".to_string()
            ));
        }

        if self.pumpfun.min_sol_amount > self.pumpfun.max_sol_amount {
            return Err(ExecutionError::Configuration(
                "PumpFun min SOL amount cannot exceed max SOL amount".to_string()
            ));
        }

        if self.strategy.trading.buy_amount_lamports == 0 {
            return Err(ExecutionError::Configuration(
                "Buy amount must be greater than 0".to_string()
            ));
        }

        if self.zeroshot.tip_accounts.accounts.is_empty() {
            return Err(ExecutionError::Configuration(
                "ZeroSlot tip accounts cannot be empty".to_string()
            ));
        }

        match self.logging.level.to_lowercase().as_str() {
            "trace" | "debug" | "info" | "warn" | "error" => {},
            _ => return Err(ExecutionError::Configuration(
                "Invalid log level. Must be one of: trace, debug, info, warn, error".to_string()
            )),
        }

        log::info!("✅ Configuration validation passed");
        Ok(())
    }

    pub fn get_shyft_rpc_endpoint(&self, region: Option<&str>) -> String {
        let region = region.unwrap_or(&self.regions.shyft_rpc);
        
        match region {
            "ny" => self.shyft.rpc_regions.ny.clone(),
            "va" => self.shyft.rpc_regions.va.clone(),
            "ams" => self.shyft.rpc_regions.ams.clone(),
            "fra" => self.shyft.rpc_regions.fra.clone(),
            _ => {
                log::warn!("Unknown Shyft RPC region '{}', using default", region);
                self.shyft.rpc_endpoint.clone()
            }
        }
    }

    pub fn get_shyft_grpc_endpoint(&self, region: Option<&str>) -> String {
        let region = region.unwrap_or(&self.regions.shyft_grpc);
        
        match region {
            "ny" => self.shyft.grpc_regions.ny.clone(),
            "va" => self.shyft.grpc_regions.va.clone(),
            "us" => self.shyft.grpc_regions.us.clone(),
            "eu" => self.shyft.grpc_regions.eu.clone(),
            "ams" => self.shyft.grpc_regions.ams.clone(),
            "fra" => self.shyft.grpc_regions.fra.clone(),
            _ => {
                log::warn!("Unknown Shyft GRPC region '{}', using default", region);
                self.shyft.grpc_endpoint.clone()
            }
        }
    }

    pub fn get_zeroshot_endpoint(&self, region: Option<&str>) -> String {
        let region = region.unwrap_or(&self.regions.zeroshot);
        
        match region {
            "ny" => self.zeroshot.regions.ny.clone(),
            "de" => self.zeroshot.regions.de.clone(),
            "ams" => self.zeroshot.regions.ams.clone(),
            "jp" => self.zeroshot.regions.jp.clone(),
            "la" => self.zeroshot.regions.la.clone(),
            _ => {
                log::warn!("Unknown ZeroSlot region '{}', using default", region);
                self.zeroshot.regions.ny.clone()
            }
        }
    }

    pub fn save_to_file<P: AsRef<std::path::Path>>(&self, path: P) -> Result<(), ExecutionError> {
        let toml_content = toml::to_string_pretty(self)
            .map_err(|e| ExecutionError::Configuration(format!("Failed to serialize config: {}", e)))?;
        
        std::fs::write(path, toml_content)
            .map_err(|e| ExecutionError::Configuration(format!("Failed to write config file: {}", e)))
    }
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            general: GeneralConfig {
                default_slippage_bps: 300,
                max_slippage_bps: 1000,
                default_max_retries: 3,
                retry_base_delay_ms: 1000,
                network_timeout_ms: 30000,
                verbose_logging: false,
            },
            regions: RegionsConfig {
                shyft_rpc: "ny".to_string(),
                shyft_grpc: "ny".to_string(),
                zeroshot: "ny".to_string(),
            },
            blockhash_cache: BlockhashCacheConfig {
                update_interval_ms: 100,
                max_age_seconds: 10,
                fallback_timeout_ms: 5000,
            },
            shyft: ShyftConfig {
                rpc_endpoint: "https://rpc.shyft.to".to_string(),
                grpc_endpoint: "https://mainnet.solana.shyft.to".to_string(),
                rpc_regions: ShyftRpcRegions {
                    ny: "https://rpc.ny.shyft.to".to_string(),
                    va: "https://rpc.va.shyft.to".to_string(),
                    ams: "https://rpc.ams.shyft.to".to_string(),
                    fra: "https://rpc.fra.shyft.to".to_string(),
                    default: "ny".to_string(),
                },
                grpc_regions: ShyftGrpcRegions {
                    ny: "https://grpc.ny.shyft.to".to_string(),
                    va: "https://grpc.va.shyft.to".to_string(),
                    us: "https://grpc.us.shyft.to".to_string(),
                    eu: "https://grpc.eu.shyft.to".to_string(),
                    ams: "https://grpc.ams.shyft.to".to_string(),
                    fra: "https://grpc.fra.shyft.to".to_string(),
                    default: "ny".to_string(),
                },
                default_priority_fee: 100000,
                max_priority_fee: 10000000,
                default_commitment: "processed".to_string(),
                timeout_seconds: 30,
                enabled: true,
            },
            zeroshot: ZeroShotConfig {
                default_tip_lamports: 1000000,
                max_tip_lamports: 100000000,
                timeout_seconds: 10,
                enabled: true,
                regions: ZeroShotRegions {
                    ny: "http://ny1.0slot.trade".to_string(),
                    de: "http://de.0slot.trade".to_string(),
                    ams: "http://ams.0slot.trade".to_string(),
                    jp: "http://jp.0slot.trade".to_string(),
                    la: "http://la.0slot.trade".to_string(),
                    default: "ny".to_string(),
                },
                tip_accounts: TipAccounts {
                    accounts: vec![
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
                },
            },
            pumpfun: PumpFunConfig {
                program_id: "6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P".to_string(),
                global_account: "4wTV1YmiEkRvAtNtsSGPtUrqRYQMe5SKy2uB4Jjaxnjf".to_string(),
                fee_recipient: "CebN5WGQ4jvEPvsVU4EoHEpgzq1VV9Q6iW8K7kLiXyFK".to_string(),
                default_slippage_bps: 500,
                min_sol_amount: 1000000,
                max_sol_amount: 1000000000000,
            },
            raydium: RaydiumConfig {
                amm_program_id: "675kPX9MHTjS2zt1qfr1NYHuzeLXfQM9H24wFSUt1Mp8".to_string(),
                launchpad_program_id: "F4NEaRvjuT41M3xK9xAzJzDV6QZh3GWb6MFsWiLT9MdT".to_string(),
            },
            strategy: StrategyConfig {
                token_filter: TokenFilterConfig {
                    min_liquidity_sol: 100000000,
                    max_market_cap: 100000000000000,
                    min_volume_24h: 1000000000,
                    max_holders: 10000,
                },
                trading: TradingConfig {
                    buy_amount_lamports: 10000000,
                    take_profit_percent: 200.0,
                    stop_loss_percent: 50.0,
                    max_positions: 1,
                },
            },
            monitoring: MonitoringConfig {
                metrics_enabled: true,
                metrics_interval_seconds: 60,
                alert_on_errors: true,
                max_error_rate_percent: 5.0,
            },
            logging: LoggingConfig {
                level: "info".to_string(),
                file_enabled: true,
                console_enabled: true,
                max_file_size_mb: 100,
                max_files: 10,
            },
            compute_budget: None,
        }
    }
}