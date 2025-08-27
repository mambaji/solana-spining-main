# 🚀 Solana代币狙击系统 - 交易执行模块使用指南

## 📋 概述

现在系统已支持实际的交易执行功能！通过集成多种RPC服务，可以自动执行代币买入操作。

## 🔧 环境配置

在使用交易执行功能前，需要配置以下环境变量：

### 必需配置
```bash
# 钱包私钥 (Base58编码)
export WALLET_PRIVATE_KEY="your_private_key_here"

# Shyft API密钥
export SHYFT_API_KEY="your_shyft_api_key"
```

### 可选配置
```bash
# Jito配置
export JITO_ENABLED="true"
export JITO_DEFAULT_TIP_LAMPORTS="50000"  # 0.00005 SOL

# ZeroSlot配置 (需要联系获取API密钥)
export ZEROSHOT_ENABLED="true"
export ZEROSHOT_API_KEY="your_zeroshot_api_key"
export ZEROSHOT_DEFAULT_REGION="ny"

# 通用配置
export DEFAULT_SLIPPAGE_BPS="300"  # 3%
export VERBOSE_LOGGING="true"
```

## 🚀 使用方式

### 1. 只读模式 (默认)
```bash
# 只监听，不执行交易
cargo run -- shyft --token "your-shyft-token"
```

### 2. 启用自动交易
```bash
# 启用交易执行，使用智能策略
cargo run -- shyft \
  --token "your-shyft-token" \
  --trading-enabled \
  --trade-amount 100000000 \
  --max-slippage-bps 300 \
  --execution smart

# 使用特定执行策略
cargo run -- shyft \
  --token "your-shyft-token" \
  --trading-enabled \
  --execution jito

# 使用回退策略
cargo run -- shyft \
  --token "your-shyft-token" \
  --trading-enabled \
  --execution fallback
```

## 🎛️ 执行策略

### Smart (智能策略) - 推荐
根据交易金额自动选择最佳执行器：
- 大额交易 (≥10 SOL): 使用Jito确保快速执行
- 中额交易 (≥1 SOL): 使用ZeroSlot快速确认
- 小额交易 (<1 SOL): 使用Shyft节省成本

### Jito
```bash
--execution jito
```
- 使用Bundle提交，优先级最高
- 适合：重要交易，需要快速执行
- 费用：tip + 基础费用

### Shyft
```bash
--execution shyft  
```
- 使用优先费用
- 适合：常规交易
- 费用：优先费用 + 基础费用

### ZeroSlot
```bash
--execution zeroshot
```
- 声称0slot确认
- 适合：追求极致速度
- 费用：tip + 基础费用

### Fallback (回退策略)
```bash
--execution fallback
```
- 按优先级尝试多个服务
- 提高执行成功率
- 自动重试机制

## 📊 命令行参数

| 参数 | 默认值 | 说明 |
|-----|--------|------|
| `--trading-enabled` | false | 启用实际交易执行 |
| `--trade-amount` | 100000000 | 交易金额 (lamports, 0.1 SOL) |
| `--max-slippage-bps` | 300 | 最大滑点 (基点, 3%) |
| `--execution` | smart | 执行策略类型 |
| `--strategy` | default | 选币策略 |

## 💡 使用示例

### 保守交易 - 小额测试
```bash
cargo run -- shyft \
  --token "your-token" \
  --trading-enabled \
  --strategy conservative \
  --trade-amount 50000000 \
  --execution shyft
```

### 激进交易 - 大额狙击
```bash  
cargo run -- shyft \
  --token "your-token" \
  --trading-enabled \
  --strategy aggressive \
  --trade-amount 5000000000 \
  --execution smart
```

### BONK专项狙击
```bash
cargo run -- letsbonk \
  --token "your-token" \
  --trading-enabled \
  --strategy default \
  --trade-amount 1000000000 \
  --execution jito
```

## 🔍 系统输出

### 健康检查示例
```
🔧 初始化交易执行器...
✅ 交易执行器初始化成功
   ✅ Shyft: 健康  
   ❌ Jito: 不健康
   💰 Shyft 建议费用: 100000 lamports
```

### 交易执行示例
```
🎯 ✅ 代币通过选币策略筛选!
   🪙 代币地址: 7xKXtg2CW87d97TXJSDpbD5jBkheTqA83TZRuJosgAsU
   📊 评分: 0.85/1.0
🚀 符合狙击条件 - 准备执行买入交易!
📋 交易参数:
   💰 SOL数量: 0.1000
   📊 最大滑点: 3%
   🎛️ 执行策略: Smart
🎉 ✅ 交易执行成功!
   📝 签名: 4v1n9xKv9LhjKuGgXzqjhTzVq8VNW6z4m...
   💸 实际费用: 105000 lamports
   ⏱️ 执行延迟: 2340ms
   ✅ 确认状态: confirmed
```

## ⚠️ 安全提醒

1. **测试环境**: 建议先在devnet测试
2. **私钥安全**: 使用环境变量，不要硬编码私钥
3. **资金管理**: 
   - 不要在钱包中存放超过需要的SOL
   - 设置合理的交易金额
   - 监控交易结果
4. **网络状况**: 关注Solana网络拥堵情况调整费用
5. **策略调整**: 根据市场情况调整选币和执行策略

## 🚨 故障排除

### 钱包余额不足
```
❌ 交易执行失败: Insufficient balance: required 105000000, available 50000000
   💡 建议: 向钱包充值或降低交易金额
```

### 服务不可用
```
❌ 交易执行失败: Service unavailable: Jito - HTTP 500
   💡 建议: 尝试其他执行策略或稍后重试
```

### 配置错误
```
❌ 无法从环境变量加载执行器配置: WALLET_PRIVATE_KEY is required
   请确保设置了必需的环境变量:
   - WALLET_PRIVATE_KEY
   - SHYFT_API_KEY
```

## 🔄 升级说明

从只读版本升级到交易版本：

1. **备份**: 确保备份当前配置
2. **环境变量**: 按上述说明设置环境变量  
3. **测试**: 先使用小额进行测试
4. **监控**: 密切关注交易结果和费用

## 📈 性能优化建议

1. **网络选择**: 根据地理位置选择最近的区域
2. **费用设置**: 根据网络拥堵情况动态调整
3. **策略组合**: 结合使用不同的选币和执行策略
4. **监控指标**: 关注执行延迟和成功率

现在你的Solana代币狙击系统具备了完整的交易执行能力！🎉

请谨慎使用，注意资金安全。