# Unknown Plugin 无损往返验收记录

> 日期：2026-07-22
>
> 对应总清单：P1-010、Gate P1 unknown kind/field/plugin、P5-012、Gate P5 unknown preservation

## 1. 协议与降级边界

未知插件使用 `RichBlockKind::Custom(plugin_kind)` 保存稳定插件 kind，使用
`BlockPayload::Opaque` 携带 `SchemaDomain::BlockPayload` 的 `VersionedEnvelope`。envelope body
是 `serde_json::value::RawValue`，所以非常规空白、字段顺序、Unicode 转义与未知嵌套字段
不会被 parse/re-serialize 归一化。Runtime 不创建文本编辑模型，只显示随 envelope 一起保存的
安全纯文本 fallback；当前 build 不解释、不执行未知插件数据。

所有反序列化入口重新验证 opaque envelope domain，避免绕过构造器：

- Core 构造器和显式 invariant validator；
- SQLite load/save；
- PostgreSQL load/save；
- native clipboard metadata；
- Runtime cold-start storage boundary。

## 2. 跨边界存储策略

| 边界 | 无损策略 |
|---|---|
| Core copy/move/serde | clone/move `RawValue`，比较原始 `body_bytes()` |
| SQLite | `payload_json TEXT` 直接序列化 opaque payload，重开数据库后验证 body 字节 |
| PostgreSQL | opaque payload 写入 `payload_bytes BYTEA`，`payload_json` 为 NULL；已知 payload 继续使用 JSONB |
| Clipboard | `CditorClipboardEnvelope` 原样携带 opaque payload，并校验 domain 与 metadata 大小 |
| Runtime | 整 Block copy/paste、undo/redo 保留 envelope；编辑能力降级为只读 fallback |

PostgreSQL 不允许 opaque payload 进入 JSONB codec，因为 JSONB 会归一化字段顺序和空白。
专用 codec 测试确保未来重构无法误走有损路径。

## 3. 共用 Fixture

`cditor_core::fixtures::unknown` 是所有层共用的单一 fixture。它包含：

- 未注册插件 kind：`future.vendor/interactive-card`；
- 新 minor 版本的 block payload envelope；
- 未知顶层与嵌套字段；
- 非标准空白、字段顺序、Unicode 转义；
- 可安全展示的纯文本 fallback。

验收不是比较解析后的 JSON 等价，而是逐字节比较 `VersionedEnvelope::body_bytes()`。

## 4. 自动化证据

- Core：fixture serde copy/move 与错误 domain 构造/反序列化拒绝。
- Runtime：整 Block copy/paste/undo/redo 字节不变；cold-start 拒绝错误 domain。
- SQLite：commit -> 关闭 -> reopen -> load 字节不变；读写均拒绝错误 domain。
- PostgreSQL：真实 Docker PostgreSQL 验证 BYTEA 写入、JSONB 为空、reload 字节不变；JSONB
  codec 单测拒绝 opaque payload。
- Clipboard：native metadata encode/decode 后 kind、fallback、raw body 全部不变。

Clipboard 的不可信输入矩阵还覆盖总大小、schema/version、system text 绑定、checksum、全局
span/cell 预算、kind/payload 冒充、重复 ID、前向/缺失 parent、parent/depth 不一致、危险
link/resource、错误 envelope domain 和嵌套 image caption 链接。任一失败都整体降级到外部纯文本
路径，不会部分接受 native metadata。

定向入口：

```text
cargo test -p cditor-core unknown
cargo test -p cditor-import-export native_clipboard_preserves_unknown_plugin_envelope_bytes
cargo test -p cditor-runtime whole_block_copy_paste_undo_redo_preserves_opaque_plugin_bytes
cargo test -p cditor-storage-sqlite unknown_plugin_payload
CDITOR_TEST_DATABASE_URL=postgres://cditor:cditor@localhost:5433/cditor_test \
  cargo test -p cditor-storage-postgres \
  postgres_payload_store_uses_bytea_for_lossless_unknown_plugin_payload \
  -- --ignored --test-threads=1
```

真实 PostgreSQL 集成测试与以上定向测试均已通过；最终 workspace tests、strict Clippy、fmt、
diff 和结构门禁随本项完成记录执行。
