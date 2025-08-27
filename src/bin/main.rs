use anyhow::Result;
use clap::Parser;
use log::{info, warn, error};
use std::sync::Arc;

// Import from library - 优化版组件
use solana_spining::{
    TokenEvent, TransactionType,
    ShyftStream, LetsbonkStream,
    EventLogger,
    // 优化后的组件 - 只保留新方案
    OptimizedStrategyManager, OptimizedTokenFilter, OptimizedExecutorManager,
    // 新配置系统
    config::{ConfigManager, AppConfig},
    // 区块哈希缓存
    BlockhashCache,
    // 🆕 计算预算管理
    executor::compute_budget::{DynamicComputeBudgetManager, ComputeBudgetConfig},
};

#[derive(Parser, Debug, Clone)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// 配置文件路径
    #[arg(short, long, default_value = "config.toml")]
    config: String,
    
    /// 流类型选择
    #[arg(value_enum)]
    stream_type: Option<StreamType>,
    
    /// 策略类型覆盖（可选，会覆盖配置文件设置）
    #[arg(long, value_enum)]
    strategy: Option<StrategyType>,
    
    /// 执行策略覆盖（可选，会覆盖配置文件设置）
    #[arg(long, value_enum)]
    execution: Option<ExecutionStrategyType>,
    
    /// 启用交易（覆盖配置文件设置）
    #[arg(long)]
    trading_enabled: bool,
    
    /// 显示配置摘要
    #[arg(long)]
    show_config: bool,
    
    /// 生成默认配置文件
    #[arg(long)]
    generate_config: bool,
    
    /// 验证配置文件
    #[arg(long)]
    validate_config: bool,
}

#[derive(clap::ValueEnum, Debug, Clone)]
enum StreamType {
    PumpFun,
    Letsbonk,
}

#[derive(clap::ValueEnum, Debug, Clone)]
enum StrategyType {
    /// Default sniper strategy - balanced for new token sniping
    Default,
    /// Conservative strategy - stricter filtering for safer investments  
    Conservative,
    /// AI-focused strategy - only targets AI-related tokens
    AiFocused,
    /// Custom aggressive strategy - very permissive for maximum opportunity
    Aggressive,
}

#[derive(clap::ValueEnum, Debug, Clone)]
enum ExecutionStrategyType {
    /// Force use ZeroSlot execution
    ZeroSlot,
    /// Use fallback strategy (try multiple services)
    Fallback,
}

#[tokio::main]
async fn main() -> Result<()> {
    // 加载 .env 文件
    if let Err(_) = dotenvy::dotenv() {
        // .env 文件不存在或读取失败，继续执行（环境变量可能通过其他方式设置）
        eprintln!("Warning: Could not load .env file");
    }
    
    env_logger::init();
    
    let args = Args::parse();

    // 处理特殊命令
    if args.generate_config {
        ConfigManager::generate_default_config_file(&args.config)?;
        info!("默认配置文件已生成: {}", args.config);
        return Ok(());
    }

    if args.validate_config {
        match ConfigManager::load_from_file(&args.config) {
            Ok(config_manager) => {
                info!("配置文件验证通过");
                info!("{}", config_manager.get_config_summary());
            }
            Err(e) => {
                error!("配置文件验证失败: {}", e);
                return Err(e.into());
            }
        }
        return Ok(());
    }

    // 加载配置
    let config_manager = match ConfigManager::load_from_file(&args.config) {
        Ok(manager) => manager,
        Err(e) => {
            error!("配置加载失败: {}", e);
            return Err(e.into());
        }
    };

    if args.show_config {
        println!("{}", config_manager.get_config_summary());
        return Ok(());
    }

    // 检查是否提供了流类型
    let stream_type = match &args.stream_type {
        Some(s) => s.clone(),
        None => {
            error!("必须指定流类型");
            return Err(anyhow::anyhow!("Missing required stream type"));
        }
    };

    let app_config = &config_manager.app_config;

    // 初始化区块哈希缓存
    let blockhash_cache = {
        // 构建带API key的Shyft RPC端点
        let shyft_rpc_endpoint = app_config.get_shyft_rpc_endpoint(None);
        let shyft_rpc_api_key = config_manager.get_shyft_rpc_api_key().unwrap_or_default();
        let rpc_endpoint_with_key = if !shyft_rpc_api_key.is_empty() {
            format!("{}?api_key={}", shyft_rpc_endpoint, shyft_rpc_api_key)
        } else {
            warn!("Shyft RPC API key is empty, blockhash cache may fail");
            shyft_rpc_endpoint.clone()
        };

        let mut cache = BlockhashCache::new(rpc_endpoint_with_key.clone());
        match cache.start() {
            Ok(()) => {
                info!("✅ 区块哈希缓存已启动，RPC端点: {}", 
                      if !shyft_rpc_api_key.is_empty() { 
                          format!("{}?api_key=***", shyft_rpc_endpoint)
                      } else {
                          rpc_endpoint_with_key
                      });
                Some(Arc::new(cache))
            }
            Err(e) => {
                warn!("⚠️ 区块哈希缓存启动失败: {}，将在需要时同步获取", e);
                None
            }
        }
    };

    // 初始化执行器管理器
    let executor_manager = if args.trading_enabled {
        // 检查服务可用性
        let mut available_services = Vec::new();
        if config_manager.is_pumpfun_enabled() {
            available_services.push("Shyft");
        }
        if config_manager.is_zeroshot_enabled() {
            available_services.push("ZeroSlot");
        }

        if available_services.is_empty() {
            warn!("没有可用的交易服务，继续以只读模式运行");
            None
        } else {
            info!("可用服务: {}", available_services.join(", "));
            
            let executor_config = create_executor_config_from_app_config(app_config, &config_manager)?;
            
            match OptimizedExecutorManager::new(executor_config, blockhash_cache.clone()).await {
                Ok(manager) => {
                    info!("交易执行器初始化成功");
                    Some(manager)
                }
                Err(e) => {
                    warn!("交易执行器初始化失败: {}，继续以只读模式运行", e);
                    None
                }
            }
        }
    } else {
        info!("只读模式");
        None
    };

    let result = match stream_type {
        StreamType::PumpFun => run_pumpfun_stream(args, &config_manager, executor_manager, blockhash_cache.as_ref()).await,
        StreamType::Letsbonk => run_letsbonk_stream(args, &config_manager, executor_manager, blockhash_cache.as_ref()).await,
    };

    // 停止区块哈希缓存
    if let Some(cache) = blockhash_cache {
        info!("正在停止区块哈希缓存...");
        // 由于是Arc，我们需要通过Arc::try_unwrap来获取所有权
        if let Ok(mut cache_owned) = Arc::try_unwrap(cache) {
            cache_owned.stop().await;
        } else {
            warn!("区块哈希缓存仍被其他地方引用，无法正常停止");
        }
    }

    result
}

/// 从新配置系统创建执行器配置
fn create_executor_config_from_app_config(
    app_config: &AppConfig, 
    config_manager: &ConfigManager
) -> Result<solana_spining::ExecutorConfig> {
    use solana_spining::executor::{
        ShyftExecutorConfig, 
        ExecutorConfig, 
        ZeroShotConfig, 
        config::{WalletConfig, GeneralConfig}
    };
    use solana_sdk::signer::Signer;

    // 获取钱包密钥
    let wallet_keypair = config_manager.get_wallet_keypair()?;
    let private_key = bs58::encode(wallet_keypair.to_bytes()).into_string();

    Ok(ExecutorConfig {
        shyft: ShyftExecutorConfig {
            rpc_endpoint: app_config.get_shyft_rpc_endpoint(None),
            grpc_endpoint: app_config.get_shyft_grpc_endpoint(None),
            api_key: config_manager.get_shyft_rpc_api_key().unwrap_or_default().to_string(),
            timeout_seconds: app_config.shyft.timeout_seconds,
            enabled: app_config.shyft.enabled,
        },
        zeroshot: ZeroShotConfig {
            base_endpoint: app_config.get_zeroshot_endpoint(None),
            regional_endpoints: {
                let mut map = std::collections::HashMap::new();
                map.insert("ny".to_string(), app_config.zeroshot.regions.ny.clone());
                map.insert("de".to_string(), app_config.zeroshot.regions.de.clone());
                map.insert("ams".to_string(), app_config.zeroshot.regions.ams.clone());
                map.insert("jp".to_string(), app_config.zeroshot.regions.jp.clone());
                map.insert("la".to_string(), app_config.zeroshot.regions.la.clone());
                map
            },
            default_region: app_config.zeroshot.regions.default.clone(),
            api_key: config_manager.get_zeroshot_api_key().unwrap_or_default().to_string(),
            default_tip_lamports: app_config.zeroshot.default_tip_lamports,
            max_tip_lamports: app_config.zeroshot.max_tip_lamports,
            tip_accounts: app_config.zeroshot.tip_accounts.accounts.clone(),
            timeout_seconds: app_config.zeroshot.timeout_seconds,
            enabled: app_config.zeroshot.enabled,
        },
        wallet: WalletConfig {
            private_key,
            pubkey: Some(wallet_keypair.pubkey()),
        },
        general: GeneralConfig {
            default_slippage_bps: app_config.general.default_slippage_bps,
            max_slippage_bps: app_config.general.max_slippage_bps,
            default_max_retries: app_config.general.default_max_retries,
            retry_base_delay_ms: app_config.general.retry_base_delay_ms,
            network_timeout_ms: app_config.general.network_timeout_ms,
            verbose_logging: app_config.general.verbose_logging,
        },
    })
}

/// 创建优化代币过滤器
fn create_optimized_token_filter(strategy: &StrategyType, app_config: &AppConfig) -> Result<OptimizedTokenFilter> {
    use solana_spining::strategy::optimized_token_filter::FilterCriteria;
    use solana_spining::processors::TransactionType;

    let base_criteria = &app_config.strategy.token_filter;

    let criteria = match strategy {
        StrategyType::Default => {
            FilterCriteria {
                min_sol_amount: Some(10_000_000), // 0.01 SOL
                max_sol_amount: None,
                required_name_keywords: vec![],
                forbidden_name_keywords: vec!["test".to_string(), "scam".to_string(), "fake".to_string()],
                min_name_length: Some(3),
                max_name_length: Some(50),
                required_symbol_keywords: vec![],
                forbidden_symbol_keywords: vec!["TEST".to_string(), "SCAM".to_string()],
                min_symbol_length: Some(3),
                max_symbol_length: Some(10),
                max_creation_age_slots: Some(100),
                allowed_transaction_types: vec![TransactionType::TokenCreation],
                whitelist_mints: vec![],
                blacklist_mints: vec![],
                blacklist_programs: vec![],
            }
        }
        StrategyType::Conservative => {
            FilterCriteria {
                min_sol_amount: Some(base_criteria.min_liquidity_sol),
                max_sol_amount: Some(base_criteria.max_market_cap),
                required_name_keywords: vec![],
                forbidden_name_keywords: vec![],
                min_name_length: Some(3),
                max_name_length: Some(30),
                required_symbol_keywords: vec![],
                forbidden_symbol_keywords: vec![],
                min_symbol_length: Some(1),
                max_symbol_length: Some(10),
                max_creation_age_slots: Some(100),
                allowed_transaction_types: vec![TransactionType::TokenCreation],
                whitelist_mints: vec![],
                blacklist_mints: vec![],
                blacklist_programs: vec![],
            }
        }
        StrategyType::AiFocused => {
            FilterCriteria {
                min_sol_amount: Some(1_000_000_000), // 1 SOL
                max_sol_amount: None,
                required_name_keywords: vec!["AI".to_string(), "GPT".to_string(), "Neural".to_string()],
                forbidden_name_keywords: vec!["test".to_string(), "scam".to_string(), "fake".to_string()],
                min_name_length: Some(3),
                max_name_length: Some(50),
                required_symbol_keywords: vec!["AI".to_string(), "BOT".to_string()],
                forbidden_symbol_keywords: vec!["TEST".to_string(), "SCAM".to_string()],
                min_symbol_length: Some(3),
                max_symbol_length: Some(10),
                max_creation_age_slots: Some(100),
                allowed_transaction_types: vec![TransactionType::TokenCreation],
                whitelist_mints: vec![],
                blacklist_mints: vec![],
                blacklist_programs: vec![],
            }
        }
        StrategyType::Aggressive => {
            FilterCriteria {
                min_sol_amount: Some(1_000_000), // 0.001 SOL
                max_sol_amount: None,
                required_name_keywords: vec![],
                forbidden_name_keywords: vec!["scam".to_string()],
                min_name_length: Some(1),
                max_name_length: Some(100),
                required_symbol_keywords: vec![],
                forbidden_symbol_keywords: vec!["SCAM".to_string()],
                min_symbol_length: Some(1),
                max_symbol_length: Some(20),
                max_creation_age_slots: Some(5),
                allowed_transaction_types: vec![TransactionType::TokenCreation],
                whitelist_mints: vec![],
                blacklist_mints: vec![],
                blacklist_programs: vec![],
            }
        }
    };

    Ok(OptimizedTokenFilter::new(criteria))
}

/// 创建计算预算管理器
async fn create_compute_budget_manager(
    app_config: &AppConfig,
    config_manager: &ConfigManager,
) -> Result<Option<Arc<DynamicComputeBudgetManager>>> {
    if let Some(ref cb_config_file) = app_config.compute_budget {
        if cb_config_file.enabled {
            info!("🚀 初始化动态计算预算管理器");
            
            // 从配置文件创建运行时配置
            let runtime_config = match ComputeBudgetConfig::from_config_file(cb_config_file.clone()) {
                Ok(config) => {
                    info!("✅ 计算预算配置加载成功");
                    info!("   - 买入CU: {}", config.pumpfun_buy_cu);
                    info!("   - 卖出CU: {}", config.pumpfun_sell_cu);
                    info!("   - 费用刷新间隔: {}秒", config.fee_refresh_interval);
                    config
                }
                Err(e) => {
                    error!("❌ 计算预算配置解析失败: {}", e);
                    return Err(anyhow::anyhow!("计算预算配置解析失败: {}", e));
                }
            };
            
            // 创建RPC客户端用于费用监控
            let rpc_client = {
                let shyft_rpc_endpoint = app_config.get_shyft_rpc_endpoint(None);
                let shyft_rpc_api_key = config_manager.get_shyft_rpc_api_key().unwrap_or_default();
                
                let rpc_endpoint_with_key = if !shyft_rpc_api_key.is_empty() {
                    format!("{}?api_key={}", shyft_rpc_endpoint, shyft_rpc_api_key)
                } else {
                    warn!("⚠️ Shyft RPC API key为空，费用监控可能失败");
                    shyft_rpc_endpoint.clone()
                };
                
                info!("🔗 使用Shyft RPC进行费用监控: {}", 
                      if !shyft_rpc_api_key.is_empty() { 
                          format!("{}?api_key=***", shyft_rpc_endpoint)
                      } else { 
                          rpc_endpoint_with_key.clone() 
                      });
                
                Some(solana_client::rpc_client::RpcClient::new(rpc_endpoint_with_key))
            };
            
            // 创建管理器实例
            let manager = Arc::new(DynamicComputeBudgetManager::new(runtime_config, rpc_client));
            
            // 如果有RPC客户端，启动费用监控
            if manager.config.fee_refresh_interval > 0 {
                info!("📊 启动优先费用监控任务");
                if let Err(e) = manager.start_fee_monitoring().await {
                    warn!("⚠️ 费用监控启动失败: {}", e);
                    warn!("   将继续运行但使用默认费用配置");
                }
            }
            
            Ok(Some(manager))
        } else {
            info!("📋 计算预算管理已禁用");
            Ok(None)
        }
    } else {
        info!("📋 未配置计算预算管理，使用默认设置");
        Ok(None)
    }
}

async fn run_pumpfun_stream(
    args: Args, 
    config_manager: &ConfigManager,
    executor_manager: Option<Arc<OptimizedExecutorManager>>,
    _blockhash_cache: Option<&Arc<BlockhashCache>>
) -> Result<()> {
    info!("启动 Shyft gRPC 监听");
    
    let app_config = &config_manager.app_config;
    
    // 创建计算预算管理器
    let compute_budget_manager = create_compute_budget_manager(app_config, config_manager).await?;
    
    // 创建优化版策略管理器
    let strategy_config = solana_spining::StrategyConfig {
        buy_amount_lamports: app_config.strategy.trading.buy_amount_lamports,
        max_slippage_bps: app_config.general.default_slippage_bps,
        holding_duration_seconds: 3,
        stop_loss_percentage: Some(app_config.strategy.trading.stop_loss_percent),
        take_profit_percentage: Some(app_config.strategy.trading.take_profit_percent),
        enable_emergency_sell: true,
    };
    
    let strategy_type = args.strategy.unwrap_or(StrategyType::Default);
    let optimized_filter = create_optimized_token_filter(&strategy_type, app_config)?;
    let strategy_manager = OptimizedStrategyManager::new(
        executor_manager.clone(),
        Some(strategy_config.clone()),
        Some(app_config.strategy.trading.max_positions as usize),
        optimized_filter,
        compute_budget_manager, // 🆕 传递计算预算管理器
    );
    
    // 创建事件日志记录器
    let event_logger = Arc::new(EventLogger::new(Some(format!(
        "shyft_events_{}.jsonl", 
        chrono::Utc::now().format("%Y%m%d_%H%M%S")
    ))));
    
    info!("事件记录到: {}", event_logger.get_log_file_path());
    info!("最大并发策略数: {}", app_config.strategy.trading.max_positions);
    
    let config = solana_spining::StreamShyftConfig::new(
        app_config.get_shyft_grpc_endpoint(None), 
        config_manager.get_shyft_api_key()?.to_string()
    ).with_timeout(app_config.shyft.timeout_seconds);

    let stream = ShyftStream::new(config);
    
    info!("已连接 Shyft gRPC");
    if executor_manager.is_some() {
        info!("交易模式: 自动策略管理, 交易金额: {:.4} SOL", strategy_config.buy_amount_lamports as f64 / 1_000_000_000.0);
    } else {
        info!("交易模式: 只读监控");
    }

    // 创建事件处理回调
    let strategy_manager_clone = strategy_manager.clone();
    let event_logger_clone = event_logger.clone();
    
    stream.start_streaming(move |event: TokenEvent| {
        let strategy_manager_for_event = strategy_manager_clone.clone();
        let event_logger_for_event = event_logger_clone.clone();
        
        // 记录关键事件
        match event.transaction_type {
            TransactionType::TokenCreation => {
                info!("[Shyft] 新代币创建: {}", event.mint.as_deref().unwrap_or("Unknown"));
            }
            TransactionType::Buy => {
                if let Some(sol_amount) = event.sol_amount {
                    info!("[Shyft] 买入 {:.4} SOL: {}", sol_amount as f64 / 1_000_000_000.0, event.mint.as_deref().unwrap_or("Unknown"));
                }
            }
            TransactionType::Sell => {
                if let Some(sol_amount) = event.sol_amount {
                    info!("[Shyft] 卖出 {:.4} SOL: {}", sol_amount as f64 / 1_000_000_000.0, event.mint.as_deref().unwrap_or("Unknown"));
                }
            }
            _ => {}
        }
                
        // 所有事件类型都传递给策略管理器处理
        
        tokio::spawn(async move {
            let _start_time = std::time::Instant::now();
            
            // 记录事件到日志文件
            if let Err(e) = event_logger_for_event.handle_event(&event).await {
                error!("处理事件失败: {}", e);
            }

            // 传递给优化版策略管理器处理
            if let Err(e) = strategy_manager_for_event.handle_token_event(&event).await {
                error!("处理代币时出错: {}", e);
            }
            
        });
        
        Ok(())
    }).await?;

    Ok(())
}

async fn run_letsbonk_stream(
    args: Args, 
    config_manager: &ConfigManager,
    executor_manager: Option<Arc<OptimizedExecutorManager>>,
    _blockhash_cache: Option<&Arc<BlockhashCache>>
) -> Result<()> {
    info!("启动 Letsbonk 监听");
    
    let app_config = &config_manager.app_config;
    
    // 🆕 创建计算预算管理器
    let compute_budget_manager = create_compute_budget_manager(app_config, config_manager).await?;
    
    // 创建优化版策略管理器
    let strategy_config = solana_spining::StrategyConfig {
        buy_amount_lamports: app_config.strategy.trading.buy_amount_lamports,
        max_slippage_bps: app_config.general.default_slippage_bps,
        holding_duration_seconds: 60,
        stop_loss_percentage: Some(app_config.strategy.trading.stop_loss_percent),
        take_profit_percentage: Some(app_config.strategy.trading.take_profit_percent),
        enable_emergency_sell: true,
    };
    
    let strategy_type = args.strategy.unwrap_or(StrategyType::Conservative);
    let optimized_filter = create_optimized_token_filter(&strategy_type, app_config)?;
    let strategy_manager = OptimizedStrategyManager::new(
        executor_manager.clone(),
        Some(strategy_config.clone()),
        Some(app_config.strategy.trading.max_positions as usize),
        optimized_filter,
        compute_budget_manager, // 🆕 传递计算预算管理器
    );
    
    // 创建事件日志记录器
    let event_logger = Arc::new(EventLogger::new(Some(format!(
        "letsbonk_events_{}.jsonl", 
        chrono::Utc::now().format("%Y%m%d_%H%M%S")
    ))));
    
    info!("BONK事件记录到: {}", event_logger.get_log_file_path());
    info!("最大并发策略数: {}", app_config.strategy.trading.max_positions);
    
    let config = solana_spining::StreamShyftConfig::new(
        app_config.get_shyft_grpc_endpoint(None), 
        config_manager.get_shyft_api_key()?.to_string()
    ).with_timeout(app_config.shyft.timeout_seconds);

    let stream = LetsbonkStream::new(config);
    
    info!("已连接 Shyft gRPC，监听 BONK 代币");
    if executor_manager.is_some() {
        info!("交易模式: 自动策略管理, 交易金额: {:.4} SOL", strategy_config.buy_amount_lamports as f64 / 1_000_000_000.0);
    } else {
        info!("交易模式: 只读监控");
    }

    // 创建优化版事件处理回调
    let strategy_manager_clone = strategy_manager.clone();
    let event_logger_clone = event_logger.clone();
    
    stream.start_streaming(move |event: TokenEvent| {
        let strategy_manager_for_event = strategy_manager_clone.clone();
        let event_logger_for_event = event_logger_clone.clone();
        
        // 记录关键事件
        match event.transaction_type {
            TransactionType::TokenCreation => {
                let has_buy_info = event.sol_amount.is_some() && event.token_amount.is_some() && 
                    event.detection_method.contains("含买入");
                
                if has_buy_info {
                    info!("[BONK] 新代币+买入: {}", event.mint.as_deref().unwrap_or("Unknown"));
                } else {
                    info!("[BONK] 新代币创建: {}", event.mint.as_deref().unwrap_or("Unknown"));
                }
            }
            TransactionType::Buy => {
                if let Some(sol_amount) = event.sol_amount {
                    info!("[BONK] 买入 {:.4} SOL: {}", sol_amount as f64 / 1_000_000_000.0, event.mint.as_deref().unwrap_or("Unknown"));
                }
            }
            TransactionType::Sell => {
                if let Some(sol_amount) = event.sol_amount {
                    info!("[BONK] 卖出 {:.4} SOL: {}", sol_amount as f64 / 1_000_000_000.0, event.mint.as_deref().unwrap_or("Unknown"));
                }
            }
            _ => {}
        }
        
        // 所有事件类型都传递给策略管理器处理
        tokio::spawn(async move {
            let _start_time = std::time::Instant::now();
            
            // 记录事件到日志文件
            if let Err(e) = event_logger_for_event.handle_event(&event).await {
                error!("处理事件失败: {}", e);
            }
            
            // 传递给策略管理器处理
            if let Err(e) = strategy_manager_for_event.handle_token_event(&event).await {
                error!("策略管理器处理事件失败: {}", e);
            }
            
        });
        
        Ok(())
    }).await?;

    Ok(())
}