#!/bin/bash
# CDitor V2 - 一键状态检查脚本

echo "╔══════════════════════════════════════════════════════════════════╗"
echo "║          CDitor V2 多 Crate 架构拆分 - 状态检查                ║"
echo "╚══════════════════════════════════════════════════════════════════╝"
echo ""

echo "📊 编译状态检查..."
echo ""

check_crate() {
    local crate=$1
    echo -n "  [$crate] ... "
    if cargo build -p "$crate" 2>&1 | grep -q "Finished"; then
        echo "✅ 成功"
        return 0
    else
        echo "❌ 失败"
        return 1
    fi
}

success=0
total=7

check_crate "cditor-core" && ((success++))
check_crate "cditor-storage-traits" && ((success++))
check_crate "cditor-storage-postgres" && ((success++))
check_crate "cditor-runtime" && ((success++))
check_crate "cditor-editor" && ((success++))
check_crate "cditor-gpui" && ((success++))
check_crate "cditor-cli" && ((success++))

echo ""
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "结果: $success/$total crates 编译成功"
echo "完成度: $(( success * 100 / total ))%"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo ""

if [ $success -eq $total ]; then
    echo "🎉 所有 crate 编译成功！可以运行编辑器了："
    echo "   cargo run -p cditor-cli"
else
    echo "📝 还需要继续修复 $(( total - success )) 个 crate"
    echo ""
    echo "💡 使用旧代码启动编辑器："
    echo "   ./START_EDITOR.sh"
    echo ""
    echo "📚 查看详细报告："
    echo "   cat FINAL_STATUS_REPORT.md"
fi

echo ""
