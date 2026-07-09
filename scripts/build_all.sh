#!/bin/bash
# CDitor V2 构建验证脚本

echo "🔨 开始构建所有 crate..."
echo ""

# 按依赖顺序构建
crates=(
    "cditor-core"
    "cditor-storage-traits"
    "cditor-storage-postgres"
    "cditor-runtime"
    "cditor-editor"
    "cditor-gpui"
    "cditor-cli"
)

success_count=0
fail_count=0

for crate in "${crates[@]}"; do
    echo "Building $crate..."
    if cargo build -p "$crate" 2>&1 | grep -q "Finished"; then
        echo "✅ $crate 构建成功"
        ((success_count++))
    else
        echo "❌ $crate 构建失败"
        ((fail_count++))
    fi
    echo ""
done

echo "================================"
echo "构建结果: $success_count 成功, $fail_count 失败"
echo "================================"

if [ $fail_count -eq 0 ]; then
    echo "🎉 所有 crate 构建成功！"
    exit 0
else
    echo "⚠️  部分 crate 需要进一步修复"
    exit 1
fi
