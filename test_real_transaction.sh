#!/bin/bash

# 真实交易查询接口测试脚本
# 
# 这个脚本帮助您轻松测试买入交易查询接口
# 需要提供真实的 API Key、交易签名、代币地址和钱包地址

set -e

echo "🚀 Solana 代币交易查询接口真实测试"
echo "======================================="

# 检查参数
if [ $# -lt 4 ]; then
    echo ""
    echo "❌ 参数不足！"
    echo ""
    echo "用法: $0 <SHYFT_API_KEY> <TRANSACTION_SIGNATURE> <TOKEN_MINT> <BUYER_WALLET> [RPC_ENDPOINT]"
    echo ""
    echo "参数说明:"
    echo "  SHYFT_API_KEY        - Shyft RPC API 密钥"
    echo "  TRANSACTION_SIGNATURE - 买入交易的签名"
    echo "  TOKEN_MINT           - 代币的 mint 地址"
    echo "  BUYER_WALLET         - 买方钱包地址"
    echo "  RPC_ENDPOINT         - (可选) RPC 端点，默认使用 ny 区域"
    echo ""
    echo "示例:"
    echo "  $0 \"shyft_abcd1234\" \"5K7xY...abc123\" \"EPjF...xyz789\" \"9WzD...def456\""
    echo ""
    exit 1
fi

# 获取参数
SHYFT_API_KEY="$1"
TRANSACTION_SIGNATURE="$2"
TOKEN_MINT="$3"
BUYER_WALLET="$4"
RPC_ENDPOINT="${5:-https://rpc.ny.shyft.to}"

# 验证参数格式
echo "🔍 验证参数格式..."

if [ ${#SHYFT_API_KEY} -lt 8 ]; then
    echo "❌ API 密钥格式无效：长度过短"
    exit 1
fi

if [ ${#TRANSACTION_SIGNATURE} -lt 64 ]; then
    echo "❌ 交易签名格式无效：长度过短"
    exit 1
fi

if [ ${#TOKEN_MINT} -lt 32 ]; then
    echo "❌ 代币地址格式无效：长度过短"
    exit 1
fi

if [ ${#BUYER_WALLET} -lt 32 ]; then
    echo "❌ 钱包地址格式无效：长度过短"
    exit 1
fi

echo "✅ 参数格式验证通过"

# 显示测试信息
echo ""
echo "🔧 测试配置:"
echo "   📡 API密钥: ${SHYFT_API_KEY:0:8}...${SHYFT_API_KEY: -8}"
echo "   🌐 RPC端点: $RPC_ENDPOINT"
echo "   📝 交易签名: $TRANSACTION_SIGNATURE"
echo "   🪙 代币地址: $TOKEN_MINT"
echo "   👤 买方地址: $BUYER_WALLET"

# 设置环境变量
export SHYFT_RPC_API_KEY="$SHYFT_API_KEY"
export SHYFT_RPC_ENDPOINT="$RPC_ENDPOINT"
export TEST_TRANSACTION_SIGNATURE="$TRANSACTION_SIGNATURE"
export TEST_TOKEN_MINT="$TOKEN_MINT"
export TEST_BUYER_WALLET="$BUYER_WALLET"

echo ""
echo "🏃 运行测试..."
echo ""

# 运行买入交易查询测试
echo "📊 测试 1: 买入交易查询"
echo "----------------------------------------"
cargo test test_real_buy_transaction_query -- --nocapture --ignored

echo ""
echo "📊 测试 2: 当前余额查询"
echo "----------------------------------------"
cargo test test_real_token_balance_query -- --nocapture --ignored

echo ""
echo "✅ 所有测试完成！"