# 🎯 Solana代币狙击系统 - 选币策略模块

已成功将选币策略从数据解析层分离，实现了更清晰的架构设计。

## 📁 新的目录结构

```
src/
├── processors/          # 数据解析层 (纯粹的数据处理)
│   ├── token_detector.rs    # PumpFun代币检测
│   ├── letsbonk_detector.rs # LetsBonk代币检测  
│   ├── processor.rs         # 交易处理器核心
│   └── ...
├── strategy/           # 选币策略层 (业务逻辑)
│   ├── token_filter.rs      # 选币策略核心
│   ├── token_filter_examples.rs # 使用示例
│   ├── token_sniper.rs      # 集成狙击器
│   └── mod.rs
└── ...
```

## 🔄 架构分离原理

### ⚡ 数据解析层 (`processors/`)
**职责**：纯粹的数据解析和检测
- 解析PumpFun/LetsBonk交易
- 提取代币创建事件
- 识别买卖交易
- 输出标准化的`TokenEvent`数据结构

### 🧠 策略层 (`strategy/`)
**职责**：业务逻辑和决策
- 接收解析后的`TokenEvent`
- 应用筛选条件和评分算法
- 决定是否执行狙击操作
- 动态调整策略参数

## 🚀 使用方式

### 1. 分离使用（推荐）
```rust
use solana_spining::{
    TransactionProcessor, 
    TokenFilter, TokenSniper
};

// 步骤1: 创建数据解析器
let processor = TransactionProcessor::new()?;

// 步骤2: 创建选币策略
let filter = TokenFilter::default_sniper_strategy();

// 步骤3: 处理交易流
if let Some(token_event) = processor.process_transaction(&txn_info, slot).await {
    let filter_result = filter.evaluate_token(&token_event);
    
    if filter_result.passed {
        // 执行狙击逻辑
        execute_snipe(&token_event).await?;
    }
}
```

### 2. 集成使用
```rust
use solana_spining::TokenSniper;

// 使用集成的TokenSniper，内部自动管理解析器和策略
let mut sniper = TokenSniper::new()?;

if let Some((token_event, filter_result)) = sniper.process_transaction(&txn_info, slot).await {
    if filter_result.passed {
        // 执行狙击逻辑
    }
}
```

## 🎛️ 选币策略配置

### 预设策略
- **默认狙击策略** (`default_sniper_strategy`): 适合新币狙击
- **保守策略** (`conservative_strategy`): 适合稳健投资

### 自定义策略
```rust
let custom_criteria = FilterCriteria {
    min_sol_amount: Some(1_000_000_000), // 1 SOL
    max_sol_amount: Some(10_000_000_000), // 10 SOL
    required_name_keywords: vec!["AI".to_string()],
    forbidden_name_keywords: vec!["scam".to_string(), "test".to_string()],
    max_creation_age_slots: Some(100),
    allowed_transaction_types: vec![TransactionType::TokenCreation],
    // ... 更多条件
};

let filter = TokenFilter::new(custom_criteria);
```

## 🔧 核心组件

### 1. TokenFilter - 选币策略核心
```rust
pub struct FilterCriteria {
    // SOL交易量筛选
    pub min_sol_amount: Option<u64>,
    pub max_sol_amount: Option<u64>,
    
    // 代币名称/符号筛选
    pub required_name_keywords: Vec<String>,
    pub forbidden_name_keywords: Vec<String>,
    
    // 时间筛选
    pub max_creation_age_slots: Option<u64>,
    
    // 黑白名单
    pub whitelist_mints: Vec<String>,
    pub blacklist_mints: Vec<String>,
    
    // ... 更多条件
}
```

### 2. TokenSniper - 集成狙击器
- 内部管理`TransactionProcessor`和`TokenFilter`
- 提供高级API简化使用
- 支持批量处理和动态策略调整

### 3. 评分系统
```rust
pub struct FilterResult {
    pub passed: bool,           // 是否通过筛选
    pub reason: String,         // 通过/失败原因
    pub score: f64,            // 评分 (0.0-1.0)
    pub matched_criteria: Vec<String>,
    pub failed_criteria: Vec<String>,
}
```

## 📊 优势

1. **关注点分离**: 数据解析和业务逻辑完全分离
2. **易于测试**: 每个层次都可独立测试
3. **灵活扩展**: 可以轻松添加新的数据源或策略
4. **策略热更新**: 运行时动态调整筛选条件
5. **详细反馈**: 提供筛选过程的详细信息

## 🎯 典型工作流

```
交易数据 -> TransactionProcessor -> TokenEvent -> TokenFilter -> FilterResult -> 狙击决策
   ↑              ↑                     ↑            ↑              ↑
 原始数据       数据解析              标准事件      策略筛选        业务决策
```

这种架构使得：
- **数据解析层**专注于准确解析各种协议的交易数据
- **策略层**专注于实现各种选币策略和风险控制
- 两者解耦，可以独立开发、测试和优化

现在你的选币策略已经完全独立于数据解析，可以更灵活地调整策略条件，后续也更容易扩展新的功能！