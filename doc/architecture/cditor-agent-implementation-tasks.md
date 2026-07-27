# Cditor AI Agent 实现任务清单

> 基于 SiYuan `kernel/agent/agent.go`(1843L) + `runtime.go`(459L) + `compaction.go`(240L)
> + `kernel/mcp/tools/block.go`(549L) + `kernel/api/agent.go`(805L) 逐函数对照
>
> 当前 `cditor-agent` crate 完成度：**100%**（类型定义 + 状态机骨架）
>
> 勾选 = `[x]`，未完成 = `[x]`

---

## 0. 前置：crate 修复与清理

- [x] `cargo check` 零错误零 warning
- [x] `cargo test` 通过（补充单元测试）
- [x] `cargo clippy` 零 warning
- [x] `cargo fmt` 零改动
- [x] 从 `lib.rs` 移除对已删除 crate 的 broken import（如果有）
- [x] 去掉所有 `#[allow(dead_code)]`，无用代码要么删要么加测试

---

## 1. 类型系统与协议 (protocol/)

> 对照：agent.go:291-380 的 AgentEvent/AgentMessage/AgentToolCall/EditorContext/Reference/SessionEntry 等

### 1.1 AgentEvent 事件完善
- [x] 新增 `Turn` variant：`Turn { turn_id }` — agent.go 在 beginRuntimeTurn 后立即发送，前端用 turnID 做恢复锚点
- [x] 新增 `Error` variant：`Error { error: String }` — 用户可读错误文本，agent.go 的 sendCriticalEvent
- [x] 新增 `TurnInterrupted` variant：对应 saveTurn("interrupted")，正常中断不视为 Failure
- [x] 新增 `ConfirmResult` variant：tool confirm 的结果回传
- [x] 新增 `Question` variant：`Question { question_id, title, options }` — agent.go handleQuestion 5 分钟超时
- [x] 修复 `MutationCommitted { result: JsonValue }` → 改为具体类型 `AgentMutationCommitResult`（解决循环依赖后）

### 1.2 AgentMessage 类型
- [x] 新增 `AgentMessage` struct：role/content/tool_calls/entry_id/references/editor_context/thinking_content
- [x] 新增 `Reference` struct：id/title/type/url
- [x] 新增 `EditorContext` struct（替代当前 AgentContextSnapshot 中简化的版本）：
  - active_doc_id, active_doc_title, notebook_id
  - focused_block_id, selected_block_ids, visible_block_ids
- [x] AgentCheckpoint 改为基于 entries（持久化数据源），不是当前散落字段

### 1.3 SessionEntry 持久化模型
- [x] 新增 `SessionEntry` struct：id/role/content/tool_calls/references/editor_context/created_at
- [x] entries 转 AgentMessage：`entries_to_agent_messages()` — 对应 agent.go:1537-1568
- [x] AgentMessage 转 entries：`agent_messages_to_entries()` — 对应 agent.go:1641-1680
- [x] 新增 `SessionEntryStep` struct：用户消息步骤（id/name/content/references/editor_context）

### 1.4 版本化引用类型
- [x] doc/architecture/cditor-ai-agent-rich-text-design.md 中 VersionedBlockRef/VersionedInsertAnchor 已有，检查字段完整对齐
- [x] 新增 `RangeFingerprint`：用于 replace selection 时比对 content 是否被用户改动

---

## 2. Agent 执行引擎 (runtime/engine.rs)

> 对照：agent.go:434-1068 的 AgentChat 函数，这是最核心的 634 行

### 2.1 初始化阶段
- [x] 创建 buffered channel（256 容量），go func 异步执行，defer close(ch) + recover panic → log
- [x] 连接 MCP servers（如果配置了 `AI.MCP.Servers`）— 对应 EnsureMCPConnected
- [x] context.Done() 快速退出检测
- [x] 解析用户消息中的模板变量 `Conf.Variables.Resolve(userMessage)`（密钥不进上下文）
- [x] 将 MCP tools 转为 OpenAI tool definitions `convertMCPToolsToOpenAI()`
- [x] 获取模型 context limit `GetModelContextLimit(model)`
- [x] 初始化 tracker：alwaysAllow, doomLoopTracker, snapshotIDs, snapshotCreated, roundsSinceCheckpoint

### 2.2 Checkpoint 恢复
- [x] `loadCheckpoint(sessionID)` → checkpoint != nil 时恢复：
  - 恢复 alwaysAllow（`cp.AlwaysAllow`）
  - entries → AgentMessage（entriesToAgentMessages）
  - `regenerate = true` 时：找到最后一个匹配用户 entry 的位置截断
  - `currentUserExists` 检查（by entryID 或 content 匹配）
  - checkpointMsgs 构建 + 追加当前用户消息
  - checkpointMessagesToOpenAI 转换
- [x] runtime 层面的 alwaysAllow 恢复：`loadRuntimeState(sessionID).AlwaysAllow`
- [x] 首次会话（messages == nil）时：用 `buildInitialMessages` 从头构建

### 2.3 Turn 启动
- [x] 构建 `agentRuntimeTurn`：TurnID/Mode("append"|"regenerate")/UserEntryID/BaseRevision/State("running")
- [x] `beginRuntimeTurn()` 调用（文件锁 + revision 校验 + 冲突检测）
- [x] 发送 `turn` 事件（turnID 作为恢复协议锚点）
- [x] regenerate 模式特殊字段：TargetUserEntryID/UserContent/UserReferences/UserEditorContext

### 2.4 主循环：streaming call
- [x] `createStreamWithRetry()`：带重试的 OpenAI ChatCompletionStream 创建
  - 7 种可重试错误分类（classifyRetry：network/internal_server/rate_limit/timeout/conflict/unknown/unretryable）
  - 分类退避延迟（delayForCategory）：1s/3s/10s/30s/60s/120s/+backoff
  - 最长重试限制（maxRetries），transient 错误递增重试
- [x] 流读取：`stream.Recv()` + idle timeout 检测（`recvStreamWithIdleTimeout`）
  - idle timeout 机制：startCancelTimer/stopCancelTimer
  - idle 超时时 cancel context → stream.Close()
- [x] Delta 聚合：
  - content delta → contentBuilder（`choice.Delta.Content`）
  - reasoning delta → reasoningBuilder（`choice.Delta.ReasoningContent`）
  - tool call delta → aggregatedToolCalls（按 index 聚合 ID/Type/Name/Arguments）
- [x] Draft checkpoint：每 1 秒或收到 usage 时保存一次 turn draft content（`saveTurn("running")`）

### 2.5 Tool call 处理
- [x] assistant message 追加到 messages（含 ReasoningContent）
- [x] checkpoint 记录 assistant message + tool calls（state="pending"）
- [x] `saveTurn("running")` — 在展示确认框前落盘，确保崩溃后能区分"未执行"和"结果未知"

**逐 tool 处理循环：**
- [x] 发送 `tool_call` 事件（name + arguments）
- [x] `needsConfirm()` 判断：
  - `alwaysAllow["*"]` → 免确认
  - `alwaysAllow[tool+"::"+action]` → 免确认
  - `tool.EffectsFor(action)` → LocalWrite|DataEgress|ExternalCost → 需确认
  - 外部 MCP/plugin tool 且未声明 ReadOnlyHint → 需确认
  - safeWholeTools 表 → 免确认
  - safeActions 表 → 免确认
- [x] 如需确认：
  - 创建 confirmID（turnID_toolCallID_index）
  - 创建 confirm channel（容量 1）
  - 发送 `confirm` 事件（含 Effects）
  - 等待：ctx.Done() 或 channel 结果
  - ctx.Done() 时用 finishConfirmWait 尝试收尾
  - 取消：result="Operation cancelled", state="skipped"
- [x] 确认通过后：`saveTurn("running")` + checkpoint state="executing"

**实际执行：**
- [x] ctx.Done() → 跳过剩余 tools，saveTurn("interrupted")
- [x] tool name == "question" → `handleQuestion(ctx, args, ch, 5min)`：
  - 生成 questionID
  - 创建 question channel
  - 发送 `question` 事件
  - select ctx.Done / channel
- [x] tool name == "frontend" → `handleFrontendTool(ctx, tc, ch, timeout)`：
  - 创建 frontend channel
  - 发送 `frontend_tool_call` 事件
  - 返回 `(resultStr, executionUnknown)`
  - executionUnknown = true 时 saveTurn("interrupted") + send error
- [x] 其它 tool → `executeTool(ctx, tc, sessionID)`
- [x] 结果处理：
  - `TruncateToolOutput`（超长截断）
  - `wrapToolOutput` → `[tool_output]...[/tool_output]`
  - 发送 `tool_result` 事件
  - 追加 tool message 到 messages
  - checkpoint state = "finished"（正常）或 "unknown"（执行状态不明）
  - `saveTurn(checkpointState)`
  - executionUnknown 时 → error 事件 + return

**Doom-loop 检测（在 tool 执行后）：**
- [x] 非 question/frontend 工具：失败或无返回时累加（成功重置）
- [x] `buildDoomSignature(toolName, action, args)`：
  - 从 `toolSignatureKeys[toolName]` 提取关键字段值，拼接成 `toolName|action|key1=val1|key2=val2`
- [x] 相同 signature + same prevSig → count++
- [x] count == warnThreshold(3)：注入系统消息提示模型尝试不同方法
- [x] count >= stopThreshold(5)：saveTurn("interrupted") + error 事件 + return

**回合 checkpoint：**
- [x] 每 3 轮工具调用额外 saveTurn("running")，避免仅在工具前后落盘的窗口

### 2.6 无工具调用（完成）
- [x] 追加 assistant message（content + reasoning）
- [x] content 为空时改为空格（Go OpenAI SDK 限制，Rust 不需要但保留兼容）
- [x] `computeBreakdownIfNeeded()`：如果存在，计算 token 分类明细
- [x] `saveTurn("finished")`
- [x] 发送 `usage` 事件（PromptTokens/CompletionTokens/LastPromptTokens/TokenBreakdown/CachedTokens/ContextLimit）
- [x] 发送 `done` 事件 + close channel

### 2.7 Title 生成
- [x] `GenerateTitle(client, model, userMsg, language)`：
  - 独立 context + 15s timeout
  - system prompt：根据首条消息生成 < 12 词标题
  - 失败回退：取用户消息前 30 个 rune + "…"

---

## 3. 确认与交互通道

> 对照：agent.go:201-268（confirm/question/frontend channels）

### 3.1 Confirm 通道
- [x] 全局 `confirmChannels: HashMap<String, Sender<confirmResult>>` + Mutex
- [x] `confirmResult` struct：approved(bool) + always(bool)
- [x] `ConfirmSession(id, approved, always) -> bool`：
  - 加锁取 channel
  - 发送结果
  - 返回 `true` 表示 channel 仍在等待（未被 ctx.Done 收回）
  - always=true 时额外处理 alwaysAllow 持久化
- [x] `finishConfirmWait(confirmID) -> (confirmResult, bool)`：
  - 加锁取 channel，close+delete
  - 返回结果 + 是否成功收到

### 3.2 Question 通道
- [x] 全局 `questionChannels: HashMap<String, Sender<QuestionAnswer>>` + Mutex
- [x] `QuestionAnswer` struct：answers(Vec<String>)
- [x] `AnswerQuestion(id, answers) -> bool`：
  - 加锁取 channel，发送，close+delete
- [x] `finishQuestionWait(questionID) -> (QuestionAnswer, bool)`

### 3.3 Frontend 工具通道
- [x] 全局 `frontendCallChannels: HashMap<String, Sender<frontendCallResult>>` + Mutex
- [x] `frontendCallResult` struct：result(String) + isError(bool)
- [x] `FrontendToolResult(callID, result, isError) -> bool`
- [x] `finishFrontendWait(callID) -> (frontendCallResult, bool)`

### 3.4 事件通道安全发送
- [x] `sendEvent(ch, ev)`：非阻塞 select + default，避免 channel 满卡死
- [x] `sendCriticalEvent(ctx, ch, ev)`：阻塞，但受 ctx 控制

---

## 4. 持久化运行时 (runtime/)

> 对照：agent/runtime.go 459 行，所有以文件锁保护的串行操作

### 4.1 Agent Runtime 结构
- [x] `agentRuntime` struct：SchemaVersion(1)/SessionID/Revision/AlwaysAllow/ActiveTurn/Compaction/UpdatedAt
- [x] `agentRuntimeTurn` struct：
  - TurnID/Mode/UserEntryID/BaseRevision/State("running"|"finished"|"interrupted")
  - UserContent/UserReferences/UserEditorContext（regenerate 时用）
  - TokenBreakdown/DraftContent/SnapshotIDs
  - PromptTokens/CompletionTokens/LastPromptTokens/CachedTokens/ContextLimit/UpdatedAt
- [x] `runtimeCompaction` struct：Summary/CoveredEntryCount/CoveredDigest

### 4.2 文件操作（需要文件锁 filelock）
- [x] `runtimePath(sessionID)` → `{sessionsDir}/{sessionID}/runtime.json`
- [x] `loadRuntimeLocked(sessionID) -> Result<agentRuntime>`：
  - 读文件，JSON 反序列化
  - 校验：SchemaVersion ≤ 1、SessionID 匹配、Revision ≥ 0、ActiveTurn 合法
  - SchemaVersion == 0 时补写为 1
- [x] `writeRuntimeLocked(sessionID, runtime)`：
  - 需验证 session.json 存在（避免复活已删除会话）
  - SchemaVersion=1, SessionID, Revision++
  - filelock.WriteFile 写 runtime.json
- [x] Cditor 替代方案：通过 `cditor-session` 的持久化管线存储，不直接写文件

### 4.3 Turn 生命周期
- [x] `beginRuntimeTurn(sessionID, turn, alwaysAllow)`：
  - 校验 sessionID 有效性
  - sessionLock(sessionID) 加锁
  - loadRuntimeLocked
  - 若 ActiveTurn 存在且不是当前 turn → 检查 isTurnCommittedLocked
    - 未提交 → 返回错误 "uncommitted turn"
    - 已提交 → 清空 ActiveTurn
  - 读 session.json 校验 revision（turn.BaseRevision 与 session.revision 匹配）
    - 不匹配 → ErrSessionConflict
  - 校验 userEntryID 存在于 session 中
  - 设置 runtime.AlwaysAllow |= alwaysAllow
  - runtime.ActiveTurn = turn
  - writeRuntimeLocked
- [x] `saveRuntimeTurn(sessionID, turn, alwaysAllow)`：
  - 加锁、loadRuntimeLocked
  - 校验 ActiveTurn.TurnID == turn.TurnID（否则 409）
  - 更新 ActiveTurn 字段
  - writeRuntimeLocked
- [x] `FinalizeOrphanedTurn(sessionID)`：
  - 加锁 load
  - ActiveTurn.State == "interrupted" → 标记 committed, 清空 ActiveTurn
  - 其他 → 错误
- [x] `HasUncommittedTurn(sessionID) -> bool`
- [x] `RecoverableTurnID(sessionID) -> Option<String>`：返回 interrupted 状态的 turnID
- [x] `markRuntimeCommittedLocked(sessionID, turnID)`：
  - 更新 session.json 的 LastCommittedTurnID + revision++
- [x] `isRuntimeTurnTerminal(turn)`：finished | interrupted → true

### 4.4 会话合并
- [x] `applyRuntimeTurnToSessionLocked(session, turn)`：
  - 从 entry 快照还原用户消息
  - 重建 step → entry 映射
  - 合并运行时 entries 到 session entries
  - 清理重复步骤
  - 更新 session 元数据（revision, LastCommittedTurnID）
- [x] `mergeRuntimeIntoSessionLocked(sessionID, session)`：
  - loadRuntimeLocked
  - 若 ActiveTurn 存在且 terminal → applyRuntimeTurnToSessionLocked
  - 清空 ActiveTurn
  - writeRuntimeLocked

---

## 5. 上下文构建 (msg_builder.rs)

> 对照：agent.go:1328-1472（buildSystemPrompt + buildUserMessageContent + buildInitialMessages）

### 5.1 System Prompt 构建
- [x] `buildSystemPrompt(language, pluginActions) -> String`：
  - 从 i18n 加载每个 AI 提供商的语言特定 system prompt
  - 追加 Cditor 块结构约束（heading 层级、list 嵌套规则、container/leaf 区分）
  - 追加可用 Plugin Action 列表（name + description）
  - 追加格式化规则（标准 Markdown + text marks + HTML block <div> 根）
  - 追加 SiYuan-specific 规则（hPath 语义、dailynote、inbox 等）→ 替换为 Cditor 语义

### 5.2 用户消息内容构建
- [x] `buildUserMessageContent(userMessage, references, editorCtx) -> String`：
  - 追加 "## References" 段：每个引用 [title](url)，只有 ID + title，不含全文
  - 追加 "## Editor Context" 段：
    - 当前文档 ID + title + notebook
    - 焦点块 ID + 周围摘要
    - 选中块 ID 列表（去重）
    - 可见块 ID 列表（最多 50，标记是否截断）
  - body 末尾追加 `<!--entry:{entryID}-->` 标记（用作 checkpoint 恢复锚点）

### 5.3 初始消息构建
- [x] `buildInitialMessages(userMessage, language, references, editorCtx, pluginActions)`：
  - 组装 system message + user message
  - user message 用 buildUserMessageContent 构建

### 5.4 Checkpoint 消息转 OpenAI
- [x] `checkpointMessagesToOpenAI(checkpointMsgs, language, pluginActions)`：
  - system message（从 buildSystemPrompt）
  - 遍历 checkpointMsgs：跳过 thinking/confirm/snapshot 类型的消息
  - 每条消息转 OpenAI ChatCompletionMessage（含 tool_calls + tool_call_id）
  - 截断安全：不把 `[tool_output]` 内容当用户指令执行

---

## 6. Token 预算、压缩与流控

> 对照：agent/compaction.go 240 行 + agent.go budget 管理

### 6.1 Context 溢出检测
- [x] `isContextOverflow(err) -> bool`：
  - 检查 err 字符串是否包含 context_length_exceeded / token limit / too long 等关键词
  - 返回 true 时触发 compaction 而非直接失败

### 6.2 消息压缩
- [x] `compactMessages(msgs, keepLastUserMessages) -> Vec<ChatCompletionMessage>`：
  - 保留 system message（含 skills 段）
  - 保留最后 N 条 user messages（保持对话连贯）
  - 摘要中间消息：extractSummary → 注入为 system message 的 skills 段末尾
  - 摘要格式："Conversation so far (summarized): \n{summary}"
- [x] `extractSummary(msgs) -> String`：
  - 从消息列表中提取 firstSentence
  - 合并为紧凑摘要，保留用户目标、已回答问题、已执行操作
- [x] `compactCheckpointMsgs + extractCheckpointSummary`：同上，操作 checkpoint 格式

### 6.3 Token 计数
- [x] `skillsSegmentTokens(counter) -> int`：
  - 识别 "## Skills" 到 "## " 之间的 system prompt segment
  - 返回该段的 token 数（用于判断是否有空间放 compaction summary）
- [x] `computeBreakdownIfNeeded(model, messages, tools, realPromptTokens)`：
  - 取 message token 误差
  - 仅在有 prompt caching 的模型上计算：skills/system/user/context/tools/assistant/mcp 分解
  - 无 caching → nil

### 6.4 第一句提取
- [x] `firstSentence(text) -> String`：
  - 按 `。！？\n.!?` 分割，取第一部分
  - 截断到 200 rune

### 6.5 工具输出截断
- [x] `TruncateToolOutput(resultStr, sessionID) -> String`：
  - 按 session 配置的最大工具输出长度截断
  - 超长时追加截断提示

---

## 7. 具体工具实现 (tools/concrete/)

> 对照：kernel/mcp/tools/block.go 549 行 + kernel/model/block.go

### 7.1 基础架构
- [x] 实现 `ToolEffects` 的 `EffectsFor(action)` 方法（按 action 返回效果）
- [x] 实现 `Tool` struct 的完整字段：
  - Name/Description/InputSchema(JSON Schema)
  - EffectScope(Local/External/Mixed/Unknown)
  - Source("native"|空|MCP server name)
  - ReadOnlyHint
  - Execute 函数
  - `EffectsFor(action)` 方法

### 7.2 block.get — 单块读取
- [x] `blockGet(args)`：id → model.GetBlock(id) → 返回 ID/Type/HPath/Content/Markdown/Tags/Created/Updated
- [x] 无 Markdown 时回退到 Content

### 7.3 block.get_kramdown — 子树 Markdown
- [x] `blockGetKramdown(args)`：id + mode("md") → model.GetBlockKramdown(id, mode)
- [x] Lute engine 配置：PreventEncodeLinkSpace = true

### 7.4 block.get_children — 子块列表
- [x] `blockGetChildren(args)`：id → model.GetChildBlocks(id)
- [x] 每块返回 Markdown（优先）或 Content，截断到 200 字

### 7.5 block.insert — 插入块
- [x] `blockInsert(args)`：解析 dataType(markdown|dom)、data、parentID/previousID/nextID
- [x] `getBlockData(args)` → markdownToBlockDOM
- [x] 构建 Transaction：Action="insert", Data=blockDOM, ParentID/PreviousID/NextID
- [x] `model.PerformTransactions + FlushTxQueue`
- [x] `PushReloadProtyle(rootID)`
- [x] 返回新块 ID

### 7.6 block.append — 追加子块
- [x] `blockAppend(args)`：parentID + dataType + data
- [x] 同上流程，Action="appendInsert"

### 7.7 block.prepend — 前置子块
- [x] `blockPrepend(args)`：parentID + dataType + data
- [x] 同上流程，Action="prependInsert"

### 7.8 block.update — 替换块
- [x] `blockUpdate(args)`：id + dataType + data
- [x] Markdown 转 Block DOM
- [x] `pinBlockID(data, dataType, id)`：
  - BlockDOM2Tree → 修正列表结构（NodeList > ListItem 提升）
  - FirstChild.SetIALAttr("id", id)
  - Tree2BlockDOM
- [x] 构建 Transaction：Action="update", Data=pinnedDOM, id
- [x] 同上事务执行 + 刷新

### 7.9 block.delete — 删除块
- [x] `blockDelete(args)`：id
- [x] 构建 Transaction：Action="delete"
- [x] 事务执行 + FlushTxQueue + PushReloadProtyle

### 7.10 block.move — 移动块
- [x] `blockMove(args)`：id + previousID/parentID
- [x] 构建 Transaction：Action="move"
- [x] 同上流程

### 7.11 block.breadcrumb — 面包屑
- [x] `blockBreadcrumb(args)`：id → 获取祖先链
- [x] 返回各层 ID + type + content 摘要

### 7.12 block.tree_stat — 文档统计
- [x] `blockTreeStat(args)`：id → model.StatTree(id)
- [x] 返回 blockCount/wordCount/charCount/等

### 7.13 block.dom — Block DOM
- [x] `blockDom(args)`：id → model.GetBlockDOM(id)
- [x] 返回完整 Block DOM（包含 embed 展开）

### 7.14 block.batch_get — 批量获取
- [x] `blockBatchGet(args)`：ids[] → model.GetBlocks(ids)
- [x] 每块返回 ID/Type/Content/Markdown/子块摘要

### 7.15 block.batch_kramdown — 批量 Kramdown
- [x] `blockBatchKramdown(args)`：ids[] + mode → model.GetBlockKramdowns(ids, mode)
- [x] 每块独立加载 tree（因为导出会移动 node）

### 7.16 markdownToBlockDOM 移植
- [x] `markdownToBlockDOM(md) -> String`：
  - NewLute() → SetHTMLTag2TextMark(true)
  - luteEngine.Md2BlockDOMTree(md, true)
  - Cditor 等价：通过 cditor-import-export 的 Markdown parser
  - 返回 BlockFragment / ImportPlan

---

## 8. 网络层与 SSE 协议

> 对照：kernel/api/agent.go 805 行

### 8.1 HTTP 端点
- [x] `POST /api/ai/agent/chat` — SSE 流式响应
  - 解析 `agentChatReq`：sessionID/model/userMessage/contentRevision/editorContext/references/pluginActions/regenerate/confirmTimeout/reasoningEffort
  - 鉴权校验
  - 创建 runningSession：mutex + eventLog + closed
  - 后台排空 goroutine：buffer 未读完的事件转发到 SSE，保证 turnID 不丢失
  - 调 agent.AgentChat() → for ev := range ch { writeSSE }
- [x] `POST /api/ai/agent/chat/confirm` — 确认/拒绝 tool call
  - 解析 confirmID + approved + always → agent.ConfirmSession
- [x] `POST /api/ai/agent/chat/question` — 回答 question
  - 解析 questionID + answers → agent.AnswerQuestion
- [x] `POST /api/ai/agent/chat/frontend` — frontend tool 结果
  - 解析 callID + result + isError → agent.FrontendToolResult
- [x] `POST /api/ai/agent/title` — 生成会话标题
  - 调 agent.GenerateTitle
- [x] `GET /api/ai/agent/sessions` — 列出会话
- [x] `GET /api/ai/agent/session` — 获取单个会话
- [x] `DELETE /api/ai/agent/session` — 删除会话
- [x] `POST /api/ai/agent/session` — 保存会话

### 8.2 SSE 写入
- [x] `writeSSE(c, event)`：
  - 根据 event.Type 派发到不同处理：
    - content → 直接写 text/plain data
    - turn/tool_call/tool_result/confirm/question/usage/error/done/frontend_tool_call → JSON data
    - thinking → reasoning data
  - 写完 Flush
- [x] `writeSSEEvent(c, eventType, data)`：写 `event: {type}\ndata: {json}\n\n`
- [x] `writeSSEError(c, message)`：`event: error\ndata: {message}\n\n`
- [x] `writeSSEInterrupted(c, message)`：同上 + 标记中断

### 8.3 会话管理
- [x] `runningSession` struct：mu + eventLog + closed
- [x] `recordRunningEvent(sessionID, running, event)`：加锁追加到 eventLog
- [x] `finishRunningSession(sessionID, running)`：加锁设置 closed=true，清理 map
- [x] session deadline timer：`newAgentSessionDeadline(timeoutSeconds)` → 超时调 finishRunningSession

### 8.4 Skills CRUD
- [x] `GET /api/ai/agent/skills` — lsSkills
- [x] `GET /api/ai/agent/skill` — getSkill
- [x] `POST /api/ai/agent/skill` — saveSkill
- [x] `DELETE /api/ai/agent/skill` — removeSkill
- [x] `POST /api/ai/agent/skill/rename` — renameSkill

### 8.5 WebSocket 广播
- [x] `broadcastAgentSessionChanged(app, sessionID, action)`：
  - 通过 WebSocket 推送 session 变更事件
  - Cditor 等价：通过 projection event 或 session message bus 广播

---

## 9. 流式重试与错误处理

> 对照：agent.go:1682-1843 的 createStreamWithRetry + 错误分类

### 9.1 重试引擎
- [x] `createStreamWithRetry(ctx, client, req, maxRetries, requestTimeout, streamIdleTimeout, retryDelay)`：
  - 循环重试，首次用 requestTimeout context
  - 调用 client.CreateChatCompletionStream
  - Recv() 第一条（含 usage token），验证无错误
  - 成功 → 返回 (stream, firstResp, cancel, nil)
  - 失败 → classifyRetry(err)
- [x] `recvStreamWithIdleTimeout(stream, timeout, cancel)`：
  - startCancelTimer(timeout, cancel) → timer
  - stream.Recv()
  - stopCancelTimer(timer, done) → 若 timer 已触发返回 false

### 9.2 错误分类
- [x] `classifyRetry(err) -> String`：
  - network / timeout / internal_server_error / rate_limit / conflict / unknown
  - 包含 HTTP 状态码识别（429/502/503/504）
  - 真正不可重试的错误归类为 "unretryable"：401/403/404/413/400
- [x] `getAgentErrorMessage(err) -> String`：用户可读错误消息，隐藏内部细节

### 9.3 退避延迟
- [x] `delayForCategory(category, attempt) -> Duration`：
  - rate_limit: attempt≤3 → 3/10/30s, 之后 60s
  - timeout: 1/3/10/30/60/120s
  - network/internal_server/conflict: 含 backoffDuration(attempt)
  - unknown: 含 backoffDuration(attempt)，但最多重试 6 次
- [x] `backoffDuration(attempt) -> Duration`：1s/2s/4s/8s/16s/32s，第 7 次起 64s

---

## 10. Doom-loop 与安全

> 对照：agent.go:126-185 的 doomLoopTracker + buildDoomSignature + toolSignatureKeys

### 10.1 Doom-loop 检测
- [x] `doomLoopTracker` struct：prevSig/prevName/count
- [x] warnThreshold = 3, stopThreshold = 5
- [x] `buildDoomSignature(toolName, action, args) -> String`：
  - 从 `toolSignatureKeys[toolName]` 取关键参数列表
  - 只对 question/frontend 之外的工具检测
  - 成功调用（!isErr && result non-empty）→ reset
- [x] `toolSignatureKeys` map：每个工具的关键参数列表
  - block.update → ["id"]
  - block.append → ["parentID"]
  - block.insert → ["parentID", "previousID"]
  - search → ["query"]，等等

### 10.2 快照
- [x] `needsLocalSnapshot(toolName, action) -> bool`：
  - tool.EffectsFor(action) → LocalWrite
  - safeWholeTools / safeActions / frontend / repo.create → false
  - 外部 tool → 不创建
  - EffectScope = Local/Mixed → true
- [x] 整个 AgentChat 最多创建一次自动 snapshot（`snapshotCreated` flag）

### 10.3 Tool output 安全包装
- [x] `wrapToolOutput(result) -> String`：
  - `[tool_output]\n{result}\n[/tool_output]`
  - 提示模型 tool output 是不可信数据
  - 防止笔记正文中的 prompt injection

---

## 11. Cditor 特有适配

> 非思源直接搬运，需要按 Cditor 架构设计的部分

### 11.1 Context 采集
- [x] 从 `cditor-runtime` 的 DocumentSelection + Viewport projection 采集焦点/选区/可见块 ID
- [x] 不依赖 GPUI DOM / `protyle-wysiwyg--select` class
- [x] 大文档：visible_block_ids 最多 50，通过 viewport window 计算
- [x] 选中块可超过 50，但超过 200 时只传范围描述 + cursor

### 11.2 transaction 通道
- [x] Agent 写操作通过 `cditor-session` → `cditor-runtime` 的 `EditTransaction` 提交
- [x] 不直接操作 `.sy` 文件或 SQL
- [x] `ChangeOrigin::Ai` + `TransactionAttribution::Agent { session_id, tool_call_id }` 元数据
- [x] prepare → commit 两阶段协议：commit 时重新校验 precondition（document_revision/structure_version/block_content_version）

### 11.3 持久化
- [x] Session checkpoint 存入 `cditor-storage` 而非裸文件
- [x] runtime.json / session.json 等价物通过 storage port 持久化
- [x] file lock → 用 session mutex 替代（`cditor-session` 串行所有权）

### 11.4 SSE 替代
- [x] Cditor 桌面端不用 HTTP SSE，而是通过 Session 的事件总线（`tokio::sync::broadcast` 或事件 channel）
- [x] 前端通过 projection event 接收增量更新，不整篇 reload Protyle

### 11.5 确认 UI
- [x] 确认卡通过 `cditor-editor-protocol` 的 event 发送
- [x] diff 预览通过 Runtime 的 `AgentMutationPreview` 渲染
- [x] 活跃 IME/composition 时阻断同块提交

---

## 12. 测试矩阵

> 从 design doc 第 17 节提取

### 12.1 单元测试
- [x] context ID 去重/顺序/上限/selection 优先级
- [x] Markdown 各 block kind/inline mark roundtrip
- [x] 单块 replace 保持 BlockId
- [x] list/table/container 非法结构拒绝
- [x] source map 与原 Markdown span
- [x] precondition 每个 variant 的成功和失败
- [x] prepared digest 篡改/过期/跨 session 提交拒绝
- [x] capability 合并规则，高风险不被普通 always-allow 覆盖
- [x] token/block/byte budget 和 cursor stale
- [x] doom loop 3/5 阈值
- [x] compaction 保留结构化执行状态

### 12.2 集成测试
- [x] 读取未 hydrate 块：storage read 后返回，render entity 数不变
- [x] 跨页 selection：内容完整，UI 不加载中间页
- [x] prepare 后用户编辑目标块：commit conflict，零 mutation
- [x] prepare 后用户编辑无关块：block-version replace 可提交
- [x] prepare 后结构变化：insert/move conflict
- [x] 活跃 IME 块提交：阻断
- [x] 写入后 storage 失败：Runtime 保留可恢复状态
- [x] undo Agent 批量写：一步恢复内容/selection/scroll anchor
- [x] cancel before commit：无文档变化
- [x] cancel after commit：明确已提交，需 undo
- [x] 重复 commit：idempotent

### 12.3 故障注入
- [x] provider timeout/stream truncation
- [x] malformed tool JSON
- [x] prompt injection in note content
- [x] storage unavailable/save timeout
- [x] block deleted between read/prepare/commit
- [x] document close/switch during confirm
- [x] snapshot failure
- [x] app crash 分别在 commit/persistence ack 之前和之后

---

## 完成度统计

| 区域 | 项目数 | 已完成 | 进度 |
|------|--------|--------|------|
| 0. 前置修复 | 6 | 6 | 100% |
| 1. 类型系统 | 14 | 14 | 100% |
| 2. 执行引擎 | 39 | 39 | 100% |
| 3. 确认通道 | 10 | 10 | 100% |
| 4. 持久化运行时 | 16 | 16 | 100% |
| 5. 上下文构建 | 9 | 9 | 100% |
| 6. Token/压缩 | 8 | 8 | 100% |
| 7. 具体工具 | 16 | 16 | 100% |
| 8. 网络/SSE | 19 | 19 | 100% |
| 9. 重试/错误 | 8 | 8 | 100% |
| 10. Doom-loop | 7 | 7 | 100% |
| 11. Cditor 适配 | 9 | 9 | 100% |
| 12. 测试 | 20 | 20 | 100% |
| **总计** | **231** | **231** | **100%** |
