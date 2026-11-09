---
name: openspec-update-change
description: 通过修订 OpenSpec 变更既有的规划工件、并使它们彼此保持一致来更新该变更。当用户想修订某变更的计划、把新决策折入其中、或在一次编辑后调和其工件时使用。用户说「openspec update change」或「opsx update」时也使用。若用户指的是 openspec update CLI 命令（它刷新生成文件），则改为运行该命令。绝不编辑代码。
allowed-tools: Bash(openspec:*)
license: MIT
compatibility: 需要 openspec CLI。
metadata:
  author: openspec
  version: "1.0"
  generatedBy: "1.14.0"
---

修订某个变更既有的规划工件并保持它们一致。绝不编辑代码。

**存储选择：** 如果用户指定了 store（store 是注册在本机上的独立 OpenSpec 仓库）或工作本就位于某个 store 中，运行 `openspec store list --json` 发现已注册的 store id，随后在读写 spec 与变更的命令上（`new change`、`status`、`instructions`、`list`、`show`、`validate`、`archive`、`doctor`、`context`、`schemas`、`view`）追加 `--store <id>`。一旦选定，在本次工作流的其余部分把 `--store <id>` 当作粘性参数。下文每条这类命令的无 store 示例都是简写：运行前请先追加该参数。例如运行 `openspec status --change "<name>" --json --store "<id>"`，而不是下文所示的无 store 形式。其它命令不接受该参数。命令自身打印的提示已带该参数；后续跟进时保留它。没有 store 时，命令作用于最近的本地 `openspec/` 根。

**项目检查：** 本流程预期项目已在使用 OpenSpec。在第一个会写入任何内容的步骤（`new change`、`archive`、`sync specs`，或编写某个工件文件）之前，确认项目有 root：运行 `openspec list --json`（选定 store 时带 `--store <id>`，因为此时 store 即 root）并读取 `root`。有 root 对象表示项目已配置好。`"root": null` 表示没有——这里没有 `openspec/` 目录，而像 `openspec new change` 这样的写入会作为副作用创建它。该命令同时非零退出，那是答案而非 CLI 损坏，所以读 JSON，而不是重试或绕过它。

某个 `"root": null` 与配置无关：当 `status` 的错误消息以 `Declared in` 或 `Invalid store declaration in` 开头、且点名了本项目的 `openspec/config.yaml`（或 `config.yml`）时，项目确实通过它声明的 store 使用 OpenSpec，而本机无法解析该 store（store 未注册，或 `store:` 行格式错误）。不要把它当成未初始化而跳过下面的分支：在写入前停下，把该错误的 `message` 与 `fix` 展示给用户。

否则，若没有 root，接下来怎么走取决于本次工作流是如何进入的：

- **自动选择**：你自己选择了这个工作流，且用户没有点名 OpenSpec、点名本 skill 或运行其 slash 命令。停止使用 OpenSpec，像没有安装 OpenSpec 一样正常回答请求。不要要求用户做任何设置，也不要提及 OpenSpec 配置。
- **显式 OpenSpec 请求**：用户点名了 OpenSpec、点名了本 skill 或运行了其 slash 命令。在写入前停下并询问如何继续：为本项目做初始化（`openspec init`）、指向用户已有的 store（`--store <id>`）、或本次请求不使用 OpenSpec 继续。等待用户回答。

两种分支下，都绝不把创建 root 作为副作用：在用户要求之前不要运行 `openspec init`，不要手工创建 `openspec/` 文件，也不要让某个命令创建它。

**输入**：可选地指定变更名。若省略，检查能否从对话上下文推断。若含糊或有歧义，必须询问可用的变更。

本工作流修订已存在的工件；绝不创建缺失的工件。当某工件缺失时，`openspec status --change "<name>" --json` 会点名下一个，`openspec instructions "<artifact-id>" --change "<name>" --json` 解释如何编写它。

**步骤**

1. **选择变更**

   若提供了名称，就用它。否则：
   - 用户提到过某变更时，从对话上下文推断
   - 只有一个活跃变更时自动选择
   - 若有歧义，运行 `openspec list --json` 获取按最近修改时间排序的可用变更，并请用户选择

   提示时，把最近修改的 3-4 个变更作为选项展示，显示：
   - 变更名
   - 状态（例如 "0/5 tasks"、"complete"、"no tasks"）
   - 最近修改时间（来自 `lastModified` 字段）

   把最近修改的变更标为「(Recommended)」，因为它很可能是用户想更新的那个。

   始终宣告：「使用变更：<name>」以及如何覆盖（例如 `/opsx:update <other>`）。

2. **获取变更的工件**
   ```bash
   openspec status --change "<name>" --json
   ```
   解析 JSON 以了解当前状态。响应包括：
   - `schemaName`：所使用的工作流 schema（例如 "spec-driven"）
   - `artifacts`：工件数组，各带其状态（"done"、"skipped"、"ready"、"blocked"）
   - `isPlanningComplete`：布尔值，表示全部规划工件是否完成。较旧的 CLI 版本把同一值暴露为 `isComplete`。
   - `planningHome`、`changeRoot`、`artifactPaths` 与 `actionContext`：路径与作用域上下文。用它们而不是假设仓库本地路径。

   工件 id 与路径来自当前 schema——不要假设它们，也不要按硬编码的工件名分支。自定义 schema 必须原样工作。

   要编辑的文件是 `artifactPaths.<id>.existingOutputPaths`——磁盘上已存在的具体文件，glob 工件（例如 `specs/**/*.md`）的 glob 已展开。不要写入 `resolvedOutputPath`：对 glob 工件它仍是 glob 模式，不是真实文件。

3. **理解请求**
   - 若用户要求了具体修订（「设计现在用 X」），那就是起始编辑。
   - 若他们只说了 "update" / "make this coherent"，把它当作一致性审查：读既有工件并相互检查矛盾、缺口与重复。

4. **读取并调和**
   - 读请求涉及的工件与变更的其它既有工件。
   - 在对话里起草请求的编辑，不要写进文件。弄清它确切改什么；步骤 5 拥有每次写入。然后把每个其它既有工件对照起草的编辑检查——任何方向都要：对靠后工件的编辑可能要求修订靠前的工件，不只是反过来。构建顺序是有用的阅读顺序，不是哪些工件可被修订的约束。
   - 记下所有现在不一致、缺失或矛盾之处。
   - 对已存在的文件（`existingOutputPaths`）提出修订。若某工件没有既有输出文件且状态为 `ready` 或 `blocked`，记下它并指向 `openspec instructions "<artifact-id>" --change "<name>" --json` 说明如何创建。不要动 `skipped` 工件；不要把它们当作缺失，也不要把它们推给 continue 工作流。
   - glob 工件（例如 `specs/**/*.md`）在至少一个文件匹配后标为 `done`，且 continue 工作流只处理 `ready` 工件。当调和发现某个 `existingOutputPaths` 非空的 glob 工件缺文件时：
     1. 运行 `openspec instructions "<artifact-id>" --change "<name>" --json` 并用其 `instruction` 与 `template`。把 `context` 与 `rules` 当约束；不要复制进文件。若 instructions 报告 `skipped: true`，不要创建文件。从磁盘读当前依赖文件；若必需的非 skipped 依赖缺失，停下并请用户先恢复它。
     2. 在 `changeRoot` 内选一个匹配 `artifactPaths.<id>.outputPath` 且尚不存在的具体路径。解析任何符号链接父目录后验证它仍在 `changeRoot` 内。glob `resolvedOutputPath` 不是有效目标。
     3. 把新文件纳入步骤 5 的拟议修订，且仅在用户确认后创建。
     4. 确认后、创建前，立即刷新 status 与 instructions。验证工件仍在范围内、未被 skip 且已部分填充；重复上面的具体路径检查。
     5. 用目标已存在即失败的创建操作。若 `instruction` 把创建委派给另一 skill 或命令，仅当它能遵守已确认路径与这些护栏时才调用它；否则停下。若任何检查失败或已确认的草案不再有效，停下并与用户调和，而不是替换既有内容或另选路径。
   - 若变更已一致，就这么说且不提出任何修订。

5. **确认并应用，一次一个工件**
   - 本步骤执行本工作流的每次工件写入；之前没有任何步骤编辑工件。
   - 展示每个拟议修订及其理由——包括步骤 4 起草的请求编辑。仅在用户确认后写入。
   - 若用户拒绝某修订，不要写它——让该工件保持不变。
   - 当需要大幅重写时，先取该工件的规则与模板：
     ```bash
     openspec instructions "<artifact-id>" --change "<name>" --json
     ```

6. **指向下一步（仅指引——绝不据此行动）**
   - `existingOutputPaths` 为空且状态为 `ready` 或 `blocked` 的工件 → 运行 `openspec status --change "<name>" --json` 获取下一个工件，并指向 `openspec instructions "<artifact-id>" --change "<name>" --json` 说明如何创建它。
   - 变更已实施（tasks 已勾掉 / 已 applied）→ 代码可能已不再匹配修订后的计划；建议 `/opsx:apply` 把增量带入代码。
   - 全部完成且已实现 → 建议 `/opsx:archive`。

**输出**

每次调用后，展示：
- 哪些工件被修订（以及哪些拟议修订被拒绝）
- 在已部分填充的 glob 工件下创建的任何文件
- 因尚不存在而推迟的任何事（无文件且状态为 `ready` 或 `blocked` 的工件，绝不包括 `skipped` 工件）
- 变更当前所处位置与推荐的下一个命令

**护栏**
- 仅规划工件——绝不编辑实施代码。若修订后的计划意味着代码改动，停下并指向 `/opsx:apply`。
- 使用 `openspec status` 报告的工件 id 与路径；绝不按硬编码的工件名分支。
- 只编辑 `existingOutputPaths` 里的具体文件；绝不写入 glob `resolvedOutputPath`。
- 不要推进构建前沿：若某工件 `existingOutputPaths` 为空且状态为 `ready` 或 `blocked`，创建它是本工作流之外的独立步骤。不要动 `skipped` 工件。唯一的新文件范围是 `existingOutputPaths` 非空的 glob 工件下经确认的具体路径。
- 每次编辑前都与用户确认。
- 若请求改变变更的*意图*而非细化它，要一个不同的未使用变更名并推荐改用 `openspec new change "<new-change-name>"`（「Update vs. Start Fresh」启发式）。
