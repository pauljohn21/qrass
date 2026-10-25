---
trigger: always_on
alwaysApply: true
---

## 🔍 代码查询强制协议 — CodeGraph MCP 优先（违反 = 任务失败）

**核心约束**：任何代码在定位、理解、修改之前，第一个动作必须是 MCP 工具 `codegraph_explore`（server: `codegraph`）。它是 Read-equivalent 的唯一主工具：一次返回相关符号的**带行号原文源码** + 符号间调用路径（含回调、动态派发等 grep 跟不动的跳数）+ 影响面摘要。

### 强制场景

1. **「X 怎么工作 / 架构 / 在哪 / 某区域概览」** → `codegraph_explore`，query 用自然语言问题或符号名堆叠，`projectPath` 带工作区根。
2. **「X 如何流到 Y」** → query 里同时写出跨流程的两端符号名，调用链由工具给出。
3. **改任何符号之前** → query 写出该符号名，取回源码 + 影响面（等价 callers/impact），带着 blast radius 动手。
4. **读一个能命名的文件/符号** → 把文件名或符号名放进 query，不再单独 `Read`；重载名一次返回全部定义体。
5. **不够用** → 带着更具体的符号名再调一次，把上次返回的源码当作已读，禁止重开文件。

### 禁止

- ❌ 用 `SearchCodebase` / `Grep` / `Glob` / `Read` 作为**首轮**代码探索——那是在重做 codegraph 已经做完的索引工作，更贵更不准。
- ❌ 把代码探索委派给 subagent 去跑 grep + read 循环。
- ❌ 拿到 codegraph 结果后再 grep「验证一遍」（AST 全量解析结果即权威）。
- ❌ 手工逐文件拼回一条调用链。
- ❌ 凭直觉猜根因而不先取调用链与影响面。

### 仅以下三种情况回退 `Read` / `Grep`

1. codegraph 不索引的内容：`*.md` 文档、`Cargo.toml` / `*.json` 配置、`sass-spec/` hrx fixture、`element-plus/` 等外部目录。
2. 结果里确实缺某个具体细节行，需要补读。
3. 返回头部的 freshness 告警点名该文件（索引落后磁盘），或报告该 project 未索引（本会话内不再对该 project 调用）。

### 索引与调用纪律

- 批量改动后执行 `codegraph sync` 刷新索引；`cargo check` / `cargo test` **不能替代**这一步，索引过期会让后续查询失真。
- **`projectPath` 在 Qoder 的 MCP schema 里是 required**（CLI 侧才是可选）：每次调用都必须显式带上工作区根绝对路径，否则工具调用直接失败。
- 工作区根 `/Users/panglijun/rust/qrass` 已有 `.codegraph/`；若工具报「无默认项目 / 未索引」，先 `codegraph status` 确认索引存在，而不是改回 grep。
- query 里的符号名用精确名（部分拼错不会被静默替换）；若返回 did-you-mean 提示，按其建议名重查。
- 无 MCP 可用时的 CLI 兜底（同一套语义）：`codegraph explore | node | callers | callees | impact | affected`。
- 改完代码的正确性校验仍归编译器与测试，codegraph 只补结构上下文。
