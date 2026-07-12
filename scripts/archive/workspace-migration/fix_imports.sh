#!/bin/bash

# Fix imports in cditor-core
find crates/cditor-core/src -name "*.rs" -type f -exec sed -i '' 's/crate::core::/crate::/g' {} \;
find crates/cditor-core/src -name "*.rs" -type f -exec sed -i '' 's/use crate::core/use crate/g' {} \;

# Fix imports in cditor-storage-traits
find crates/cditor-storage-traits/src -name "*.rs" -type f -exec sed -i '' 's/crate::core::/cditor_core::/g' {} \;
find crates/cditor-storage-traits/src -name "*.rs" -type f -exec sed -i '' 's/use crate::core/use cditor_core/g' {} \;

# Fix imports in cditor-runtime
find crates/cditor-runtime/src -name "*.rs" -type f -exec sed -i '' 's/crate::core::/cditor_core::/g' {} \;
find crates/cditor-runtime/src -name "*.rs" -type f -exec sed -i '' 's/use crate::core/use cditor_core/g' {} \;
find crates/cditor-runtime/src -name "*.rs" -type f -exec sed -i '' 's/crate::storage::/cditor_storage_traits::/g' {} \;
find crates/cditor-runtime/src -name "*.rs" -type f -exec sed -i '' 's/use crate::storage/use cditor_storage_traits/g' {} \;

# Fix imports in cditor-editor
find crates/cditor-editor/src -name "*.rs" -type f -exec sed -i '' 's/crate::core::/cditor_core::/g' {} \;
find crates/cditor-editor/src -name "*.rs" -type f -exec sed -i '' 's/use crate::core/use cditor_core/g' {} \;
find crates/cditor-editor/src -name "*.rs" -type f -exec sed -i '' 's/crate::runtime::/cditor_runtime::/g' {} \;
find crates/cditor-editor/src -name "*.rs" -type f -exec sed -i '' 's/use crate::runtime/use cditor_runtime/g' {} \;

# Fix imports in cditor-storage-postgres
find crates/cditor-storage-postgres/src -name "*.rs" -type f -exec sed -i '' 's/crate::core::/cditor_core::/g' {} \;
find crates/cditor-storage-postgres/src -name "*.rs" -type f -exec sed -i '' 's/use crate::core/use cditor_core/g' {} \;
find crates/cditor-storage-postgres/src -name "*.rs" -type f -exec sed -i '' 's/crate::storage::traits/cditor_storage_traits/g' {} \;
find crates/cditor-storage-postgres/src -name "*.rs" -type f -exec sed -i '' 's/use crate::storage::traits/use cditor_storage_traits/g' {} \;

# Fix imports in cditor-gpui
find crates/cditor-gpui/src -name "*.rs" -type f -exec sed -i '' 's/crate::core::/cditor_core::/g' {} \;
find crates/cditor-gpui/src -name "*.rs" -type f -exec sed -i '' 's/use crate::core/use cditor_core/g' {} \;
find crates/cditor-gpui/src -name "*.rs" -type f -exec sed -i '' 's/crate::runtime::/cditor_runtime::/g' {} \;
find crates/cditor-gpui/src -name "*.rs" -type f -exec sed -i '' 's/use crate::runtime/use cditor_runtime/g' {} \;
find crates/cditor-gpui/src -name "*.rs" -type f -exec sed -i '' 's/crate::editor::/cditor_editor::/g' {} \;
find crates/cditor-gpui/src -name "*.rs" -type f -exec sed -i '' 's/use crate::editor/use cditor_editor/g' {} \;

echo "Import paths fixed!"
