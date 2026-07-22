# ADR-007：Editor Session Protocol 作为稳定应用边界

> 状态：Accepted
>
> 日期：2026-07-22
>
> 关联章节：`doc/architecture/重构方案 0722.md` 第 3、6、7.2 节
>
> 关联任务：R2-001 ~ R2-006、R3-001 ~ R3-006

## 1. 背景

Command DTO 和 catalog 原先位于 viewport 算法 crate，SDK 又定义了第二套
`CditorCommand`、state 和 outcome。Runtime projection 类型则由 Runtime 直接公开给 GPUI。
这使 viewport 承担应用协议、SDK 成为内部 DTO 来源，并让 UI 编译期依赖 Runtime 的公开
状态形状。Session、headless host 和未来 UI adapter 缺少共同的稳定边界。

## 2. 候选方案

### 候选 A：协议继续分散在现有 crate

改动较少，但 viewport、SDK、Runtime 和 Editor 会继续互相泄漏类型；Cargo 无法强制 UI
不读取 Runtime 内部状态，也无法独立测试协议兼容性。否决。

### 候选 B：协议并入 Core

依赖最少，但 Command、UI projection、host capability 和 session event 是应用语义，不是文档
领域语义。放入 Core 会迫使领域层随宿主和呈现协议演进。否决。

### 候选 C：独立 `cditor-editor-protocol`

建立只依赖 Core 和必要序列化支持的无状态协议 crate。Runtime、Session、Editor、SDK 和
headless 测试宿主共同消费它；具体 UI、存储、网络和文本引擎类型不得进入公共面。采用。

## 3. 决定

- crate owner：编辑器应用架构层；任何公共 DTO 变更由 Runtime、Session、Editor 和 SDK
  共同审查。
- 稳定 API：版本化 Command/Query/Event/Projection/Capability/Error DTO；协议类型不执行
  命令、不持有文档状态、不启动任务。
- 允许依赖：`cditor-core`、`serde`/`serde_json` 等协议编解码支持。
- 禁止依赖：Runtime、Viewport、Storage、AI、Import/Export、SDK、GPUI、Parley、SQLx、
  reqwest 和具体 adapter。
- 线程模型：纯 owned value，禁止 `Entity`、executor、锁、task handle 和线程亲和对象；DTO
  在需要时可 `Send + Sync`，但同步热路径不因协议边界增加异步或序列化。
- 真实消费者：`cditor-api` 与 `cditor-editor` 已消费 Command；迁移完成后 Runtime、Session、
  SDK 和测试宿主直接消费完整协议。

协议是进程内应用边界，不等同于 operation journal、clipboard、持久化 schema 或未来网络
协议。只有确需跨进程或持久化的 DTO 才承诺 serde wire 兼容。

## 4. 代价

新增 crate 会增加少量编译图和 DTO 转换成本。Projection 不能直接复用 Runtime/Viewport
内部类型，需要维护 bounded read model 和显式转换；这是阻止 UI 依赖真相实现细节的必要
成本。协议演进还需要 compatibility tests 和版本纪律。

## 5. 迁移

1. 迁移 viewport 中 Command DTO/catalog，API 和 Editor 改用 Protocol。
2. 定义 Query/Event/Projection/Capability/Error，Runtime 负责构造，Editor 只消费。
3. 删除 API 和 Runtime 的重复 state/outcome/projection 类型及临时 re-export。
4. Session 建立后，所有 dispatch/query/projection/event 通过 Protocol 边界。
5. R9 删除兼容 façade，并以 dependency gate 固定最终拓扑。

## 6. 回滚

在 SDK semver 和持久化 wire schema 对外承诺前，可将 DTO 合回消费者并删除 crate；Core 文档
数据不受影响。对外承诺后只允许以新增版本和兼容转换演进，不回收已经发布的字段语义。

## 7. 测试

- Command ID、catalog 唯一性、参数匹配、schema 版本、serde round-trip 和 unknown command。
- Query state matrix、event 顺序事实、projection boundedness 和 capability default-deny。
- `cargo check --workspace --all-targets` 验证所有消费者。
- `check_structure.sh` 禁止 Protocol 引入 Runtime/UI/Storage/网络/Parley 依赖。
- 输入和 100k Block benchmark 确认协议边界未引入序列化或异步热路径。

## 8. 需要更新的文档章节

| 文档 | 章节 | 更新内容 |
|---|---|---|
| `重构方案 0722.md` | 第 6、7.2、10 节 | crate 拓扑、公共面和迁移清单 |
| `project-structure.md` | 当前迁移态 | 登记 Protocol 与禁止依赖 |
| `cditor-mature-notion-editor-master-design.md` | Command/Projection/Session 相关章节 | 后续以 Protocol 类型统一术语 |
