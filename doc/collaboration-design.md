# Cditor 协同编辑方案

> 基于对 Cditor-V2 代码库的深入分析（ChangeOrigin、EditTransaction、OrderKey、UndoStack）。
> 参考 Zed CRDT 架构，适配 Cditor 的 Block 富文本编辑器模型。

---

## 目录

1. [代码现状分析](#1-代码现状分析)
2. [核心矛盾：现有的逆操作模型 vs CRDT](#2-核心矛盾现有的逆操作模型-vs-crdt)
3. [方案选择：混合模型](#3-方案选择混合模型)
4. [CRDT 数据模型设计](#4-crdt-数据模型设计)
5. [操作编码与传输](#5-操作编码与传输)
6. [与 DocumentRuntime 的集成](#6-与-documentruntime-的集成)
7. [Undo/Redo 协同化](#7-undoredo-协同化)
8. [服务端架构](#8-服务端架构)
9. [同步协议](#9-同步协议)
10. [Presence 系统](#10-presence-系统)
11. [crate 结构与依赖](#11-crate-结构与依赖)
12. [分阶段实施](#12-分阶段实施)

---

## 1. 代码现状分析

### 1.1 已有的协同基础设施

Cditor 已经为协同做了明确的前期设计，而非事后补丁：

| 基础设施 | 位置 | 状态 | 协同价值 |
|---|---|---|---|
| `ChangeOrigin::Remote` | `core/src/edit/origin.rs:24` | ✅ 已定义 | 远程操作不进本地 undo、不标记 dirty |
| `OrderKey` | `core/src/identity/order_key.rs` | ✅ 完整实现 | Fractional Indexing，可并发插入消歧 |
| `OrderKey::between_with_entropy` | `order_key.rs:88` | ✅ 已实现 | 两端同时在同一位置插入时不冲突 |
| `TransactionPrecondition` | `core/src/edit/transactions.rs:223` | ✅ 已定义 | DocumentRevision/StructureVersion/BlockContentVersion 可做乐观锁 |
| `PersistentIdGenerator` | `core/src/identity/generator.rs` | ✅ RFC 9562 | UUIDv7 单调生成，可作 replica_id |
| `EditTransaction` 含 inverse_ops | `transactions.rs:251` | ✅ 完整 | 当前用逆操作做 undo，协同下需调整 |
| `UndoStack` 含 typing coalescing | `core/src/edit/undo.rs` | ✅ 完整 | 需升级为 per-user undo map |

### 1.2 需要改造的部分

| 问题 | 现状 | 改造方向 |
|---|---|---|
| 文本操作用绝对偏移量 | `InsertText { offset: usize }` | 改为 CRDT Anchor：`(fragment_id, offset)` |
| 删除是破坏性的 | `DeleteText { range }` | 改为 Tombstone 标记 |
| Undo 是全局单栈 | `UndoStack { undo: VecDeque }` | 改为 per-user undo map |
| 块 ID 是本地 u64 | `BlockId = u64` | 改为全局唯一 `(replica_id, seq)` |
| 没有操作日志 | — | 新增 Journal（不可变操作序列） |

---

## 2. 核心矛盾：现有的逆操作模型 vs CRDT

### 2.1 当前模型的假设

```rust
// 当前：每个 EditTransaction 包含逆操作，undo = apply(inverse_ops)
pub struct EditTransaction {
    pub ops: Arc<Vec<EditOperation>>,        // 正向操作
    pub inverse_ops: Arc<Vec<EditOperation>>, // 逆操作
}

// InsertText 的 inverse 是 DeleteText（用相同的绝对偏移量）
fn insert_text(id, ts, block_id, offset, text) -> EditTransaction {
    Self::new(
        ops: vec![InsertText { block_id, offset, text: "hello" }],
        inverse_ops: vec![DeleteText { block_id, range: offset..offset+5 }],
    )
}
```

**这个模型假设：** undo 时文档状态和 insert 时完全一致。在协同场景下这个假设不成立——其他用户可能已经在 offset 前后插入了文字，`DeleteText { range: 3..8 }` 可能删错东西。

### 2.2 CRDT 替代方案

CRDT 不做逆操作，而是：

1. **插入的文字永不修改**——给它一个全局唯一 ID
2. **删除只是打标记**（Tombstone）——不改变其他 fragment 的位置
3. **Undo 是 undo_map[id] += 1**——不依赖逆操作

### 2.3 选择：混合模型

不让现有架构推倒重来。保持 `EditTransaction` 作为 Cditor 内部的编辑模型，新增一层 CRDT 作为协同的"传输格式"：

```
            现有路径（单用户）                  协同路径
                │                                 │
    用户输入 → EditTransaction → DocumentRuntime    │
                │                                 │
                │                      EditTransaction
                │                           │
                │              ┌────────────▼────────────┐
                │              │   TransactionToCRDT     │ ← 新增转换层
                │              │   (ops → CRDT ops)      │
                │              └────────────┬────────────┘
                │                           │
                │              ┌────────────▼────────────┐
                │              │   CollaborativeOp       │ ← 传输/存储格式
                │              │   (Anchor + Tombstone)  │
                │              └────────────┬────────────┘
                │                           │
                │                     WebSocket → Server → 其他客户端
                │                                            │
                │                              ┌─────────────▼────────────┐
                │                              │   CRDTToTransaction      │
                │                              │   (CRDT ops → EditTrans) │
                │                              └─────────────┬────────────┘
                │                                            │
                │                              EditTransaction(origin=Remote)
                │                                            │
                │                              ┌─────────────▼────────────┐
                └──────────────────────────────│    DocumentRuntime        │
                                               │    (apply + rebuild)     │
                                               └──────────────────────────┘
```

**核心策略：编辑操作的"真理源"是 CRDT Journal，EditTransaction 只是视图。**

---

## 3. CRDT 数据模型设计

### 3.1 全局标识符体系

```rust
// crates/cditor-collaboration/src/ids.rs

/// 副本标识符（服务端分配，加入会话时获取）
pub type ReplicaId = u16;

/// 全局唯一的操作 ID
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct OperationId {
    pub replica: ReplicaId,
    pub sequence: u64,       // 每个 replica 自增
}

/// Lamport 逻辑时钟
pub type LamportTime = u64;
```

### 3.2 三层 CRDT

Cditor 的数据模型天然支持**三层独立 CRDT**：

```rust
/// 文档 CRDT 聚合根
pub struct DocumentCRDT {
    pub replica_id: ReplicaId,
    pub lamport_clock: LamportTime,
    pub version_vector: VersionVector,

    // Layer 1: 块结构 CRDT（树形结构 + 排序）
    pub block_tree: BlockTreeCRDT,

    // Layer 2: 富文本内容 CRDT（每个块独立）
    pub text_contents: HashMap<BlockOpId, TextContentCRDT>,

    // Layer 3: 块属性 CRDT（LWW Register）
    pub block_attrs: HashMap<BlockOpId, LwwAttributeSet>,
}
```

### 3.2.1 Layer 1：块结构 CRDT

```rust
/// 块操作 ID——同时作为块的身份标识
/// 一个块被创建后，它的 BlockOpId 就是它一生的身份
pub type BlockOpId = OperationId;

/// 块树节点
#[derive(Debug, Clone)]
pub struct BlockTreeNode {
    pub block_id: BlockOpId,              // 块的身份（= 创建操作的 ID）
    pub parent_id: Option<BlockOpId>,      // 父块（None = 根级）
    pub order_key: OrderKey,              // 同级排序（Fractional Indexing）
    pub kind: RichBlockKind,              // 块类型
    pub tombstone: Option<DeletionRecord>, // 删除标记
}

/// 块树 CRDT
///
/// 收敛策略：
/// - 创建：分配新的 BlockOpId + OrderKey（在同级中的位置）
/// - 删除：打 tombstone，子块级联隐藏
/// - 移动：分配新的 OrderKey（在目标位置之间）
/// - 排序冲突：同一间隙的并发插入用 OrderKey::between_with_entropy 消歧
pub struct BlockTreeCRDT {
    /// 所有块（包括被删的）
    nodes: BTreeMap<BlockOpId, BlockTreeNode>,
    /// 根级块的排序索引
    root_order: BTreeSet<(OrderKey, BlockOpId)>,
    /// 每个父块下的子块排序索引
    children: HashMap<BlockOpId, BTreeSet<(OrderKey, BlockOpId)>>,
}
```

**为什么用 OrderKey 而不是单纯的数组索引？**

- 数组索引做 CRDT 需要复杂的 RGA/Logoot 算法
- OrderKey 天然可并发：两个用户同时在两个块之间插入，`between_with_entropy` 保证两个新块有不同但有序的 key
- 移动块只需要分配一个新的 OrderKey，不影响其他块

### 3.2.2 Layer 2：富文本内容 CRDT

```rust
/// 文本插入片段（不可变）
#[derive(Debug, Clone)]
pub struct TextFragment {
    pub id: OperationId,                  // 全局唯一
    pub lamport: LamportTime,
    pub parent_fragment: OperationId,     // 在哪个已有 fragment 中插入
    pub parent_offset: usize,            // 在父 fragment 的字节偏移
    pub text: String,                     // 插入的文字（不可变）
    pub marks: Vec<(InlineMark, Range<usize>)>, // 行内标记（相对于本 fragment）
    pub tombstone: Option<DeletionRecord>,
}

/// 每个块的文本内容 CRDT
///
/// 所有 fragment 按 Lamport 时间戳排序（同时保证因果一致性和全序）。
pub struct TextContentCRDT {
    pub block_id: BlockOpId,
    /// 按 Lamport 降序排列的 fragment 集合
    fragments: BTreeMap<OperationId, TextFragment>,
    /// 从逻辑 Anchor 到物理偏移的快速索引
    anchor_index: AnchorIndex,
}

impl TextContentCRDT {
    /// 插入文字（本地操作）
    pub fn insert(&mut self, id: OperationId, lamport: LamportTime,
                  parent: OperationId, offset: usize, text: String) { ... }

    /// 删除文字（打 tombstone）
    pub fn delete(&mut self, id: OperationId, lamport: LamportTime,
                  anchor: CrdtAnchor, length: usize, visible_to: VersionVector) { ... }

    /// 获取可见文本（跳过 tombstoned fragments）
    pub fn visible_text(&self, undo_map: &UndoMap) -> String { ... }

    /// 获取可见 spans（用于富文本渲染）
    pub fn visible_spans(&self, undo_map: &UndoMap) -> Vec<InlineSpan> { ... }
}

/// CRDT Anchor：逻辑位置，不受并发编辑影响
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CrdtAnchor {
    pub fragment_id: OperationId,
    pub byte_offset: usize,
    pub affinity: CrdtAffinity,  // 前向/后向偏置（处理 fragment 边界）
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CrdtAffinity {
    Before,  // 偏向前一个 fragment
    After,   // 偏向后一个 fragment
}
```

**Anchor 的工作原理（和 Zed 一致）：**

```
初始状态：fragment (0,0): "Hello World"

用户 A 插入：在 (0,0) offset=6 处插入 "Beautiful "
  → 新 fragment (0,1): "Beautiful "
  → anchor (0,1, 0) 指向 "B"

用户 B 同时插入：在 (0,0) offset=6 处插入 "Big "
  → 新 fragment (1,1): "Big "
  → anchor (1,1, 0) 指向 "B"

结果（Lamport 排序后）："Hello Big Beautiful World" 或 "Hello Beautiful Big World"
                            （取决于 Lamport 时间戳，但两个副本一致）
```

### 3.2.3 Layer 3：块属性 CRDT

```rust
/// LWW (Last-Writer-Wins) Register
#[derive(Debug, Clone)]
pub struct LwwRegister<T: Clone + PartialEq> {
    pub value: T,
    pub updated_at: LamportTime,
    pub updated_by: OperationId,
}

impl<T: Clone + PartialEq> LwwRegister<T> {
    /// 应用远程更新
    pub fn apply(&mut self, value: T, lamport: LamportTime, by: OperationId) {
        // 更大的 Lamport 时间戳胜出；相等时 replica_id 大的胜出
        if lamport > self.updated_at
            || (lamport == self.updated_at && by.replica > self.updated_by.replica)
        {
            self.value = value;
            self.updated_at = lamport;
            self.updated_by = by;
        }
    }
}

/// 块的 LWW 属性集合
#[derive(Debug, Clone, Default)]
pub struct LwwAttributeSet {
    pub color: LwwRegister<Option<String>>,
    pub background_color: LwwRegister<Option<String>>,
    pub text_align: LwwRegister<TextAlign>,
    pub folded: LwwRegister<bool>,
    pub locked: LwwRegister<bool>,
}
```

### 3.3 删除记录与 Version Vector

```rust
/// 删除记录
#[derive(Debug, Clone)]
pub struct DeletionRecord {
    pub deleted_by: OperationId,        // 谁删的
    pub deleted_at: LamportTime,
    pub visible_to: VersionVector,      // 删除时"看到"的版本
}

/// Version Vector：记录每个 replica 已看到的最高 sequence
#[derive(Debug, Clone, Default)]
pub struct VersionVector {
    entries: BTreeMap<ReplicaId, u64>,
}

impl VersionVector {
    /// 检查某个操作在此版本向量代表的时刻是否"可见"
    pub fn covers(&self, id: OperationId) -> bool {
        self.entries.get(&id.replica).copied().unwrap_or(0) >= id.sequence
    }

    /// 合并：取每个 replica 的 max sequence
    pub fn merge(&mut self, other: &VersionVector) {
        for (replica, seq) in &other.entries {
            let entry = self.entries.entry(*replica).or_insert(0);
            *entry = (*entry).max(*seq);
        }
    }
}

/// Undo Map：每个用户的撤销/重做计数
pub type UndoMap = BTreeMap<OperationId, u32>;
// count = 0  → 正常
// count = 奇数 → 已撤销（隐藏）
// count = 偶数 → 已重做（显示）
```

---

## 4. 操作编码与传输

### 4.1 协同操作类型

```rust
/// 所有协同操作（与 EditOperation 不同，这是 CRDT 原生格式）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CollaborativeOp {
    // ── 块结构 ──
    InsertBlock {
        id: OperationId,
        lamport: LamportTime,
        parent: Option<BlockOpId>,
        order_key: OrderKey,
        kind: RichBlockKind,
        initial_payload: BlockPayload,
    },
    DeleteBlock {
        id: OperationId,
        lamport: LamportTime,
        block_id: BlockOpId,
        visible_to: VersionVector,
    },
    MoveBlock {
        id: OperationId,
        lamport: LamportTime,
        block_id: BlockOpId,
        new_parent: Option<BlockOpId>,
        new_order_key: OrderKey,
    },
    SetBlockKind {
        id: OperationId,
        lamport: LamportTime,
        block_id: BlockOpId,
        kind: RichBlockKind,
    },

    // ── 富文本 ──
    InsertText {
        id: OperationId,
        lamport: LamportTime,
        block_id: BlockOpId,
        parent_fragment: OperationId,
        parent_offset: usize,
        text: String,
        marks: Vec<(InlineMark, Range<usize>)>,
    },
    DeleteText {
        id: OperationId,
        lamport: LamportTime,
        block_id: BlockOpId,
        anchor: CrdtAnchor,
        length: usize,
        visible_to: VersionVector,
    },

    // ── 块属性 ──
    SetBlockAttr {
        id: OperationId,
        lamport: LamportTime,
        block_id: BlockOpId,
        key: BlockAttrKey,
        value: serde_json::Value,
    },

    // ── 富块 Payload 替换（Table/Collection/Image 等不可 CRDT 化的复杂结构）──
    ReplacePayload {
        id: OperationId,
        lamport: LamportTime,
        block_id: BlockOpId,
        before_kind: RichBlockKind,
        before_payload_hash: u64,     // 乐观锁：校验替换基础
        after_kind: RichBlockKind,
        after_payload: BlockPayload,
    },

    // ── Undo/Redo ──
    Undo {
        id: OperationId,
        lamport: LamportTime,
        target_ops: Vec<OperationId>,
    },
    Redo {
        id: OperationId,
        lamport: LamportTime,
        target_ops: Vec<OperationId>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BlockAttrKey {
    Color,
    BackgroundColor,
    TextAlign,
    Folded,
    Locked,
}
```

### 4.2 复杂块的策略：混合

Table、Collection、Mermaid、Whiteboard 等复杂块不能用纯文本 CRDT——它们有自己的结构语义。

**策略：**

| 块类型 | CRDT 策略 |
|---|---|
| Paragraph/Heading/Quote/Callout | 结构 CRDT（OrderKey）+ 文本 CRDT（Anchor） |
| Code | 文本 CRDT（整个 code block 内容作为单一 text CRDT） |
| Image/Video/File | 结构 CRDT + LWW payload（URL/MIME/size 用 LWW） |
| Table | 结构 CRDT + **表内文本用 cell 级 CRDT** + 行列操作用 LWW payload |
| Collection（数据库） | 结构 CRDT + **记录级 CRDT**（每条记录一个 CRDT 节点） |
| Divider | 结构 CRDT（无内容） |

**对于 Table：** 每个 Cell 内的文本作为独立的 mini CRDT（和 Paragraph 一样）。行列的增删、合并拆分使用 `ReplacePayload`——携带 before_payload_hash 做乐观锁，冲突时以 Lamport 时间戳大的为准。

---

## 5. 与 DocumentRuntime 的集成

### 5.1 双模式 DocumentRuntime

```rust
// runtime/src/document_runtime/state.rs 增量修改

pub enum DocumentMode {
    /// 单用户模式（现状）
    Local,
    /// 协同模式
    Collaborative {
        session: CollaborationSession,
    },
}

pub struct CollaborationSession {
    pub replica_id: ReplicaId,
    pub crdt: DocumentCRDT,
    pub connection: Arc<dyn CollaborationTransport>,
    pub remote_presence: PresenceState,
    /// 待发送的操作队列（下一帧 flush）
    pub outbox: Vec<CollaborativeOp>,
    /// 收到的远程操作（下一帧应用）
    pub inbox: Vec<CollaborativeOp>,
}
```

### 5.2 操作流程

```rust
impl DocumentRuntime {
    /// 路径 1：本地编辑 → CRDT outbox + 现有逻辑
    pub fn apply_local_edit(&mut self, transaction: EditTransaction) {
        // 只在协同模式下生成 CRDT 操作
        if let DocumentMode::Collaborative { session } = &mut self.mode {
            let crdt_ops = TransactionToCrdt::convert(&transaction, session);
            for op in crdt_ops {
                session.crdt.apply_local(op.clone());
                session.outbox.push(op);
            }
        }
        // 保持现有逻辑（更新 DocumentIndex / BlockHeightIndex）
        self.apply_transaction(transaction, ChangeOrigin::User);
    }

    /// 路径 2：远程操作 → CRDT inbox → 重建视图
    pub fn apply_remote_ops(&mut self) {
        let ops = if let DocumentMode::Collaborative { session } = &mut self.mode {
            std::mem::take(&mut session.inbox)
        } else {
            return;
        };
        if ops.is_empty() { return; }

        for op in &ops {
            if let DocumentMode::Collaborative { session } = &mut self.mode {
                session.crdt.apply_remote(op.clone());
            }
        }

        // 从 CRDT 重建 DocumentRuntime 状态
        self.rebuild_from_crdt();

        // 外部 undo 栈收到 remote 变更后更新
        self.external_undo_stack.record_remote_operations(&ops);
    }

    /// 每帧调用：发送 outbox + 应用 inbox
    pub fn sync_collaboration_frame(&mut self) {
        if let DocumentMode::Collaborative { session } = &mut self.mode {
            let outbox = std::mem::take(&mut session.outbox);
            if !outbox.is_empty() {
                session.connection.send(SyncMessage { operations: outbox });
            }
        }
        self.apply_remote_ops();
    }
}
```

### 5.3 从 CRDT 重建 DocumentRuntime

```rust
impl DocumentRuntime {
    /// 从 CRDT 状态完全重建 DocumentIndex / BlockHeightIndex / PayloadWindow
    fn rebuild_from_crdt(&mut self) {
        let crdt = match &self.mode {
            DocumentMode::Collaborative { session } => &session.crdt,
            _ => return,
        };

        // 1. 遍历块树（跳过 tombstoned 节点），重建可见块列表
        let visible_blocks = crdt.block_tree.visible_nodes(&crdt.undo_map);

        // 2. 重建 DocumentIndex（SoA 数组）
        self.index = DocumentIndex::from_crdt_nodes(&visible_blocks);

        // 3. 重建 DocumentRuntime 内部状态
        //    - 保留当前选择（用 CRDT anchor 重新定位）
        //    - 保留当前滚动位置
        //    - 重建 height_index（可以增量更新而非全量重建）
        self.height_index.update_from_crdt(&visible_blocks);

        // 4. 重建每个块的文本模型
        for (block_id, text_crdt) in &crdt.text_contents {
            let spans = text_crdt.visible_spans(&crdt.undo_map);
            let plain = plain_text_from_spans(&spans);
            self.text_models.insert(
                block_id.to_local_id(),  // CRDT id → 本地 id（通过 IdArena 映射）
                PieceTableTextModel::new(plain),
            );
        }

        // 5. 重建块属性
        for (block_id, attrs) in &crdt.block_attrs {
            if let Some(local_id) = self.id_arena.get_local(block_id) {
                self.block_attrs.insert(local_id, attrs.to_block_attrs());
            }
        }
    }
}
```

### 5.4 CRDT ID ↔ 本地 ID 映射

Cditor 内部使用 `BlockId = u64`（紧凑高效），协同使用 `BlockOpId = (ReplicaId, u64)`（全局唯一）。需要一个 `IdArena` 做双向映射：

```rust
// core/src/identity/arena.rs（已存在类似结构，需扩展）
pub struct IdArena {
    crdt_to_local: HashMap<BlockOpId, BlockId>,
    local_to_crdt: HashMap<BlockId, BlockOpId>,
    next_local: BlockId,
}
```

本地模式（非协同）：`BlockOpId = (0, local_id)`，无需映射。

### 5.5 EditTransaction → CollaborativeOp 转换

这是最关键的桥接代码。不需要一一对应转换，而是**增量生成** CRDT 操作：

```rust
/// EditTransaction → CollaborativeOp 转换器
pub struct TransactionToCrdt;

impl TransactionToCrdt {
    pub fn convert(
        transaction: &EditTransaction,
        session: &CollaborationSession,
    ) -> Vec<CollaborativeOp> {
        let mut ops = Vec::new();

        for edit_op in transaction.ops.iter() {
            match edit_op {
                EditOperation::InsertText { block_id, offset, text } => {
                    // 从绝对偏移转为 CRDT Anchor
                    let anchor = session.crdt
                        .text_contents[&block_id_to_crdt(*block_id)]
                        .offset_to_anchor(*offset);

                    ops.push(CollaborativeOp::InsertText {
                        id: session.next_op_id(),
                        lamport: session.crdt.lamport_clock,
                        block_id: block_id_to_crdt(*block_id),
                        parent_fragment: anchor.fragment_id,
                        parent_offset: anchor.byte_offset,
                        text: text.clone(),
                        marks: vec![],
                    });
                    session.crdt.lamport_clock += 1;
                }

                EditOperation::InsertBlock { index, block } => {
                    let crdt_id = session.next_op_id();
                    let order_key = session.crdt.block_tree
                        .order_key_for_index(*index);

                    ops.push(CollaborativeOp::InsertBlock {
                        id: crdt_id,
                        lamport: session.crdt.lamport_clock,
                        parent: None,  // 简化为根级
                        order_key,
                        kind: kind_from_tag(block.kind_tag),
                        initial_payload: BlockPayload::RichText { spans: vec![] },
                    });
                    session.crdt.lamport_clock += 1;
                }

                // BlockEditOperation::SetAttrs → CollaborativeOp::SetBlockAttr × N
                EditOperation::Block(BlockEditOperation::SetAttrs { block_id, before, after }) => {
                    let crdt_block_id = block_id_to_crdt(*block_id);
                    if before.color != after.color {
                        ops.push(CollaborativeOp::SetBlockAttr {
                            id: session.next_op_id(),
                            lamport: session.crdt.lamport_clock,
                            block_id: crdt_block_id,
                            key: BlockAttrKey::Color,
                            value: after.color.clone().into(),
                        });
                        session.crdt.lamport_clock += 1;
                    }
                    // ... 其他属性同理
                }

                // 复杂块（Table/Collection 等）：用 ReplacePayload
                EditOperation::Block(BlockEditOperation::ReplacePayload { block_id, before_payload, after_payload, .. }) => {
                    ops.push(CollaborativeOp::ReplacePayload {
                        id: session.next_op_id(),
                        lamport: session.crdt.lamport_clock,
                        block_id: block_id_to_crdt(*block_id),
                        before_kind: ...,
                        before_payload_hash: hash(before_payload),
                        after_kind: ...,
                        after_payload: after_payload.clone(),
                    });
                    session.crdt.lamport_clock += 1;
                }

                // 其他操作同理...
                _ => {}
            }
        }

        ops
    }
}
```

---

## 6. Undo/Redo 协同化

### 6.1 从全局 UndoStack 到 Per-User UndoMap

```rust
/// 协同版 UndoManager：保留本地 UndoStack 体验 + 全局 UndoMap
pub struct CollaborativeUndoManager {
    /// 本地 undo 历史（保持 Ctrl+Z 体验）
    /// 只记录本地用户的操作，不记录 Remote
    local_stack: Vec<LocalUndoEntry>,

    /// CRDT undo map（跨用户收敛）
    pub undo_map: UndoMap,
}

struct LocalUndoEntry {
    /// 对应的 CRDT 操作 ID 列表
    crdt_op_ids: Vec<OperationId>,
    /// 用于恢复选择位置
    before_selection: Option<DocumentSelection>,
    after_selection: Option<DocumentSelection>,
}
```

### 6.2 Undo 流程

```
用户按 Ctrl+Z
  │
  ├─ local_stack.pop() → 找到自己上一个操作的 CRDT ID 列表
  │
  ├─ 对每个 crdt_op_id：undo_map[id] += 1
  │
  ├─ 生成 CollaborativeOp::Undo { target_ops: [id1, id2] }
  │
  ├─ 本地 CRDT 应用 undo（重新渲染到 undo 后状态）
  │
  └─ 发送 Undo 操作到服务器
       │
       其他客户端收到后：
         undo_map[id] += 1
         重新渲染（被 undo 的 fragment 隐藏）
```

**注意：** `ChangeOrigin::Remote` 本身不进入 local_stack（origin.rs:44 已定义），所以远程操作不会出现在本地用户的 Ctrl+Z 历史中。但要撤销远程操作，可以在 UI 中手动 undo 他人的操作。

---

## 7. 服务端架构

### 7.1 服务端职责

```
cditor-collab-server (Rust)
  │
  ├─ WebSocket 连接管理
  │    ├─ 认证（JWT token）
  │    ├─ 文档加入/离开
  │    └─ 消息广播
  │
  ├─ CRDT Journal（不可变操作序列）
  │    ├─ 接收客户端操作 → 分配全局 journal_seq → 持久化 → 广播
  │    └─ 冲突仲裁（服务端版本优先）
  │
  ├─ 持久化
  │    ├─ Operation Journal → PostgreSQL
  │    └─ 定期快照（每 N 个操作）
  │
  └─ Presence 追踪
       ├─ 在线用户列表
       ├─ 远程光标/选择广播
       └─ 离开检测
```

### 7.2 Journal 持久化

```sql
-- 操作日志表（不可变，只追加）
CREATE TABLE collab_journal (
    journal_seq BIGSERIAL PRIMARY KEY,
    document_id BIGINT NOT NULL,
    replica_id INTEGER NOT NULL,
    operation_seq BIGINT NOT NULL,
    operation_kind TEXT NOT NULL,
    operation_payload JSONB NOT NULL,
    lamport BIGINT NOT NULL,
    server_timestamp TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    INDEX idx_journal_document (document_id, journal_seq)
);

-- 快照表（定期生成）
CREATE TABLE collab_snapshot (
    id BIGSERIAL PRIMARY KEY,
    document_id BIGINT NOT NULL,
    journal_seq BIGINT NOT NULL,    -- 快照包含到此 journal_seq
    crdt_state BYTEA NOT NULL,      -- DocumentCRDT 序列化
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
```

### 7.3 连接加入流程

```
新客户端连接 → 服务端：
  1. 验证 JWT token
  2. 分配 replica_id（从 DB 获取该文档的下一个 replica_id）
  3. 发送最新的快照（crdt_state）+ 快照之后的 journal 操作
  4. 通知其他客户端有新用户加入（Presence）
  5. 开始双向同步
```

---

## 8. 同步协议

### 8.1 WebSocket 消息格式

```rust
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CollabMessage {
    // Client → Server
    Auth { token: String },
    JoinDoc { document_id: u64 },
    LeaveDoc { document_id: u64 },
    Sync { version_vector: VersionVector, ops: Vec<CollaborativeOp> },
    Presence { selection: Option<CollaborativeSelection>, active_block: Option<BlockOpId> },
    Ping { timestamp: u64 },

    // Server → Client
    Welcome {
        replica_id: ReplicaId,
        document_id: u64,
        snapshot: CrdtSnapshot,
        journal_ops: Vec<(u64, CollaborativeOp)>,  // (journal_seq, op)
        presence: Vec<UserPresence>,
    },
    Broadcast {
        sender_replica: ReplicaId,
        ops: Vec<(u64, CollaborativeOp)>,  // (journal_seq, op)
    },
    PresenceUpdate {
        changes: Vec<PresenceChange>,
    },
    Error { code: u32, message: String },
    Pong { timestamp: u64 },
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CrdtSnapshot {
    pub journal_seq: u64,       // 快照对应的 journal_seq
    pub replica_count: u16,     // 已分配的 replica 数量
    pub crdt_state: Vec<u8>,    // bincode 序列化的 DocumentCRDT
}
```

### 8.2 同步时序

```
Client                          Server
  │                                │
  ├─ WebSocket connect ───────────>│
  ├─ Auth ─────────────────────────>│
  ├─ JoinDoc ──────────────────────>│
  │                                │
  │<── Welcome (snapshot + ops) ────│  ← 一次性同步历史
  │                                │
  │  ... 持续双向同步 ...             │
  │                                │
  ├─ Sync { ops } ─────────────────>│  ← 本地编辑
  │                                │
  │<── Broadcast { ops } ──────────│  ← 远程编辑
  │                                │
  ├─ Presence { ... } ─────────────>│  ← 选择/光标
  │                                │
  │<── PresenceUpdate { ... } ─────│  ← 其他用户的状态
  │                                │
  ├─ LeaveDoc ─────────────────────>│
  ×                                ×
```

---

## 9. Presence 系统

```rust
/// 用户在场状态
pub struct UserPresence {
    pub user_id: u64,
    pub display_name: String,
    pub avatar_url: Option<String>,
    pub selections: Vec<CollaborativeSelection>,
    pub active_block_id: Option<BlockOpId>,
    pub last_activity: Instant,
    pub color: u32,  // 分配给该用户的远程光标颜色
}

/// 协同选择（用 CRDT anchor 而非绝对偏移）
pub struct CollaborativeSelection {
    pub anchor: CrdtAnchor,
    pub focus: CrdtAnchor,
}

/// Presence 变更
pub enum PresenceChange {
    Joined(UserPresence),
    Left { user_id: u64 },
    Updated(UserPresence),
}
```

### 渲染方案

```
cditor-editor 中新增 overlay 层：
  ├─ RemoteCursors：在文本行上绘制其他用户的彩色光标
  │    - 光标颜色 = user.color
  │    - 光标上方浮动用户名标签
  │
  ├─ RemoteSelections：半透明选择高亮
  │    - 颜色 = user.color 的 20% 透明度
  │
  └─ BlockIndicators：Gutter 区域显示正在编辑该块的用户头像
```

---

## 10. 性能策略

### 10.1 Anchor 索引

文本 CRDT 的 `offset_to_anchor()` 需要高效。使用 Fenwick Tree（Cditor 已有 `BlockHeightIndex` 的思路）构建按 Lamport 排序的 fragment 索引：

```rust
/// Anchor 索引：快速 offset ↔ anchor 转换
struct AnchorIndex {
    /// 按 Lamport 排序的可见 fragment 列表
    fragments: Vec<(OperationId, usize)>, // (id, length)
    /// Fenwick Tree：前缀长度和
    prefix_sums: FenwickTree,
}

impl AnchorIndex {
    /// O(log n)：从绝对偏移找到 CRDT anchor
    fn offset_to_anchor(&self, offset: usize) -> CrdtAnchor { ... }

    /// O(log n)：从 CRDT anchor 找到绝对偏移
    fn anchor_to_offset(&self, anchor: CrdtAnchor) -> usize { ... }
}
```

### 10.2 操作批处理

```rust
impl CollaborationSession {
    /// 将连续的 InsertText 合并为单个操作（50ms 窗口内）
    pub fn flush_outbox(&mut self) -> Vec<CollaborativeOp> {
        let mut ops = std::mem::take(&mut self.outbox);
        ops = Self::coalesce_inserts(ops);
        ops
    }

    fn coalesce_inserts(ops: Vec<CollaborativeOp>) -> Vec<CollaborativeOp> {
        // 连续的 InsertText（同一个 block，同一个 parent_fragment，offset 连续）
        // → 合并为一个 operation
        ...
    }
}
```

### 10.3 Tombstone GC

| 触发条件 | 动作 |
|---|---|
| 快照创建时 | 删除所有 tombstoned 超过 10 分钟的 fragment（所有在线客户端已确认） |
| CRDT 内存 > 10MB | 触发 GC |
| 文档空闲 > 30 秒 | 创建快照 + GC |

### 10.4 大文档分片加载

| 文档规模 | 策略 |
|---|---|
| < 1000 blocks | 完整 CRDT 在内存 |
| 1000-10000 blocks | CRDT 全量 + Payload Window（和现有一致） |
| > 10000 blocks | 按视口分片，只加载视口附近 ±200 blocks 的 CRDT 状态 |

---

## 11. crate 结构与依赖

```
crates/
├── cditor-core/                    # 不变
├── cditor-editor-core/             # 不变
├── cditor-collaboration/           # ★ 新增：CRDT 数据结构 + 转换层
│   ├── Cargo.toml                  # 依赖：cditor-core, serde, bincode
│   ├── src/
│   │   ├── lib.rs
│   │   ├── ids.rs                  # ReplicaId, OperationId, LamportTime
│   │   ├── crdt.rs                 # DocumentCRDT, BlockTreeCRDT, TextContentCRDT
│   │   ├── fragment.rs             # TextFragment, CrdtAnchor, CrdtAffinity
│   │   ├── operation.rs            # CollaborativeOp 枚举
│   │   ├── version_vector.rs       # VersionVector
│   │   ├── undo_map.rs             # UndoMap
│   │   ├── attribute.rs            # LwwRegister, LwwAttributeSet
│   │   ├── deletion.rs             # DeletionRecord
│   │   ├── anchor_index.rs         # AnchorIndex (Fenwick Tree)
│   │   ├── convert.rs              # EditTransaction ↔ CollaborativeOp 转换
│   │   └── serde_impl.rs           # serde/bincode 序列化
│   └── tests/
│       ├── crdt_convergence_tests.rs
│       └── anchor_tests.rs
│
├── cditor-collab-server/           # ★ 新增：协同服务端
│   ├── Cargo.toml                  # 依赖：cditor-collaboration, tokio, sqlx, axum
│   ├── src/
│   │   ├── main.rs
│   │   ├── server.rs               # WebSocket 服务主循环
│   │   ├── session.rs              # 文档会话管理
│   │   ├── journal.rs              # Journal 持久化 + 快照
│   │   ├── auth.rs                 # JWT 认证
│   │   ├── broadcast.rs            # 消息广播
│   │   └── presence.rs             # Presence 追踪
│   └── migrations/
│       └── 0001_collab_journal.sql
│
├── cditor-runtime/                 # ★ 修改：新增协同模式
│   └── src/
│       └── document_runtime/
│           └── collaboration.rs    # CollaborationSession, sync_collaboration_frame
│
├── cditor-editor/                  # ★ 修改：新增远程光标/选择渲染
│   └── src/
│       └── overlay/
│           └── remote_presence.rs  # RemoteCursors overlay
```

**依赖关系：**

```
cditor-core
    ↑
cditor-collaboration  ← 纯 CRDT 数据结构，不依赖 runtime/editor
    ↑
cditor-collab-server  ← 服务端，只依赖 collaboration + tokio + sqlx
    ↑
cditor-runtime        ← 集成 CollaborationSession
    ↑
cditor-editor         ← 渲染远程光标/选择
```

---

## 12. 分阶段实施

| 阶段 | 内容 | 预估工作量 |
|---|---|---|
| **1. CRDT 核心** | DocumentCRDT + BlockTreeCRDT + TextContentCRDT + 序列化 | 2-3 周 |
| **2. 转换层** | EditTransaction → CollaborativeOp 转换 + 反向重建 | 2 周 |
| **3. 服务端** | WebSocket + Journal + 快照 + 认证 | 3-4 周 |
| **4. Runtime 集成** | DocumentMode::Collaborative + sync_collaboration_frame | 2 周 |
| **5. Undo 改造** | CollaborativeUndoManager + undo_map | 1 周 |
| **6. Presence** | 远程光标/选择渲染 | 1-2 周 |
| **7. 端到端测试** | 多客户端并发编辑 + 收敛验证 | 2 周 |
| **8. 生产加固** | Tombstone GC + 断线重连 + 性能优化 | 2-3 周 |

**总计：约 15-20 周（一个季度左右）**

---

## 附录：与 Zed 方案的关键差异

| 维度 | Zed | Cditor |
|---|---|---|
| 内容模型 | 纯文本 | Block 树 + 富文本 + Table + Collection |
| CRDT 层数 | 1 层（文本） | 3 层（结构 + 文本 + 属性） |
| 块排序 | 无（文件就是顺序文本） | OrderKey（Fractional Indexing） |
| 复杂块 | 无 | Table/Collection 用 ReplacePayload + 乐观锁 |
| Undo | Per-user undo map | Per-user undo map（同） |
| 删除 | Tombstone + VersionVector | Tombstone + VersionVector（同） |
| 服务端 | Rust + PG | Rust + PG（同） |
