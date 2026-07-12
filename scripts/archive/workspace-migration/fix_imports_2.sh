#!/bin/bash

# Fix editor::scroll references - ScrollAnchor is now in cditor_core
find crates -name "*.rs" -type f -exec sed -i '' 's/crate::editor::scroll::ScrollAnchor/cditor_core::ScrollAnchor/g' {} \;
find crates -name "*.rs" -type f -exec sed -i '' 's/use crate::editor::scroll/use cditor_core::scroll/g' {} \;

# Fix gui references
find crates/cditor-runtime/src -name "*.rs" -type f -exec sed -i '' 's/crate::gui::/cditor_gpui::/g' {} \;
find crates/cditor-runtime/src -name "*.rs" -type f -exec sed -i '' 's/use crate::gui/use cditor_gpui/g' {} \;

# Fix api references
find crates/cditor-runtime/src -name "*.rs" -type f -exec sed -i '' 's/crate::api::/cditor_editor::/g' {} \;
find crates/cditor-runtime/src -name "*.rs" -type f -exec sed -i '' 's/use crate::api/use cditor_editor/g' {} \;

echo "Additional import paths fixed!"
