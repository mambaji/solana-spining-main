# 配置系统使用指南

## 概览

项目现在支持通过 **配置文件** + **环境变量** 的方式管理所有参数，替代了之前的硬编码方式。

### 🏗️ **配置架构**
- **config.toml** - 主配置文件（非敏感信息）
- **.env** - 环境变量文件（敏感信息）
- **ConfigManager** - 统一配置管理

## 快速开始

### 1. 复制配置文件
```bash
# 复制示例配置文件
cp config.toml your_config.toml
cp .env.example .env
```

### 2. 设置环境变量（必需）
编辑 `.env` 文件：
```bash
# 必需配置
WALLET_PRIVATE_KEY="your_base58_private_key"
SHYFT_API_KEY="your_shyft_api_key"

# 可选配置
ZEROSHOT_API_KEY="your_zeroshot_api_key"
JITO_ENABLED="true"
```

### 3. 调整配置文件（可选）
编辑 `your_config.toml` 文件调整交易参数、费用设置等。

## 配置文件详解

### 📋 **config.toml 结构**

```toml
[general]
default_slippage_bps = 300      # 默认滑点 3%
max_slippage_bps = 1000         # 最大滑点 10%
network_timeout_ms = 30000      # 网络超时

[blockhash_cache]
update_interval_ms = 100        # 区块哈希缓存更新间隔
max_age_seconds = 10           # 最大有效期

[shyft]
rpc_endpoint = "https://rpc.shyft.to"
default_priority_fee = 100000   # 默认优先费用

[jito]
default_tip_lamports = 10000    # 默认tip金额
timeout_seconds = 30

[zeroshot]
default_tip_lamports = 1000000  # 0slot最低tip要求
enabled = false                 # 默认禁用

[pumpfun]
program_id = "6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P"
default_slippage_bps = 500      # PumpFun默认滑点

[strategy]
[strategy.trading]
position_size_percent = 10      # 仓位大小
take_profit_percent = 200       # 止盈点
stop_loss_percent = 50          # 止损点
```

### 🔐 **环境变量说明**

#### 必需变量
- `WALLET_PRIVATE_KEY` - 钱包私钥（Base58或JSON格式）
- `SHYFT_API_KEY` - Shyft API密钥

#### 可选变量
- `ZEROSHOT_API_KEY` - ZeroSlot API密钥
- `JITO_ENABLED` - 是否启用Jito
- `LOG_LEVEL` - 日志级别
- 其他覆盖配置...

## 代码使用示例

### 加载配置
```rust
use solana_spining::config::ConfigManager;

// 从配置文件和环境变量加载
let config_manager = ConfigManager::load_from_file("config.toml")?;

// 获取应用配置
let app_config = &config_manager.app_config;

// 获取敏感信息
let wallet = config_manager.get_wallet_keypair()?;
let shyft_key = config_manager.get_shyft_api_key()?;
```

### 服务状态检查
```rust
// 检查服务是否启用
if config_manager.is_shyft_enabled() {
    println!("Shyft服务已启用");
}

if config_manager.is_zeroshot_enabled() {
    println!("ZeroSlot服务已启用");
}

// 获取配置摘要
println!("{}", config_manager.get_config_summary());
```

### 获取端点
```rust
// 获取区域化端点
let jito_endpoint = app_config.get_jito_endpoint(Some("ny"));
let zeroshot_endpoint = app_config.get_zeroshot_endpoint(Some("de"));
```

## 安全最佳实践

### 🔒 **敏感信息管理**
1. **绝对不要**将 `.env` 文件提交到代码仓库
2. 使用专门的交易钱包，不要使用主钱包
3. 定期更换API密钥
4. 在生产环境中使用环境变量而不是文件

### 📁 **文件权限**
```bash
# 设置正确的文件权限
chmod 600 .env                 # 只有所有者可读写
chmod 644 config.toml         # 所有者可读写，其他人只读
```

## 配置验证

配置系统包含完整的验证机制：

### 自动验证
- 滑点范围检查
- 超时时间合理性
- Tip金额限制
- 必需环境变量检查

### 手动验证
```bash
# 生成默认配置文件
cargo run --bin generate-config

# 验证配置文件
cargo run --bin validate-config config.toml
```

## 环境变量优先级

环境变量会覆盖配置文件中的对应设置：

1. **环境变量** （最高优先级）
2. **配置文件**
3. **默认值** （最低优先级）

## 故障排除

### 常见错误

1. **钱包密钥格式错误**
   ```
   Error: Invalid wallet private key format
   ```
   解决：确保使用正确的Base58格式或JSON数组格式

2. **API密钥缺失**
   ```
   Error: SHYFT_API_KEY environment variable is required
   ```
   解决：在 `.env` 文件中设置对应的API密钥

3. **配置文件解析失败**
   ```
   Error: Failed to parse config file
   ```
   解决：检查TOML文件语法，确保引号和缩进正确

### 调试技巧

```bash
# 启用详细日志
export VERBOSE_LOGGING="true"
export LOG_LEVEL="debug"

# 检查配置摘要
cargo run -- --show-config
```

## 迁移指南

如果你有旧版本的硬编码配置，按以下步骤迁移：

1. 创建 `config.toml` 文件
2. 将硬编码的端点、费用等移到配置文件
3. 将敏感信息（密钥等）移到 `.env` 文件
4. 使用 `ConfigManager` 替代直接的配置访问

这样可以实现更安全、更灵活的配置管理！