#!/bin/bash
# 注释掉 cditor_editor 引用以打破循环依赖

echo "🔧 注释掉循环依赖引用..."

find crates/cditor-runtime/src -name "*.rs" -type f -exec sed -i '' 's/^use cditor_editor/\/\/ TODO: Fix circular dependency - use cditor_editor/g' {} \;
find crates/cditor-runtime/src -name "*.rs" -type f -exec sed -i '' 's/^    use cditor_editor/    \/\/ TODO: Fix circular dependency - use cditor_editor/g' {} \;

echo "✅ 完成"
