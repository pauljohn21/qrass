---
name: openspec-archive-change
description: 在 experimental 工作流中归档一个已完成的 OpenSpec 变更。当用户想在实现完成后定稿并归档某个变更时使用。用户说「openspec archive」或「opsx archive」时也使用。
allowed-tools: Bash(openspec:*)
license: MIT
compatibility: 需要 openspec CLI。
metadata:
  author: openspec
  version: "1.0"
  generatedBy: "1.14.0"
---

在 experimental 工作流中归档一个已完成的变更。

**存储选择：** 如果用户指定了 store（store 是注册在本机上的独立 OpenSpec 仓库）或工作本就位于某个 store 中，运行 `openspec store list --json` 发现已注册的 store id，随后在读写 spec 与变更的命令上（`new change`、`status`、`instructions`、`list`、`show`、`validate`、`archive`、`doctor`、`context`、`schemas`、`view`）追加 `--store <id>`。一旦选定，在本次工作流的其余部分把 `--store <id>` 当作粘性参数。下文每条这类命令的无 store 示例都是简写：运行前请先追加该参数。例如运行 `openspec status --change "<name>" --json --store "<id>"`，而不是下文所示的无 store 形式。其它命令不接受该参数。命令自身打印的提示已带该参数；后续跟进时保留它。没有 store 时，命令作用于最近的本地 `openspec/` 根。

**项目检查：** 本流程预期项目已在使用 OpenSpec。在第一个会写入任何内容的步骤（`new change`、`archive`、`sync specs`，或编写某个工件文件）之前，确认项目有 root：运行 `openspec list --json`（选定 store 时带 `--store <id>`，因为此时 store 即 root）并读取 `root`。有 root 对象表示项目已配置好。`"root": null` 表示没有——这里没有 `openspec/` 目录，而像 `openspec new change` 这样的写入会作为副作用创建它。该命令同时非零退出，那是答案而非 CLI 损坏，所以读 JSON，而不是重试或绕过它。

某个 `"root": null` 与配置无关：当 `status` 的错误消息以 `Declared in` 或 `Invalid store declaration in` 开头、且点名了本项目的 `openspec/config.yaml`（或 `config.yml`）时，项目确实通过它声明的 store 使用 OpenSpec，而本机无法解析该 store（store 未注册，或 `store:` 行格式错误）。不要把它当成未初始化而跳过下面的分支：在写入前停下，把该错误的 `message` 与 `fix` 展示给用户。

否则，若没有 root，接下来怎么走取决于本次工作流是如何进入的：

- **自动选择**：你自己选择了这个工作流，且用户没有点名 OpenSpec、点名本 skill 或运行其 slash 命令。停止使用 OpenSpec，像没有安装 OpenSpec 一样正常回答请求。不要要求用户做任何设置，也不要提及 OpenSpec 配置。
- **显式 OpenSpec 请求**：用户点名了 OpenSpec、点名了本 skill 或运行了其 slash 命令。在写入前停下并询问如何继续：为本项目做初始化（`openspec init`）、指向用户已有的 store（`--store <id>`）、或本次请求不使用 OpenSpec 继续。等待用户回答。

两种分支下，都绝不把创建 root 作为副作用：在用户要求之前不要运行 `openspec init`，不要手工创建 `openspec/` 文件，也不要让某个命令创建它。

`<capability-path>` 是相对 `specs/` 的 spec 目录（例如 `user-auth` 或 `identity/user-auth`）。解析其主 spec 时，保留每个 delta spec 的完整路径。

**输入**：可选指定一个变更名。若省略，检查能否从对话上下文推断。若含糊或有歧义，你必须列出现有变更并追问。

**步骤**

1. **选择变更**

   若提供了名称，就用它。否则：
   - 若用户提到过某个变更，从对话上下文推断
   - 若只有一个活动变更，自动选中它
   - 若有歧义，运行 `openspec list --json` 获取可用变更并请用户选择

   提示时，只展示活动变更（不含已归档的）。
   若有，附上每个变更所用的 schema。

   始终宣告：「使用变更：<name>」，并说明如何覆盖（例如 `/opsx:archive <other>`）。

   **在既有归档检查之前加载当前归档输入：**

   解析出选定的变更与 planning root 之后，运行：
   ```bash
   openspec instructions archive --change "<name>" --json
   ```
   该命令保留相同的选定 root 参数。此查询是建议性的、可选的：它只提供额外的提示输入，因此绝不能阻塞归档。
   若它非零退出或返回无效 JSON——例如较旧的 CLI 尚不支持该命令——不带任何上下文、不带任何操作指引继续归档流程。不要报告错误，也不要停下。

   成功响应可能省略两个可选字段。把 `context` 当作必需的提示级输入：读取并考虑它，应用相关的项目事实、约定与约束。把 `operationGuidance` 当作可选的附加建议：读取并考虑每一条，遵循其中适用且与内建归档流程兼容的条目。

   让这两个字段都独立于内建步骤、用户显式选择、已解析路径、CLI 检查与命令契约。若 context 与某个控制性输入冲突，报告冲突并保留控制性取值。若 guidance 不适用或与某个控制性输入冲突，不要遵循它，并解释原因。不要从这两个字段推断替代路径、被跳过的提示或参数，也不要把它们的文本逐字抄进 spec、变更工件或归档摘要，除非用户另行要求。它们是提示级行为契约，不是可强制执行的检查。

2. **检查工件完成状态**

   运行 `openspec status --change "<name>" --json` 检查 artifact_completion。

   解析 JSON 以理解：
   - `schemaName`：所使用的工作流
   - `planningHome`、`changeRoot`、`artifactPaths` 与 `actionContext`：路径与作用域上下文
   - `artifacts`：工件清单及其状态（`done`、`skipped` 或其它）

   **若任一工件既非 `done` 也非 `skipped`**（skipped 工件视为满足要求——变更声明了 skip_specs）：
   - 展示告警，列出未完成的工件
   - 请用户确认是否继续
   - 用户确认后继续

3. **检查任务完成状态**

   用相同的选定 root 参数运行 `openspec list --json`，在 `changes` 中找到 `name` 与选定变更完全一致的条目。
   要求恰好一条匹配，且 `totalTasks` 与 `completedTasks` 为非负整数，`completedTasks <= totalTasks`。CLI 会解析该 schema 追踪的任务文件，包括自定义工件名、输出路径与 glob。
   未完成任务 = `totalTasks - completedTasks`。

   不要从工件状态或缺少顶层 `tasks.md` 推断任务完成度。若查询失败、返回无效 JSON、省略或重复选定变更、或返回无效计数，报告问题并在同步或归档前停下。
   CLI 只把 `x`/`X` 复选框标记计为完成；其它标记（包括不认识的）保持未完成。

   **若发现未完成任务：**
   - 展示告警，给出未完成任务数
   - 请用户确认是否继续
   - 用户确认后继续

   **若 `totalTasks` 为零：** 继续，不展示任务相关告警。

4. **评估 delta spec 同步状态**

   只用 status JSON 里的 `artifactPaths.specs.existingOutputPaths` 作为 delta spec 的唯一来源。若缺少 `specs` 条目或 `existingOutputPaths` 为空，不做同步提示直接继续，也不要从其它工件推断 delta spec。

   **若存在 delta spec：**
   - 把每个 delta spec 与其对应的主 spec 比较：`<planningHome.root>/openspec/specs/<capability-path>/spec.md`（用第 2 步的 store 感知 `planningHome.root`，而非硬编码仓库路径）
   - 主 spec 缺失**并非自动**等于「已同步」。对新 capability 而言，主 spec 是同步的*产出*，不是输入：
     - 若 delta 有 MODIFIED 或 RENAMED requirement，报告只有 ADDED requirement 才能创建新主 spec，并把该 capability 标记为同步阻塞。绝不臆造没有当前版本的 requirement。
     - 否则，若 delta 只有 REMOVED requirement 且变更的 `.openspec.yaml` 声明了 `retire_capabilities: true`，该 capability 已退役：计为已同步，告警没有剩下可移除的东西，不要重建主 spec。此规则现在与校验已完成同步时都要应用。
     - 否则，若 delta 没有 ADDED requirement，报告无法同步并把该 capability 标记为同步阻塞。对于只有 REMOVED 的 delta，告警没有主 spec 可移除，让主 spec 树保持原样。`openspec archive` 会以 `Spec must have at least one requirement` 拒绝未标记的 REMOVED-only 情形。
     - 否则，计该 capability 需要同步，并在摘要里点名（`<capability-path>: new main spec will be created`）。若 delta 也有 REMOVED requirement，告警它们会被忽略，因为没有主 spec 可移除。同步只从 delta 的 ADDED requirement 创建主 spec，与 `openspec archive` 的做法完全一致。
   - 确定会被应用的变更（新增、修改、移除、重命名）
   - 即使某个 capability 同步阻塞，也继续评估其余 capability。提示前展示合并摘要。

   **提示选项：**
   - 若有任何 capability 同步阻塞：解释原因，只提供「不同步直接归档」「取消」
   - 否则，若需要变更：「立即同步（推荐）」「不同步直接归档」
   - 否则，若已同步：「立即归档」「仍要同步」「取消」

   按回答分流：
   - 「取消」——停下，不归档
   - 「不同步直接归档」或「立即归档」——继续归档
   - 「立即同步（推荐）」或「仍要同步」——同步，然后校验（见下）。任何 capability 同步阻塞时不要启动同步；解释阻塞原因并重复可选选项。
   - 其它任何回答——重新询问，而不是归档

   选定的同步写任何主 spec 之前，用相同的选定 root 参数运行一次
   `openspec instructions specs --change "<name>" --json`。要求零退出码与有效的工件说明 JSON。若查询失败或返回无效 JSON，报告错误，并在写任何主 spec 或移动变更前停下。返回有效但省略 `rules` 即无规则情形。把返回的 `rules` 只应用于本次合并产出的主 spec 的内容与形式；不要把它们当归档指引、不要把 `rules` 当作变更 CLI 行为的依据，也不要把规则文本抄进任何产出文件。

   然后为变更 '<name>' 在行内运行 `openspec-sync-specs` 工作流（agent 驱动的智能合并），传入上面的 delta spec 分析与刚获取的 specs 规则快照，并等它完成。行内同步必须复用该快照，不得再次获取 `specs` instructions。不要把它委托给后台任务——第 5 步会把 `changeRoot` 从仍在读取它的同步底下移走，导致变更被归档而主 spec 永不更新。若你的 agent 只能通过委托运行它，就同步委托并等待结果。

   若同步报告任何停止或阻塞条件，把同步视为失败。
   立即停止归档。不要执行同步后内容比较，也不要移动其 `changeRoot`。
   没有任何东西被移动，所以用户可以修复阻塞条件或重跑同步。

   同步每写出一个主 spec 后，对照规范的同步契约校验其结构：
   - 新主 spec 以 `# <capability> Specification` 标题开头。既有主 spec 保持其标题原样。
   - 对已建立的主 spec，完整保留既有 `## Purpose` 小节不动。
   - 对新主 spec，逐字复制 delta 的 `## Purpose`。仅当 purpose 文本短于标准校验预期时告警。不要重新生成或改写既有已撰写的 purpose。若未提供可用的 `## Purpose`，沿用既有的 TBD Purpose 行为与告警。
   - 校验主 spec 里不再残留 delta 式小节头（`## ADDED Requirements`、`## MODIFIED Requirements`、`## REMOVED Requirements`、`## RENAMED Requirements`），严格遵循同步工作流的格式规则。
   - 同步写入或改动的 requirement 块用 `### Requirement:` 标题，其 scenario 用 `#### Scenario:` 标题，位于该 spec 的 `## Requirements` 小节下。delta 未提及的内容保持原样。

   然后从本步开头重跑比较（含显式退役、主 spec 缺失的情形），覆盖 `artifactPaths.specs.existingOutputPaths` 里每个有 delta spec 的 capability——不只是同步报告它触碰过的那些。成功的同步不留下任何待应用内容，所以每个 capability 现在都必须读作已同步：
   - ADDED requirement 存在
   - MODIFIED requirement 携带 delta 指定的 scenario 与描述改动，其它 scenario 完好
   - REMOVED requirement 消失——且本次同步退役某 capability 处（移除其最后一个 requirement，使 `## Requirements` 为空），其主 spec 被删除而不是留空
   - RENAMED requirement 以新名存在、以旧名不存在

   若同步失败，或任何 capability 不匹配，报告差异并停下——不要归档。没有任何东西被移动，`changeRoot` 完好，所以用户可以修复不一致、重跑同步并重新开始归档。

5. **执行归档**

   若 `planningHome.changesDir` 下没有 `archive` 目录则创建：
   ```bash
   mkdir -p "<planningHome.changesDir>/archive"
   ```

   生成目标名：变更名已以 `YYYY-MM-DD-` 前缀开头时原样使用；否则在当前日期前拼接为 `YYYY-MM-DD-<change-name>`。绝不叠加第二个日期（与 `openspec archive` 同规则）。

   **检查目标是否已存在：**
   - 若已存在：以错误失败，建议重命名既有归档或改用不同日期
   - 若不存在：把 `changeRoot` 移入归档目录

   ```bash
   mv "<changeRoot>" "<planningHome.changesDir>/archive/<target-name>"
   ```

6. **展示摘要**

   展示归档完成摘要，包括：
   - 变更名
   - 所用 schema
   - 归档位置
   - 是否同步了 spec（如适用）
   - 任何告警的说明（未完成的工件/任务）

**成功时的输出**

```markdown
## Archive Complete

**Change:** <change-name>
**Schema:** <schema-name>
**Archived to:** the archive path derived from `planningHome.changesDir`/<target-name>/
**Specs:** <"✓ Synced to main specs" only if the step 4 verification passed; otherwise "No delta specs" or "Sync skipped">

<"All artifacts complete. All tasks complete." — or, if archived with warnings, list them instead (e.g. "Archived with 2 incomplete tasks")>
```

**护栏**
- 宣告选定的变更；含糊时请用户选择
- 用工件图（openspec status --json）做完成度检查
- 不要因告警阻塞归档——告知并确认即可
- 移到归档时保留 .openspec.yaml（它随目录一起移动）
- 展示清晰的所发生事情摘要
- 若请求同步，在行内运行 `openspec-sync-specs` 工作流（agent 驱动）
- 绝不在 spec 同步仍在进行时归档——在行内运行同步并在移动 `changeRoot` 前校验主 spec
- 若存在 delta spec，始终运行同步评估并在提示前展示合并摘要
- 应用相关运行时上下文并报告冲突；operation guidance 保持建议性
- 考虑每一条 guidance 条目，解释任何不适用或冲突的建议
- 既有 CLI 检查、已解析路径、提示与命令契约不变
- 工件规则只约束正在写入的 spec，绝不是操作指引
- 绝不把运行时上下文、operation guidance 或工件规则文本逐字抄进产出文件
