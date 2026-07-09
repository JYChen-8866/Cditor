# CDitor V2 - 大文档富文本编辑器

基于 GPUI 的高性能富文本编辑器，支持 10 万+ block 的大文档编辑。

## 🏗️ 架构

本项目采用多 crate workspace 架构：

```
crates/
├── cditor-core              # 核心类型和算法（无 async/storage/UI 依赖）
├── cditor-storage-traits    # 存储层抽象接口
├── cditor-storage-postgres  # PostgreSQL 存储实现
├── cditor-runtime          # 运行时状态管理（虚拟滚动、窗口规划）
├── cditor-editor           # 编辑器逻辑（事务、选择、剪贴板）
├── cditor-gpui             # GPUI 渲染和交互层
└── cditor-cli              # CLI 入口
```

## 🚀 快速开始

### 构建

```bash
# 构建所有 crate
cargo build --workspace

# 构建特定 crate
cargo build -p cditor-core

# 运行主程序
cargo run -p cditor-cli
```

### 测试

```bash
# 运行所有测试
cargo test --workspace

# 运行特定 crate 的测试
cargo test -p cditor-core
```

## 📊 当前状态

- ✅ cditor-core: 编译通过
- ✅ cditor-storage-traits: 编译通过
- 🔄 其他 crate: 修复中

详见 [CRATE_MIGRATION_FINAL_REPORT.md](./CRATE_MIGRATION_FINAL_REPORT.md)

## 📚 文档

- [架构设计文档](doc/large-document-rich-text-architecture.md)
- [Crate 迁移指南](doc/crate-migration-guide.md)
- [迁移最终报告](CRATE_MIGRATION_FINAL_REPORT.md)

## 🛠️ 开发

### 依赖

- Rust 2024 edition
- PostgreSQL (可选)
- GPUI

### 脚本

```bash
./scripts/build_all.sh       # 构建所有 crate 并报告状态
./scripts/fix_imports.sh     # 修复导入路径（已执行）
./scripts/fix_imports_2.sh   # 第二轮路径修复（已执行）
```

## 📝 License

(待添加)
