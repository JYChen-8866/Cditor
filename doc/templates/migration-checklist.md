# Migration Checklist：<迁移名称>

> 日期：YYYY-MM-DD
>
> 关联 ADR：ADR-NNN
>
> 涉及 schema 版本：document / block payload / operation / clipboard / plugin
> manifest / SQLite / PostgreSQL 中受影响的版本号（旧 -> 新）

按总设计第 6.3、18.4 节：破坏性升级必须先备份、dry-run、校验，再原子切换；
降级无法安全写时进入只读兼容模式；未知 Block、mark、property、字段和插件
payload 必须保存 raw envelope。

## 1. 迁移前

- [ ] 列出受影响的存储对象（表、文件、payload kind）和当前版本分布。
- [ ] 明确 reader/writer 兼容窗口：旧 reader 读新数据、新 reader 读旧数据各自行为。
- [ ] 备份完成并验证可恢复（不是只验证备份文件存在）。
- [ ] dry-run 在 production-like 匿名 fixture 上执行，输出报告（条数、耗时、警告、失败样本）。
- [ ] dry-run 前后语义 checksum 一致；unknown payload 字节不变。
- [ ] 回滚脚本/程序已编写并在 dry-run 数据上演练。
- [ ] 迁移在中途 kill -9 后可安全重入（幂等或断点续跑），有 fault-injection 证据。

## 2. 迁移执行

- [ ] 迁移在独立事务/journal 中执行，失败时不留半成品可见状态。
- [ ] 进度可观测（当前阶段、已处理量、预计剩余），可取消且取消后状态明确。
- [ ] 迁移期间写入策略明确：阻塞写 / 双写 / 只读，与用户可见状态一致。

## 3. 迁移后

- [ ] 新旧数据语义 checksum 对比通过，抽样人工核对通过。
- [ ] unknown kind/field/plugin fixture round-trip 100% 通过。
- [ ] 全量自动化（fmt/check/clippy/test）在迁移后代码上通过。
- [ ] 旧版本客户端打开新数据的行为符合预期（只读模式或明确错误 UI）。
- [ ] 回滚演练：迁移后立刻回滚一次，checksum 与迁移前一致。
- [ ] 旧写路径按保留窗口计划标记废弃日期，未到期不删除。

## 4. 记录

| 项 | 值 |
|---|---|
| 执行环境（硬件/OS/DB 版本） | |
| 数据规模 | |
| dry-run 耗时 / 正式耗时 | |
| 失败与处理 | |
| checksum 证据位置 | |
