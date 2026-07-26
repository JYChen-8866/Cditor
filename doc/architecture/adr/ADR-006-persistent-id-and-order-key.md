# ADR-006：PersistentId 采用 UUIDv7，顺序键采用 base-256 fractional key

> 状态：Accepted
>
> 日期：2026-07-17
>
> 关联章节：`doc/architecture/cditor-mature-notion-editor-master-design.md` 第 6.1 节
>
> 关联任务：P1-001 ~ P1-006

## 1. 背景

总设计 6.1 要求所有持久化 ID（Workspace、Document、Block、Surface、Row、Column、
Collection、Property、View、Operation、Actor、Device、Asset）统一为"UUIDv7 或
ULID 128 位"，允许离线生成，但未裁决具体格式；Block sibling 顺序要求可比较的
fractional order key，未裁决编码。当前代码的 `DocumentId`/`BlockId` 是进程内
`u64`（`crates/cditor-core/src/ids.rs`），顺序依赖 `Vec` 位置，两者都不能进入网络协议
或多设备持久化。

## 2. 候选方案

### 候选 A：UUIDv7（RFC 9562）

48 位 Unix 毫秒时间戳 + 12 位单调计数 + 62 位随机。字节序即时间序；`uuid`
crate 1.x 已在依赖树中；PostgreSQL 17+ 原生支持 `uuidv7()`，SQLite 存 16 字节
BLOB。标准化程度最高。

### 候选 B：ULID

同为 128 位、时间可排序，文本形态是 26 字符 Crockford base32。规范非 IETF
标准，单调性依赖实现约定；Rust 生态 `ulid` crate 需新增依赖；数据库侧无原生
类型，工具链（psql、grafana、日志检索）对 UUID 的支持远好于 ULID。

### 候选 C：UUIDv4

完全随机，无时间局部性；B-tree 插入位置随机导致索引页分裂，大表写放大明显。
不满足"以时间为主序"的分页/compaction 假设。否决。

### 顺序键候选：LexoRank（base-36 字符串 + bucket）/ f64 中点 / base-256 字节串

- f64 中点：约 50 次连续中点插入后精度耗尽，必须全量重排，违反 6.1 的
  "rebalance 不改变 BlockId"约束下的局部性要求。否决。
- LexoRank：可用，但 bucket 轮转是全局操作，且 base-36 字符串在 Rust 侧比较
  与存储都不如原始字节串紧凑。
- base-256 字节串（fractional indexing 的字节版）：`Vec<u8>` 字典序即文档序，
  中点算法输出最短可行 key，SQLite/PostgreSQL 均可作 BLOB 索引。

## 3. 决定

- PersistentId = **UUIDv7**，`uuid` crate 表示，序列化为 16 字节（二进制存储）
  或标准 hyphenated 字符串（JSON）。每类实体一个 newtype typed ID，禁止裸
  `Uuid` 跨界传递。
- 生成器实现 RFC 9562 Method 3（rand_a 作 12 位单调计数）：同毫秒内计数递增；
  时钟回拨时冻结在已见过的最大毫秒并继续计数，计数溢出则毫秒 +1；62 位
  rand_b 每次取新熵。时间源与熵源可注入，保证可测试。
- 顺序键 = **base-256 fractional key**（`OrderKey`）：非空 `Vec<u8>`、不以
  0x00 结尾；`between/first/before/after` 生成最短 key；并发插入用熵尾缀
  消歧；`rebalance` 只对指定 sibling 区间生成等距短 key，不触碰 Block 身份。
- 不可逆项：持久化格式一旦写入生产数据，ID 字节布局与 key 字典序语义不可再改。
  生成器实现、arena、映射表可替换。

## 4. 代价

- 128 位 ID 相比 u64 使索引与外键体积翻倍；以 arena/`RuntimeHandle(u64)`
  隔离热路径（P1-002），渲染/布局路径不接触 128 位 ID。
- UUIDv7 泄露创建时间戳（毫秒级）。产品层不把 ID 用作安全令牌；分享链接等
  另行签名。
- fractional key 在恶意/极端交替插入下会增长；由局部 rebalance 兜底，属于
  已知维护成本。

## 5. 迁移

Legacy `u64` -> UUIDv7 通过持久映射表（P1-003）：迁移时为每个既有实体生成一次
UUIDv7 并固化映射；运行期新实体直接生成。`u64` 别名保留为 Runtime handle，
Phase 15 才切换持久层主键。顺序键迁移：首次迁移按现有 `Vec` 顺序调用
`rebalance` 批量生成。

## 6. 回滚

映射表保留双向索引，Phase 15 前任何时刻可回读 legacy id。写入生产前
（Phase 7/8 落地前）回滚只需删除新模块；写入后回滚需按 migration checklist
的备份恢复路径执行。

## 7. 测试

- 生成器：同毫秒单调、跨毫秒递增、时钟回拨不回退、计数溢出进位、并发唯一性
  （P1-004）。
- OrderKey：between 严格序、最短性、边界（None/None、头插、尾插）、深度增长、
  并发插入消歧、rebalance 保序且不重复（P1-005/006）。
- 映射表：双向一致、冲突拒绝、序列化 round-trip（P1-003）。
- property test：随机操作序列下 ID 唯一与 key 全序不变量（P1-012 一部分）。

## 8. 需要更新的文档章节

| 文档 | 章节 | 更新内容 |
|---|---|---|
| `cditor-mature-notion-editor-master-design.md` | 第 6.1 节 | "UUIDv7 或 ULID" 裁决为 UUIDv7；顺序键裁决为 base-256 fractional key |
| `cditor-mature-notion-editor-master-design.md` | 第 32 节 Phase 1 | P1-001~006 勾选与证据 |
