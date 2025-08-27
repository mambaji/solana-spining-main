# 🎯 代币狙击系统 - 使用指南

已成功集成选币策略到主程序中！现在在检测到新代币时会自动应用选币策略进行筛选。

## 🚀 使用方式

### 基本命令格式
```bash
cargo run -- <stream_type> [选项]
```

### 📊 数据流类型

#### 1. Shyft流 (推荐用于生产)
```bash
# 使用默认狙击策略
cargo run -- shyft --token "your-shyft-token"

# 使用保守策略
cargo run -- shyft --strategy conservative --token "your-shyft-token"

# 使用AI专注策略
cargo run -- shyft --strategy ai-focused --token "your-shyft-token"

# 使用激进策略
cargo run -- shyft --strategy aggressive --token "your-shyft-token"
```

#### 2. LetsBonk流 (专门监控BONK代币)
```bash
# 监控BONK代币创建，使用默认策略
cargo run -- letsbonk --token "your-shyft-token"

# 监控BONK代币，使用保守策略
cargo run -- letsbonk --strategy conservative --token "your-shyft-token"
```

#### 3. Jito流 (高速监控)
```bash
# 使用默认策略监控Jito ShredStream
cargo run -- jito

# 使用激进策略
cargo run -- jito --strategy aggressive
```

## 🎛️ 选币策略详解

### 🎯 Default (默认狙击策略)
- **适用场景**: 平衡的新币狙击
- **特点**: 
  - SOL交易量: 0.1-10 SOL
  - 过滤明显垃圾币关键词
  - 只关注100个slot内的新币
  - 适合大多数场景

### 🛡️ Conservative (保守策略)
- **适用场景**: 稳健投资，降低风险
- **特点**:
  - SOL交易量: 1-100 SOL (更高门槛)
  - 更严格的关键词筛选
  - 允许代币创建和买入交易
  - 适合风险厌恶投资者

### 🤖 AI-Focused (AI专注策略)
- **适用场景**: 专门狙击AI相关代币
- **特点**:
  - 必须包含AI相关关键词: "AI", "GPT", "BOT", "Neural", "Intelligence"
  - 最小1 SOL交易量
  - 适合AI概念炒作期

### ⚡ Aggressive (激进策略)
- **适用场景**: 最大化机会，高风险高收益
- **特点**:
  - 极低门槛: 0.01 SOL最小交易量
  - 只关注5个slot内的超新币
  - 移除大部分限制条件
  - 适合经验丰富的交易者

## 📝 输出示例

### ✅ 通过筛选的代币
```
🎯 ✅ 代币通过选币策略筛选!
   🪙 代币地址: 7xKXtg2CW87d97TXJSDpbD5jBkheTqA83TZRuJosgAsU
   📝 签名: 4v1n9xKv9LhjKuGgXzqjhTzVq8VNW6z4m...
   📊 评分: 0.85/1.0
   ✅ 匹配条件: ["交易类型匹配", "mint不在黑名单中", "SOL金额在范围内"]
   💰 SOL数量: 500000000 lamports
   🎉 **符合狙击条件 - 可以执行买入逻辑!**
===
```

### ❌ 未通过筛选的代币
```
❌ 代币未通过选币策略筛选
   🪙 代币地址: 9yBhXt8dG1Ks9N3Vq6PzLw2F4mQz1...
   📊 评分: 0.35/1.0
   ❌ 原因: 名称包含禁止关键词: scam
   ⚠️ 未通过条件: ["名称包含禁止关键词: scam", "SOL金额过低: 50000000 < 100000000"]
---
```

## 🔧 高级配置

### 环境变量
```bash
# 设置日志级别
export RUST_LOG=info

# Shyft配置
export SHYFT_GRPC_ENDPOINT="https://mainnet.solana.shyft.to"
export SHYFT_GRPC_TOKEN="your-token-here"
```

### 自定义端点
```bash
# 使用自定义Shyft端点
cargo run -- shyft --endpoint "https://your-custom-endpoint.com" --token "your-token"
```

## 📊 策略性能对比

| 策略类型 | 通过率 | 风险等级 | 适用场景 |
|---------|--------|----------|----------|
| Default | ~30% | 中等 | 日常狙击 |
| Conservative | ~10% | 低 | 稳健投资 |
| AI-Focused | ~5% | 中等 | 主题投资 |
| Aggressive | ~60% | 高 | 激进交易 |

## 🎯 实际使用建议

### 1. 新手建议
```bash
# 从保守策略开始
cargo run -- shyft --strategy conservative --token "your-token"
```

### 2. 经验用户
```bash
# 使用默认策略
cargo run -- shyft --token "your-token"
```

### 3. 高频交易者
```bash
# 使用激进策略 + Jito高速流
cargo run -- jito --strategy aggressive
```

### 4. 主题投资
```bash
# AI热潮期间使用AI策略
cargo run -- shyft --strategy ai-focused --token "your-token"
```

## 🔄 动态调整

程序运行时会根据市场情况自动调整部分参数，但你也可以：

1. **重启程序**切换策略
2. **查看日志**了解筛选详情
3. **分析通过率**优化策略选择

## ⚠️ 重要提醒

1. **测试环境**: 建议先在测试网络测试
2. **资金管理**: 合理分配资金，不要全仓
3. **风险控制**: 设置止损和止盈点
4. **监控日志**: 关注筛选原因，调整策略
5. **及时更新**: 定期更新代码获取最新功能

## 🚀 下一步

在 `process_token_with_strategy` 函数的 TODO 部分添加你的实际狙击逻辑：

```rust
// TODO: 在这里添加实际的狙击逻辑
// execute_snipe_logic(&event).await?;
```

例如：
- 调用Solana RPC执行买入交易
- 发送Discord/Telegram通知
- 记录到数据库
- 设置自动止盈止损

现在你的系统已经具备了完整的选币能力！🎉