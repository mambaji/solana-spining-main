# 交易执行器测试文档

## 概述

这个测试套件为Solana交易执行器提供全面的测试覆盖，包括PumpFun和Raydium协议的买入和卖出逻辑验证。

## 测试结构

```
src/executor/tests/
├── mod.rs                  # 测试模块声明
├── pumpfun_tests.rs       # PumpFun协议测试
├── raydium_tests.rs       # Raydium协议测试
└── integration_tests.rs   # 集成测试和性能测试
```

## 快速开始

### 1. 运行所有测试
```bash
./run_trading_tests.sh --all
```

### 2. 只测试PumpFun
```bash
./run_trading_tests.sh --pumpfun --verbose
```

### 3. 只测试Raydium
```bash
./run_trading_tests.sh --raydium --verbose
```

### 4. 运行集成测试（包含网络测试）
```bash
# 设置环境变量
export SHYFT_RPC_API_KEY="你的API密钥"

# 运行网络集成测试
./run_trading_tests.sh --integration --with-integration
```

## 测试类型

### PumpFun测试 (`pumpfun_tests.rs`)

- **买入指令构建测试**: 验证PumpFun买入交易的指令构建
- **卖出指令构建测试**: 验证PumpFun卖出交易的指令构建  
- **指令判别器一致性**: 验证相同类型指令使用相同的判别器
- **账户结构测试**: 验证指令包含正确的账户信息
- **滑点保护测试**: 验证滑点计算和保护机制
- **边界值测试**: 测试极端值和边界条件

### Raydium测试 (`raydium_tests.rs`)

- **AMM买入/卖出测试**: 测试Raydium AMM协议交易
- **Launchpad买入/卖出测试**: 测试Raydium Launchpad协议交易
- **协议识别测试**: 验证不同Raydium协议的正确识别
- **参数验证测试**: 验证交易参数的合理性检查
- **滑点计算测试**: 测试Raydium特定的滑点计算

### 集成测试 (`integration_tests.rs`)

- **指令数据格式验证**: 深度验证指令的数据结构
- **协议识别功能**: 测试多协议的自动识别
- **交易参数转换**: 测试参数在不同协议间的转换
- **错误处理测试**: 验证异常情况的处理
- **性能测试**: 测试指令构建的性能指标
- **并发测试**: 验证多线程环境下的安全性

## 测试命令详解

### 基础命令
```bash
# 查看帮助
./run_trading_tests.sh --help

# 运行特定协议测试
./run_trading_tests.sh --pumpfun      # 只测试PumpFun
./run_trading_tests.sh --raydium      # 只测试Raydium

# 运行特定类型测试
./run_trading_tests.sh --integration  # 只运行集成测试
./run_trading_tests.sh --performance  # 只运行性能测试
```

### 高级选项
```bash
# 详细输出（推荐用于调试）
./run_trading_tests.sh --all --verbose

# 包含网络集成测试（需要API密钥）
./run_trading_tests.sh --all --with-integration

# 组合使用
./run_trading_tests.sh --pumpfun --raydium --verbose
```

## 环境变量

### 基础配置
- `RUST_LOG`: 日志级别 (默认: info, 详细模式: debug)
- `CARGO_TEST_TIMEOUT`: 测试超时时间 (默认: 300秒)

### 集成测试配置
- `ENABLE_PUMPFUN_INTEGRATION_TESTS=true`: 启用PumpFun网络测试
- `ENABLE_RAYDIUM_INTEGRATION_TESTS=true`: 启用Raydium网络测试
- `ENABLE_LETSBONK_TESTS=true`: 启用LetsBonk特定测试
- `SHYFT_RPC_API_KEY`: Shyft RPC API密钥（网络测试需要）

## 测试结果解读

### 成功示例
```bash
✅ PumpFun买入指令构建成功
   💰 SOL金额: 100000000 lamports
   🪙 最小代币输出: 1000000 tokens
   👥 账户数量: 10
✅ PumpFun买入指令验证完成
```

### 失败处理
如果测试失败，脚本会：
1. 显示具体的失败信息
2. 生成详细的测试报告
3. 返回非零退出码
4. 提供调试建议

## 常见问题

### Q: 测试失败 "指令判别器错误"
A: 这通常表示PumpFun的指令判别器不正确，需要检查 `transaction_builder.rs` 中的判别器值。

### Q: 网络集成测试被跳过
A: 确保设置了 `SHYFT_RPC_API_KEY` 环境变量并使用 `--with-integration` 参数。

### Q: 编译错误
A: 确保在项目根目录运行，并且已安装最新版本的Rust。

## 调试技巧

### 1. 启用详细日志
```bash
export RUST_LOG=debug
./run_trading_tests.sh --pumpfun --verbose
```

### 2. 运行单个测试
```bash
cargo test test_pumpfun_buy_instruction_building -- --nocapture
```

### 3. 查看生成的测试报告
```bash
cat test_report_*.txt
```

## 下一步

测试通过后，你可以：
1. 检查具体的指令数据格式是否符合PumpFun协议要求
2. 根据测试结果修复发现的问题
3. 在真实环境中进行小额测试
4. 逐步扩展到生产环境

## 注意事项

⚠️ **重要提醒**:
- 测试不会进行真实的链上交易
- 集成测试可能需要网络连接
- 在修改核心逻辑前务必运行完整测试套件
- 生产环境部署前建议进行额外的端到端测试