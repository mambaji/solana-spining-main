#!/bin/bash

# Solana Sniping Bot - Local Run Script
# 设置敏感环境变量并运行程序

set -e  # 遇到错误立即退出

# 颜色定义
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

echo -e "${BLUE}🚀 Solana Sniping Bot - 启动脚本${NC}"
echo "============================================"

# 检查是否存在 .env 文件
if [ ! -f ".env" ]; then
    echo -e "${YELLOW}⚠️  未找到 .env 文件，请创建并配置敏感信息${NC}"
    echo "参考 .env.example 文件进行配置"
    
    if [ -f ".env.example" ]; then
        echo -e "${BLUE}📋 复制 .env.example 到 .env:${NC}"
        cp .env.example .env
        echo "✅ 已创建 .env 文件，请编辑后重新运行"
        exit 1
    else
        echo -e "${RED}❌ 未找到 .env.example 文件${NC}"
        exit 1
    fi
fi

# 加载敏感环境变量
echo -e "${BLUE}📁 加载敏感环境变量...${NC}"
source .env

# 验证必需的敏感环境变量
required_vars=(
    "WALLET_PRIVATE_KEY"
    "SHYFT_API_KEY"
)

missing_vars=()
for var in "${required_vars[@]}"; do
    if [ -z "${!var}" ]; then
        missing_vars+=("$var")
    fi
done

if [ ${#missing_vars[@]} -ne 0 ]; then
    echo -e "${RED}❌ 缺少必需的敏感环境变量:${NC}"
    for var in "${missing_vars[@]}"; do
        echo "  - $var"
    done
    echo "请在 .env 文件中设置这些变量"
    exit 1
fi

# 设置运行时环境变量（非敏感）
export RUST_LOG="${RUST_LOG:-info}"
export RUST_BACKTRACE="${RUST_BACKTRACE:-1}"

# 显示配置信息
echo -e "${GREEN}✅ 敏感环境变量已加载${NC}"
echo "📊 配置信息:"
echo "  - RUST_LOG: $RUST_LOG"
echo "  - 钱包公钥: $(echo $WALLET_PRIVATE_KEY | head -c 10)..."
echo "  - 配置文件: config.toml (非敏感配置)"

# 检查 Rust 和 Cargo
if ! command -v cargo &> /dev/null; then
    echo -e "${RED}❌ 未找到 cargo，请安装 Rust${NC}"
    exit 1
fi

echo -e "${BLUE}🔨 检查依赖和编译...${NC}"

# 编译项目
if ! cargo check --quiet; then
    echo -e "${RED}❌ 编译检查失败${NC}"
    exit 1
fi

echo -e "${GREEN}✅ 编译检查通过${NC}"

# 解析命令行参数
COMMAND=""
ARGS=""
DEBUG_MODE=""

# 检查是否是debug模式
if [ "$1" = "debug" ]; then
    DEBUG_MODE="debug"
    shift
    echo -e "${YELLOW}🔍 Debug模式已启用${NC}"
fi

if [ $# -eq 0 ]; then
    echo -e "${YELLOW}⚠️  未指定命令，显示帮助信息${NC}"
    COMMAND="--help"
else
    COMMAND="$1"
    shift
    ARGS="$@"
fi

# 根据命令显示启动信息
case "$COMMAND" in
    "shyft")
        echo -e "${BLUE}🔍 启动 Shyft 监控模式...${NC}"
        echo "  - 配置通过 config.toml 加载"
        ;;
    "letsbonk")
        echo -e "${BLUE}🎯 启动 LetsBonk 监控模式...${NC}"
        echo "  - 配置通过 config.toml 加载"
        ;;
    "pumpfun")
        echo -e "${BLUE}🚀 启动 PumpFun 监控模式...${NC}"
        echo "  - 配置通过 config.toml 加载"
        ;;
    "--help"|"-h"|"help")
        echo -e "${BLUE}📖 显示帮助信息...${NC}"
        ;;
    *)
        echo -e "${YELLOW}ℹ️  运行自定义命令: $COMMAND${NC}"
        ;;
esac

echo "============================================"
echo -e "${GREEN}🚀 启动程序...${NC}"
echo ""

# 运行程序
if [ "$COMMAND" = "--help" ] || [ "$COMMAND" = "-h" ] || [ "$COMMAND" = "help" ]; then
    cargo run -- --help
elif [ "$DEBUG_MODE" = "debug" ]; then
    # Debug模式：使用debug日志级别并构建debug版本
    echo -e "${YELLOW}🔍 Debug模式运行 (RUST_LOG=debug)${NC}"
    RUST_LOG=debug cargo run -- "$COMMAND" $ARGS
else
    cargo run -- "$COMMAND" $ARGS
fi

# 捕获退出码
EXIT_CODE=$?

echo ""
echo "============================================"
if [ $EXIT_CODE -eq 0 ]; then
    echo -e "${GREEN}✅ 程序正常退出${NC}"
else
    echo -e "${RED}❌ 程序异常退出 (退出码: $EXIT_CODE)${NC}"
fi

exit $EXIT_CODE