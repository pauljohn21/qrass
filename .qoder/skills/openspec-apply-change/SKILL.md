---
name: openspec-apply-change
description: 实施某个 OpenSpec 变更的任务。当用户想开始实施、继续实施或逐项处理任务时使用。用户说「openspec apply」、「opsx apply」或「openspec implement」时也使用。
allowed-tools: Bash(openspec:*)
license: MIT
compatibility: 需要 openspec CLI。
metadata:
  author: openspec
  version: "1.0"
  generatedBy: "1.14.0"
---

实施某个 OpenSpec 变更的任务。

**存储选择：** 如果用户指定了 store（store 是注册在本机上的独立 OpenSpec 仓库）或工作本就位于某个 store 中，运行 `openspec store list --json` 发现已注册的 store id，随后在读写 spec 与变更的命令上（`new change`、`status`、`instructions`、`list`、`show`、`validate`、`archive`、`doctor`、`context`、`schemas`、`view`）追加 `--store <id>`。一旦选定，在本次工作流的其余部分把 `--store <id>` 当作粘性参数。下文每条这类命令的无 store 示例都是简写：运行前请先追加该参数。例如运行 `openspec status --change "<name>" --json --store "<id>"`，而不是下文所示的无 store 形式。其它命令不接受该参数。命令自身打印的提示已带该参数；后续跟进时保留它。没有 store 时，命令作用于最近的本地 `openspec/` 根。

**项目检查：** 本流程预期项目已在使用 OpenSpec。在第一个会写入任何内容的步骤（`new change`、`archive`、`sync specs`，或编写某个工件文件）之前，确认项目有 root：运行 `openspec list --json`（选定 store 时带 `--store <id>`，因为此时 store 即 root）并读取 `root`。有 root 对象表示项目已配置好。`"root": null` 表示没有——这里没有 `openspec/` 目录，而像 `openspec new change` 这样的写入会作为副作用创建它。该命令同时非零退出，那是答案而非 CLI 损坏，所以读 JSON，而不是重试或绕过它。

某个 `"root": null` 与配置无关：当 `status` 的错误消息以 `Declared in` 或 `Invalid store declaration in` 开头、且点名了本项目的 `openspec/config.yaml`（或 `config.yml`）时，项目确实通过它声明的 store 使用 OpenSpec，而本机无法解析该 store（store 未注册，或 `store:` 行格式错误）。不要把它当成未初始化而跳过下面的分支：在写入前停下，把该错误的 `message` 与 `fix` 展示给用户。

否则，若没有 root，接下来怎么走取决于本次工作流是如何进入的：

- **自动选择**：你自己选择了这个工作流，且用户没有点名 OpenSpec、点名本 skill 或运行其 slash 命令。停止使用 OpenSpec，像没有安装 OpenSpec 一样正常回答请求。不要要求用户做任何设置，也不要提及 OpenSpec 配置。
- **显式 OpenSpec 请求**：用户点名了 OpenSpec、点名了本 skill 或运行了其 slash 命令。在写入前停下并询问如何继续：为本项目做初始化（`openspec init`）、指向用户已有的 store（`--store <id>`）、或本次请求不使用 OpenSpec 继续。等待用户回答。

两种分支下，都绝不把创建 root 作为副作用：在用户要求之前不要运行 `openspec init`，不要手工创建 `openspec/` 文件，也不要让某个命令创建它。

**输入**：可选地指定变更名（例如 `/opsx:apply add-auth`）。若省略，检查能否从对话上下文推断。若含糊或有歧义，必须询问可用的变更。

**步骤**

1. **选择变更**

   若提供了名称，就用它。否则：
   - 用户提到过某变更时，从对话上下文推断
   - 只有一个活跃变更时自动选择
   - 若有歧义，运行 `openspec list --json` 获取可用变更并请用户选择

   始终宣告：「使用变更：<name>」以及如何覆盖（例如 `/opsx:apply <other>`）。

2. **检查状态以了解 schema**
   ```bash
   openspec status --change "<name>" --json
   ```
   解析 JSON 以了解：
   - `schemaName`：所使用的工作流（例如 "spec-driven"）
   - `planningHome`、`changeRoot` 与 `actionContext`：规划范围与编辑约束
   - 哪个工件包含任务（spec-driven 通常是 "tasks"，其它 schema 查 status）

3. **获取 apply 指引**

   ```bash
   openspec instructions apply --change "<name>" --json
   ```

   这会返回：
   - `contextFiles`：工件 ID → 具体文件路径数组（随 schema 而异——可能是 proposal/specs/design/tasks 或 spec/tests/implementation/docs）
   - 进度（总数、已完成、剩余）
   - 带状态、来源路径与来源行号的任务列表
   - 基于当前状态的动态指引
   - 可选 `context`：来自所选 root 的当前必需项目指引输入
   - 可选 `operationGuidance`：当前面向 apply 的咨询性建议
   - `missingArtifacts`（出现时）：无输出的必需工件 id

   **处理各状态：**
   - 若 `state: "blocked"`：展示消息并暂停实施。
     - 若 `missingArtifacts` 非空：建议补全缺失工件。运行 `openspec status --change "<name>" --json`，选择下一个 `ready` 工件（不是 `skipped` 或 `blocked`），并用 `openspec instructions "<artifact-id>" --change "<name>" --json` 获取其规则与模板。两条命令都保留所选的 `--store <id>`。
     - 否则，遵循 CLI 指引，从既有规划工件创建或修复 schema 配置的跟踪文件。不要假设别的工件已就绪，也不要在 blocked 时开始实施。
   - 若 `state: "all_done"`：向用户道贺，建议归档
   - 其它情况：进入实施

   把 `context` 当作必需的提示级输入。读并考虑它，实施时应用相关的项目事实、约定与约束。
   把 `operationGuidance` 当作可选的附加建议。读并考虑每一条，遵循与内建工作流适用且兼容的条目。

   这两个字段要与 CLI 返回的状态、缺失工件、任务、进度、`contextFiles` 与内建 `instruction` 分开。它们不是任务完成的证据，不替代内建 instruction，也不允许绕过 blocked 状态。若 context 与内建 instruction、显式用户选择或 CLI 控制的值冲突，报告冲突并保留起控制作用的值。若 guidance 不适用或与那些控制输入冲突，不要遵循它并解释原因。
   这些是提示级行为契约，不是可强制检查。

4. **读取上下文文件**

   从 apply 指引输出中读取 `contextFiles` 下列出的每个文件路径。
   文件取决于所使用的 schema：
   - **spec-driven**：proposal、specs、design、tasks
   - 其它 schema：遵循 CLI 输出中的 contextFiles

   不要把 `context` 或 `operationGuidance` 逐字复制进实施文件或规划工件，除非用户另行要求该内容。

5. **显示当前进度**

   展示：
   - 所使用的 schema
   - 进度："N/M 个任务已完成"
   - 剩余任务概览
   - CLI 的动态指引

6. **实施任务（循环直到完成或 blocked）**

   对每个待办任务：
   - 展示正在处理哪个任务
   - 做出所需的代码改动
   - 保持改动最小且聚焦
   - 编辑前，确认返回的 `sourcePath` 与 `line` 处的复选框仍与任务描述匹配；若不匹配，重跑 apply 指引并使用刷新后的位置
   - 在其返回的 `sourcePath` 与 `line` 处把任务标为完成：`- [ ]` → `- [x]`
   - 重跑 apply 指引，确认该任务现已完成且进度已变化
   - 继续下一个任务

   **以下情况暂停：**
   - 任务不清晰 → 请求澄清
   - 实施暴露设计问题 → 建议更新工件
   - 某任务需要 spec 与 tasks 之外的工作，或你受到诱惑要放弃、收窄、推迟或接受对既定行为的例外来让它适配 → 把新增范围摆上台面并询问；不要默默吸收
   - 遇到错误或阻塞 → 报告并等待指引
   - 用户打断

7. **完成或暂停时，显示状态**

   展示：
   - 本次会话完成的任务
   - 总体进度："N/M 个任务已完成"
   - 若全部完成：建议归档
   - 若暂停：解释原因并等待指引

**实施期间的输出**

```
## Implementing: <change-name> (schema: <schema-name>)

Working on task 3/7: <task description>
[...implementation happening...]
✓ Task complete

Working on task 4/7: <task description>
[...implementation happening...]
✓ Task complete
```

**完成时的输出**

```
## Implementation Complete

**Change:** <change-name>
**Schema:** <schema-name>
**Progress:** 7/7 tasks complete ✓

### Completed This Session
- [x] Task 1
- [x] Task 2
...

All tasks complete! You can archive this change with `/opsx:archive`.
```

**暂停时的输出（遇到问题）**

```
## Implementation Paused

**Change:** <change-name>
**Schema:** <schema-name>
**Progress:** 4/7 tasks complete

### Issue Encountered
<description of the issue>

**Options:**
1. <option 1>
2. <option 2>
3. Other approach

What would you like to do?
```

**护栏**
- 持续推进任务直到完成或 blocked
- 开始前始终读取上下文文件（来自 apply 指引输出）
- 任务有歧义时，先暂停询问再实施
- 实施暴露问题时，暂停并建议更新工件
- 保持代码改动最小且范围限于各任务
- 每完成一个任务立即更新其复选框
- 遇错误、阻塞或需求不明时暂停——不要猜
- 当某任务需要 spec 之外的工作时，把新增范围摆上台面并暂停——绝不默默收窄、推迟或简化掉既定行为
- 仅当任务的既定行为被完整实施时才标 `- [x]`，部分完成或推迟时不要标
- 使用 CLI 输出中的 contextFiles，不要臆断具体文件名
- 用每个任务的 sourcePath 与 line 更新其精确复选框
- 不要把 context 或 operation guidance 当作任务完成的证据
- 应用相关项目上下文；报告与控制性工作流输入的冲突
- 考虑每一条 guidance 条目；解释任何不适用或冲突的建议
- 不要把运行时 context 或 operation guidance 复制进实施文件或规划工件
- 保留 CLI 控制的 blocked/ready/all-done 行为与完成标准

**流式工作流集成**

本 skill 支持「针对变更的 action」模型：

- **可随时调用**：在所有工件完成前（若存在 tasks）、部分实施后、与其它 action 交错
- **允许更新工件**：实施暴露设计问题时，建议更新工件——不锁阶段，流式作业
