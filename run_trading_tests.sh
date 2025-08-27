#!/bin/bash

# =============================================================================
# 交易执行器测试套件运行脚本
# =============================================================================
# 
# 这个脚本用于运行所有交易相关的测试，包括：
# - PumpFun协议测试
# - Raydium协议测试
# - 集成测试和性能测试
# - 指令验证测试
#
# 使用方法：
#   ./run_trading_tests.sh [选项]
#
# 选项：
#   --all                   运行所有测试
#   --pumpfun              只运行PumpFun测试
#   --raydium              只运行Raydium测试
#   --integration          只运行集成测试
#   --with-integration     包含需要网络的集成测试
#   --performance          只运行性能测试
#   --verbose              详细输出
#   --help                 显示帮助信息
# =============================================================================

set -e

# 颜色定义
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
PURPLE='\033[0;35m'
CYAN='\033[0;36m'
NC='\033[0m' # No Color

# 默认参数
RUN_ALL=false
RUN_PUMPFUN=false
RUN_RAYDIUM=false
RUN_INTEGRATION=false
RUN_PERFORMANCE=false
WITH_INTEGRATION=false
VERBOSE=false

# 解析命令行参数
while [[ $# -gt 0 ]]; do
    case $1 in
        --all)
            RUN_ALL=true
            shift
            ;;
        --pumpfun)
            RUN_PUMPFUN=true
            shift
            ;;
        --raydium)
            RUN_RAYDIUM=true
            shift
            ;;
        --integration)
            RUN_INTEGRATION=true
            shift
            ;;
        --with-integration)
            WITH_INTEGRATION=true
            shift
            ;;
        --performance)
            RUN_PERFORMANCE=true
            shift
            ;;
        --verbose)
            VERBOSE=true
            shift
            ;;
        --help)
            echo "交易执行器测试套件"
            echo ""
            echo "使用方法: $0 [选项]"
            echo ""
            echo "选项:"
            echo "  --all                   运行所有测试"
            echo "  --pumpfun              只运行PumpFun测试"
            echo "  --raydium              只运行Raydium测试"
            echo "  --integration          只运行集成测试"
            echo "  --with-integration     包含需要网络的集成测试"
            echo "  --performance          只运行性能测试"
            echo "  --verbose              详细输出"
            echo "  --help                 显示此帮助信息"
            echo ""
            echo "示例:"
            echo "  $0 --all                # 运行所有测试"
            echo "  $0 --pumpfun --verbose  # 详细运行PumpFun测试"
            echo "  $0 --integration --with-integration  # 运行包含网络的集成测试"
            exit 0
            ;;
        *)
            echo -e "${RED}❌ 未知参数: $1${NC}"
            echo "使用 --help 查看可用选项"
            exit 1
            ;;
    esac
done

# 如果没有指定特定测试，默认运行所有基础测试
if [[ "$RUN_ALL" == false && "$RUN_PUMPFUN" == false && "$RUN_RAYDIUM" == false && "$RUN_INTEGRATION" == false && "$RUN_PERFORMANCE" == false ]]; then
    RUN_ALL=true
fi

# 函数定义
print_header() {
    echo -e "${BLUE}=============================================${NC}"
    echo -e "${BLUE}$1${NC}"
    echo -e "${BLUE}=============================================${NC}"
}

print_section() {
    echo -e "${CYAN}📊 $1${NC}"
    echo -e "${CYAN}---------------------------------------------${NC}"
}

print_success() {
    echo -e "${GREEN}✅ $1${NC}"
}

print_warning() {
    echo -e "${YELLOW}⚠️ $1${NC}"
}

print_error() {
    echo -e "${RED}❌ $1${NC}"
}

print_info() {
    echo -e "${PURPLE}ℹ️ $1${NC}"
}

# 检查环境
check_environment() {
    print_header "检查测试环境"
    
    # 检查Rust和Cargo
    if ! command -v cargo &> /dev/null; then
        print_error "Cargo未找到，请安装Rust工具链"
        exit 1
    fi
    
    print_success "Cargo版本: $(cargo --version)"
    
    # 检查当前目录
    if [[ ! -f "Cargo.toml" ]]; then
        print_error "未在Rust项目根目录中运行"
        exit 1
    fi
    
    print_success "在正确的项目目录中"
    
    # 检查是否在solana-spining项目中
    if grep -q "solana-spining" Cargo.toml 2>/dev/null; then
        print_success "确认在solana-spining项目中"
    else
        print_warning "可能不在solana-spining项目中"
    fi
}

# 设置测试环境变量
setup_test_environment() {
    print_section "设置测试环境"
    
    # 设置日志级别
    export RUST_LOG=info
    if [[ "$VERBOSE" == true ]]; then
        export RUST_LOG=debug
        print_info "启用详细日志输出"
    fi
    
    # 设置测试超时
    export CARGO_TEST_TIMEOUT=300
    
    # 如果启用集成测试，设置相关环境变量
    if [[ "$WITH_INTEGRATION" == true ]]; then
        print_info "启用网络集成测试"
        export ENABLE_PUMPFUN_INTEGRATION_TESTS=true
        export ENABLE_RAYDIUM_INTEGRATION_TESTS=true
        export ENABLE_LETSBONK_TESTS=true
        
        # 检查是否有测试用的API密钥
        if [[ -n "$SHYFT_RPC_API_KEY" ]]; then
            print_success "检测到Shyft API密钥"
        else
            print_warning "未设置SHYFT_RPC_API_KEY，部分集成测试可能跳过"
        fi
    fi
    
    print_success "测试环境设置完成"
}

# 运行PumpFun测试
run_pumpfun_tests() {
    print_section "运行PumpFun协议测试"
    
    local test_args="--test pumpfun_tests"
    if [[ "$VERBOSE" == true ]]; then
        test_args="$test_args -- --nocapture"
    fi
    
    echo "🚀 开始PumpFun测试..."
    if cargo test $test_args; then
        print_success "PumpFun测试通过"
        return 0
    else
        print_error "PumpFun测试失败"
        return 1
    fi
}

# 运行Raydium测试
run_raydium_tests() {
    print_section "运行Raydium协议测试"
    
    local test_args="--test raydium_tests"
    if [[ "$VERBOSE" == true ]]; then
        test_args="$test_args -- --nocapture"
    fi
    
    echo "🚀 开始Raydium测试..."
    if cargo test $test_args; then
        print_success "Raydium测试通过"
        return 0
    else
        print_error "Raydium测试失败"
        return 1
    fi
}

# 运行集成测试
run_integration_tests() {
    print_section "运行集成测试"
    
    local test_args="--test integration_tests"
    if [[ "$VERBOSE" == true ]]; then
        test_args="$test_args -- --nocapture"
    fi
    
    # 如果启用网络集成测试，包含被忽略的测试
    if [[ "$WITH_INTEGRATION" == true ]]; then
        test_args="$test_args --ignored"
        echo "🌐 包含网络集成测试..."
    fi
    
    echo "🚀 开始集成测试..."
    if cargo test $test_args; then
        print_success "集成测试通过"
        return 0
    else
        print_error "集成测试失败"
        return 1
    fi
}

# 运行性能测试
run_performance_tests() {
    print_section "运行性能测试"
    
    local test_args="performance_tests"
    if [[ "$VERBOSE" == true ]]; then
        test_args="$test_args -- --nocapture"
    fi
    
    echo "⚡ 开始性能测试..."
    if cargo test $test_args; then
        print_success "性能测试通过"
        return 0
    else
        print_error "性能测试失败"
        return 1
    fi
}

# 运行基础单元测试
run_basic_tests() {
    print_section "运行基础单元测试"
    
    local test_args="executor::transaction_builder::tests"
    if [[ "$VERBOSE" == true ]]; then
        test_args="$test_args -- --nocapture"
    fi
    
    echo "🧪 开始基础单元测试..."
    if cargo test $test_args; then
        print_success "基础单元测试通过"
        return 0
    else
        print_error "基础单元测试失败"
        return 1
    fi
}

# 生成测试报告
generate_test_report() {
    print_section "生成测试报告"
    
    local timestamp=$(date '+%Y%m%d_%H%M%S')
    local report_file="test_report_${timestamp}.txt"
    
    echo "📋 生成测试报告: $report_file"
    
    {
        echo "=================================="
        echo "交易执行器测试报告"
        echo "生成时间: $(date)"
        echo "=================================="
        echo ""
        echo "测试配置:"
        echo "- 详细输出: $VERBOSE"
        echo "- 包含集成测试: $WITH_INTEGRATION"
        echo "- Rust版本: $(rustc --version)"
        echo "- Cargo版本: $(cargo --version)"
        echo ""
        echo "环境变量:"
        echo "- RUST_LOG: $RUST_LOG"
        echo "- CARGO_TEST_TIMEOUT: $CARGO_TEST_TIMEOUT"
        if [[ "$WITH_INTEGRATION" == true ]]; then
            echo "- ENABLE_PUMPFUN_INTEGRATION_TESTS: $ENABLE_PUMPFUN_INTEGRATION_TESTS"
            echo "- ENABLE_RAYDIUM_INTEGRATION_TESTS: $ENABLE_RAYDIUM_INTEGRATION_TESTS"
            echo "- ENABLE_LETSBONK_TESTS: $ENABLE_LETSBONK_TESTS"
        fi
        echo ""
    } > "$report_file"
    
    print_success "测试报告已生成: $report_file"
}

# 主执行流程
main() {
    print_header "Solana交易执行器测试套件"
    
    # 检查环境
    check_environment
    
    # 设置环境
    setup_test_environment
    
    # 记录开始时间
    local start_time=$(date +%s)
    local failed_tests=()
    
    # 运行测试
    if [[ "$RUN_ALL" == true || "$RUN_PUMPFUN" == true ]]; then
        if ! run_pumpfun_tests; then
            failed_tests+=("PumpFun")
        fi
        echo ""
    fi
    
    if [[ "$RUN_ALL" == true || "$RUN_RAYDIUM" == true ]]; then
        if ! run_raydium_tests; then
            failed_tests+=("Raydium")
        fi
        echo ""
    fi
    
    if [[ "$RUN_ALL" == true || "$RUN_INTEGRATION" == true ]]; then
        if ! run_integration_tests; then
            failed_tests+=("Integration")
        fi
        echo ""
    fi
    
    if [[ "$RUN_ALL" == true || "$RUN_PERFORMANCE" == true ]]; then
        if ! run_performance_tests; then
            failed_tests+=("Performance")
        fi
        echo ""
    fi
    
    # 如果是运行所有测试，也运行基础测试
    if [[ "$RUN_ALL" == true ]]; then
        if ! run_basic_tests; then
            failed_tests+=("Basic")
        fi
        echo ""
    fi
    
    # 计算耗时
    local end_time=$(date +%s)
    local duration=$((end_time - start_time))
    
    # 生成报告
    generate_test_report
    
    # 输出总结
    print_header "测试总结"
    
    echo -e "${CYAN}总耗时: ${duration}秒${NC}"
    echo ""
    
    if [[ ${#failed_tests[@]} -eq 0 ]]; then
        print_success "所有测试都通过了! 🎉"
        echo ""
        echo -e "${GREEN}你现在可以放心地运行交易逻辑了${NC}"
        exit 0
    else
        print_error "以下测试失败:"
        for test in "${failed_tests[@]}"; do
            echo -e "${RED}  - $test${NC}"
        done
        echo ""
        echo -e "${YELLOW}请检查失败的测试并修复问题${NC}"
        exit 1
    fi
}

# 运行主函数
main "$@"