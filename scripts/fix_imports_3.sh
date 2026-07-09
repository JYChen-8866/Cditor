#!/bin/bash
# 第三轮导入路径修复

echo "🔧 修复第三轮导入路径..."

# 修复 crate::storage 引用
find crates/cditor-runtime/src -name "*.rs" -type f -exec sed -i '' 's/use crate::storage::/use cditor_storage_traits::/g' {} \;
find crates/cditor-runtime/src -name "*.rs" -type f -exec sed -i '' 's/crate::storage::/cditor_storage_traits::/g' {} \;

# 修复 crate::runtime 自引用（应该用 crate:: 或 super::）
find crates/cditor-runtime/src -name "*.rs" -type f -exec sed -i '' 's/use crate::runtime::/use crate::/g' {} \;

# 修复 postgres 引用
find crates/cditor-runtime/src -name "*.rs" -type f -exec sed -i '' 's/cditor_storage_traits::postgres/cditor_storage_postgres/g' {} \;

# 修复 layout_cache 引用
find crates/cditor-runtime/src -name "*.rs" -type f -exec sed -i '' 's/cditor_storage_traits::layout_cache/cditor_storage_traits/g' {} \;

# 修复 editor 引用
find crates/cditor-runtime/src -name "*.rs" -type f -exec sed -i '' 's/use crate::editor::/use cditor_editor::/g' {} \;
find crates/cditor-runtime/src -name "*.rs" -type f -exec sed -i '' 's/crate::editor::/cditor_editor::/g' {} \;

echo "✅ 第三轮修复完成"
