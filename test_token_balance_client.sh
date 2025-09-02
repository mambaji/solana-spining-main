#!/bin/bash

# TokenBalanceClient 测试脚本
# 用法: ./test_token_balance_client.sh [SHYFT_API_KEY]

echo "🧪 TokenBalanceClient 测试脚本"
echo "================================="

# 检查是否提供了API密钥
if [ -z "$1" ]; then
    echo "❌ 用法: $0 [SHYFT_API_KEY]"
    echo "💡 示例: $0 your_shyft_api_key_here"
    exit 1
fi

# 设置环境变量
export SHYFT_RPC_API_KEY="$1"
export SHYFT_RPC_ENDPOINT="https://rpc.ny.shyft.to"

echo "🔧 环境配置:"
echo "   API密钥: ${SHYFT_RPC_API_KEY:0:8}..."
echo "   RPC端点: $SHYFT_RPC_ENDPOINT"
echo ""

echo "🎯 运行指定账户余额测试..."
echo "================================="
echo "📍 测试账户: 893AbbfPCHShb1SsAnMB6k4nBtroYZbWYNfVVxyX52f6"

# 运行指定账户余额测试
cargo test test_specific_token_account_balance --package solana-spining -- --nocapture

echo ""
echo "🚀 运行其他TokenBalanceClient测试..."
echo "================================="

# 运行特定的TokenBalanceClient测试
cargo test test_get_token_balance_changes_specific_transaction --package solana-spining -- --nocapture

echo ""
echo "🎯 运行买入交易测试..."
cargo test test_get_tokens_acquired_from_buy_transaction --package solana-spining -- --nocapture

echo ""
echo "🛡️ 运行错误处理测试..."
cargo test test_error_handling --package solana-spining -- --nocapture

echo ""
echo "✅ 所有测试完成!"