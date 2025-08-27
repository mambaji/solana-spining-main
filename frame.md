src/
├── data_sources/                   # 数据源层
│   ├── streams/                    # 流式数据源（按池子区分）
│   │   ├── pumpfun_stream.rs       # PumpFun池流（现有 streams/shyft/stream.rs）
│   │   ├── bonk_stream.rs          # Bonk池流（现有 streams/letsbonk/stream.rs）
│   │   └── stream_trait.rs         # 流接口抽象
│   └── source_manager.rs           # 数据源统一管理
│
├── data_processing/                # 数据处理层（按池子区分）
│   ├── parsers/                    # 池子特定解析器
│   │   ├── pumpfun_parser.rs       # PumpFun池解析（现有 processors/token_detector.rs）
│   │   ├── bonk_parser.rs          # Bonk池解析（现有 processors/letsbonk_detector.rs）
│   │   └── parser_trait.rs         # 解析器接口抽象
│   ├── processor.rs                # 统一处理器（现有 processors/processor.rs）
│   └── processing_coordinator.rs   # 处理流程协调
│
├── strategy/                       # 策略层
│   ├── filters/                    # 过滤器
│   │   ├── token_filter.rs         # 代币过滤器（现有 strategy/optimized_token_filter.rs）
│   │   └── filter_trait.rs         # 过滤器接口
│   ├── trading_strategy.rs         # 交易策略（现有 strategy/optimized_trading_strategy.rs）
│   ├── strategy_manager.rs         # 策略管理器（现有 strategy/optimized_strategy_manager.rs）
│   └── strategy_coordinator.rs     # 策略协调器
│
├── execution/                      # 交易执行层
│   ├── interfaces/                 # 交易接口
│   │   ├── zeroshot_executor.rs    # ZeroShot执行器（现有 executor/zeroshot_executor.rs）
│   │   └── executor_trait.rs       # 执行器接口（现有 executor/traits.rs）
│   ├── builders/                   # 池子特定交易构建器
│   │   ├── pumpfun_builder.rs      # PumpFun池交易构建
│   │   ├── bonk_builder.rs         # Bonk池交易构建
│   │   └── builder_trait.rs        # 构建器接口抽象
│   ├── executor_manager.rs         # 执行器管理（现有 executor/optimized_executor_manager.rs）
│   └── execution_coordinator.rs    # 执行协调器
│
├── rpc/                           # 🆕 链上API模块
│   ├── clients/                   # RPC客户端
│   │   ├── solana_rpc_client.rs   # 标准Solana RPC客户端
│   │   ├── shyft_rpc_client.rs    # Shyft RPC API客户端
│   │   └── client_trait.rs        # 客户端接口抽象
│   ├── cache/                     # 缓存组件
│   │   ├── blockhash_cache.rs     # 区块哈希缓存（现有 executor/blockhash_cache.rs）
│   │   └── account_cache.rs       # 账户信息缓存
│   └── balance_tracker.rs         # 余额查询（现有 utils/token_balance_client.rs）
│
├── compute_budget/                # 🆕 计算预算管理
│   ├── dynamic_manager.rs         # 动态CU管理（现有 executor/compute_budget.rs）
│   ├── fee_monitor.rs             # 费用监控
│   └── budget_optimizer.rs       # 预算优化策略
│
├── network/                       # 🆕 网络通信层
│   ├── grpc/                      # gRPC相关
│   │   ├── shyft_grpc.rs          # Shyft gRPC客户端
│   │   └── connection_manager.rs  # 连接管理
│   ├── websocket/                 # WebSocket（如需要）
│   │   └── ws_client.rs           # WebSocket客户端
│   └── http_client.rs             # HTTP客户端封装
│
├── monitoring/                    # 🆕 监控模块
│   ├── metrics.rs                 # 性能指标收集
│   ├── health_check.rs            # 健康检查
│   └── performance_tracker.rs     # 性能追踪
│
├── shared/                         # 共享组件
│   ├── config/                     # 配置管理（现有）
│   ├── utils/                      # 工具函数（现有）
│   ├── models/                     # 🔄 数据模型
│   │   ├── token_event.rs          # TokenEvent等（现有lib.rs中的类型）
│   │   ├── trade_types.rs          # 交易相关类型
│   │   └── common_types.rs         # 通用类型
│   ├── errors/                     # 🔄 错误处理拆分
│   │   ├── execution_errors.rs     # 执行相关错误（现有 executor/errors.rs）
│   │   ├── network_errors.rs       # 网络相关错误
│   │   └── strategy_errors.rs      # 策略相关错误
│   └── constants.rs                # 🆕 常量定义
│
└── orchestrator/                   # 总协调器
    ├── trading_pipeline.rs         # 业务流程协调（现有 main.rs 逻辑）
    ├── service_coordinator.rs      # 🆕 服务协调
    └── resource_manager.rs         # 🆕 资源管理

## 📋 重构说明

### 🆕 新增模块
- **rpc/**: 链上API调用的统一封装，包含所有RPC客户端和缓存
- **compute_budget/**: 计算预算和费用管理的专门模块
- **network/**: 网络通信层抽象，统一gRPC、HTTP等协议
- **monitoring/**: 监控和性能追踪模块

### 🔄 优化调整
- **shared/models/**: 将数据模型按功能分类，便于维护
- **shared/errors/**: 按领域拆分错误类型，更精确的错误处理
- **orchestrator/**: 增加服务协调和资源管理，更好的系统控制

### 📂 模块迁移映射
```
现有文件                              →  新架构位置
──────────────────────────────────────────────────────────────
streams/shyft/stream.rs              →  data_sources/streams/pumpfun_stream.rs
streams/letsbonk/stream.rs           →  data_sources/streams/bonk_stream.rs
processors/token_detector.rs        →  data_processing/parsers/pumpfun_parser.rs
processors/letsbonk_detector.rs     →  data_processing/parsers/bonk_parser.rs
processors/processor.rs             →  data_processing/processor.rs
strategy/optimized_token_filter.rs  →  strategy/filters/token_filter.rs
strategy/optimized_trading_strategy.rs → strategy/trading_strategy.rs
strategy/optimized_strategy_manager.rs → strategy/strategy_manager.rs
executor/zeroshot_executor.rs       →  execution/interfaces/zeroshot_executor.rs
executor/traits.rs                  →  execution/interfaces/executor_trait.rs
executor/optimized_executor_manager.rs → execution/executor_manager.rs
executor/blockhash_cache.rs         →  rpc/cache/blockhash_cache.rs
executor/compute_budget.rs          →  compute_budget/dynamic_manager.rs
executor/errors.rs                  →  shared/errors/execution_errors.rs
utils/token_balance_client.rs       →  rpc/balance_tracker.rs
bin/main.rs                         →  orchestrator/trading_pipeline.rs
```

### 🎯 架构优势
1. **清晰分层**: 数据源→处理→策略→执行的清晰流水线
2. **模块解耦**: 各模块职责单一，依赖关系清晰
3. **易于扩展**: 新增池子或功能只需实现对应trait
4. **便于测试**: 每层都可独立测试
5. **性能监控**: 专门的监控模块便于优化性能

### 🔧 下一步重构建议
1. 先创建新的目录结构
2. 逐步迁移现有代码到新架构
3. 实现缺失的trait抽象
4. 添加监控和错误处理
5. 完善测试覆盖率