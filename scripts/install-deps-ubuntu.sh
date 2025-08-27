#!/bin/bash
# Ubuntu依赖安装脚本

set -e

echo "🔧 正在为Ubuntu系统安装构建依赖..."

# 更新包列表
echo "📦 更新包列表..."
sudo apt-get update

# 安装必要的构建工具
echo "🛠️ 安装构建工具..."
sudo apt-get install -y \
    build-essential \
    pkg-config \
    libssl-dev \
    protobuf-compiler \
    git \
    curl

# 验证protoc安装
echo "✅ 验证protoc安装..."
if command -v protoc &> /dev/null; then
    echo "✅ protoc已安装，版本: $(protoc --version)"
else
    echo "❌ protoc安装失败"
    exit 1
fi

# 检查Rust是否已安装
if command -v cargo &> /dev/null; then
    echo "✅ Rust已安装，版本: $(rustc --version)"
else
    echo "📦 安装Rust..."
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
    source ~/.cargo/env
fi

echo "🎉 所有依赖安装完成！"
echo "💡 现在可以运行: cargo build --release"
