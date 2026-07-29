# CDitor V2 - 多 Crate 架构迁移

## 项目结构

```
CDitor-V2/
├── Cargo.toml                 # Workspace 配置
├── crates/
│   ├── cditor-core/          # 核心类型和 trait（无 async、无 storage、无 UI）
│   │   ├── src/
│   │   │   ├── lib.rs
│   │   │   ├── ids.rs        # BlockId, DocumentId
│   │   │   ├── version.rs    # StructureVersion
│   │   │   ├── block/        # Block 相关类型
│   │   │   ├── document/     # Document 索引
│   │   │   ├── layout/       # 布局相关类型
│   │   │   ├── rich_text/    # 富文本类型
│   │   │   ├── edit/         # 编辑操作类型
│   │   │   └── error.rs      # CoreError
│   │   └── Cargo.toml
│   │
│   ├── cditor-storage-traits/ # 存储层抽象（纯 trait 定义）
│   │   ├── src/
│   │   │   ├── lib.rs
│   │   │   ├── document.rs   # DocumentStore trait
│   │   │   ├── block.rs      # BlockStore trait
│   │   │   ├── layout.rs     # LayoutCache trait
│   │   │   ├── asset.rs      # AssetStore trait
│   │   │   └── error.rs      # StorageError
│   │   └── Cargo.toml
│   │
│   ├── cditor-storage-postgres/ # PostgreSQL 存储实现
│   │   ├── src/
│   │   │   ├── lib.rs
│   │   │   └── postgres/     # 具体实现
│   │   └── Cargo.toml
│   │
│   ├── cditor-runtime/       # 运行时状态管理
│   │   ├── src/
│   │   │   ├── lib.rs
│   │   │   ├── document_runtime.rs    # DocumentRuntime
│   │   │   ├── virtual_scroll.rs      # VirtualScrollState
│   │   │   ├── window_planner.rs      # WindowPlanner
│   │   │   ├── entity_cache.rs        # EntityCache
│   │   │   ├── height_index.rs        # BlockHeightIndex
│   │   │   ├── layout_scheduler.rs    # LayoutScheduler
│   │   │   ├── editing_session.rs     # EditingSession
│   │   │   ├── composition.rs         # IME Composition
│   │   │   └── error.rs               # RuntimeError
│   │   └── Cargo.toml
│   │
│   ├── cditor-editor/        # 编辑器逻辑（框架无关）
│   │   ├── src/
│   │   │   ├── lib.rs
│   │   │   ├── operations.rs          # EditOperation
│   │   │   ├── commands.rs            # BlockCommand
│   │   │   ├── transactions.rs        # EditTransaction
│   │   │   ├── selection.rs           # Selection 管理
│   │   │   ├── clipboard.rs           # Copy/Cut/Paste
│   │   │   ├── keyboard.rs            # 键盘映射
│   │   │   └── error.rs               # EditorError
│   │   └── Cargo.toml
│   │
│   ├── cditor-gpui/          # GPUI 渲染层
│   │   ├── src/
│   │   │   ├── lib.rs
│   │   │   ├── editor_view.rs         # 主编辑器视图
│   │   │   ├── block_view.rs          # Block 渲染
│   │   │   ├── theme.rs               # 主题系统
│   │   │   ├── input_handler.rs       # 输入处理
│   │   │   └── scrollbar.rs           # 滚动条
│   │   └── Cargo.toml
│   │
│   └── cditor-cli/           # CLI 入口
│       ├── src/
│       │   └── main.rs
│       └── Cargo.toml
│
├── migrations/               # 数据库迁移
├── assets/                   # 静态资源
├── examples/                 # 示例代码
└── doc/                      # 设计文档
```

## Crate 依赖关系

```
cditor-cli
  └─> cditor-gpui
       ├─> cditor-editor
       │    ├─> cditor-runtime
       │    │    ├─> cditor-storage-traits
       │    │    │    └─> cditor-core
       │    │    └─> cditor-core
       │    └─> cditor-core
       └─> cditor-runtime
            └─> cditor-storage-traits
                 └─> cditor-core

cditor-storage-postgres
  ├─> cditor-storage-traits
  │    └─> cditor-core
  └─> cditor-core
```

## 模块职责

### cditor-core
- **职责**: 纯数据类型和 trait 定义
- **不包含**: async/await、存储实现、UI 代码
- **导出**: BlockId, DocumentId, BlockKind, BlockRecord, LayoutMeta 等

### cditor-storage-traits
- **职责**: 存储层抽象接口
- **依赖**: cditor-core, async-trait
- **导出**: DocumentStore, BlockStore, LayoutCache 等 trait

### cditor-storage-postgres
- **职责**: PostgreSQL 存储实现
- **依赖**: cditor-core, cditor-storage-traits, sqlx
- **导出**: PostgresStore

### cditor-runtime
- **职责**: 运行时状态、虚拟滚动、窗口管理
- **依赖**: cditor-core, cditor-storage-traits
- **导出**: DocumentRuntime, VirtualScrollState, WindowPlanner

### cditor-editor
- **职责**: 编辑器逻辑、事务、选择、剪贴板
- **依赖**: cditor-core, cditor-runtime
- **导出**: EditTransaction, Selection, BlockCommand

### cditor-gpui
- **职责**: GPUI 渲染和交互
- **依赖**: cditor-core, cditor-runtime, cditor-editor, gpui
- **导出**: EditorView

### cditor-cli
- **职责**: 主程序入口
- **依赖**: 所有上层 crate
- **导出**: 可执行文件 `cditor`

## 构建和开发

```bash
# 构建所有 crate
cargo build --workspace

# 构建特定 crate
cargo build -p cditor-core
cargo build -p cditor-runtime

# 运行主程序
cargo run -p cditor-cli

# 运行测试
cargo test --workspace

# 检查所有 crate
cargo check --workspace
```

## 迁移状态

- [x] Workspace 结构创建
- [x] 各 crate Cargo.toml 配置
- [x] 代码文件迁移
- [x] 导入路径修复脚本
- [ ] 编译错误修复（进行中）
- [ ] 测试迁移
- [ ] 文档更新

## 下一步

1. 修复所有编译错误
2. 更新模块间的依赖引用
3. 添加集成测试
4. 完善各 crate 的 README
5. 配置 CI/CD pipeline
