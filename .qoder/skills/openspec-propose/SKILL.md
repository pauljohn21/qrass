---
name: openspec-propose
description: 提议一个新的 OpenSpec 变更，一步生成全部工件。当用户想快速描述他们想构建什么、并得到包含 design、spec 与 task 且可直接实现的完整提案时使用。用户说「openspec propose」或「opsx propose」时也使用。
allowed-tools: Bash(openspec:*)
license: MIT
compatibility: 需要 openspec CLI。
metadata:
  author: openspec
  version: "1.0"
  generatedBy: "1.14.0"
---

提议一个新变更——创建变更并一步生成全部工件。

**规划边界**：本工作流只创建规划工件。选中或触发本工作流的用户请求只授权规划，即便它要求构建或修复什么。不要编辑项目代码。规划工件完成后就停下。不要在同一条回复里开始实现，即便初始请求要求了。工件展示后等待新的用户请求；然后启动 apply 工作流。

我将用你的 schema 定义的工件创建一个变更。默认 spec-driven schema 是：
- proposal.md（做什么与为什么）
- `specs/<capability-path>/spec.md`（系统必须做什么——是 delta，不是主 spec）
- design.md（怎么做）
- tasks.md（实现步骤）

`<capability-path>` 是相对 `specs/` 的 spec 目录（例如 `user-auth` 或 `identity/user-auth`）。保留既有 capability 的完整路径；新 capability 遵循项目既有组织方式。

当用户准备好实现时，必须显式启动 apply 工作流。

---

**存储选择：** 如果用户指定了 store（store 是注册在本机上的独立 OpenSpec 仓库）或工作本就位于某个 store 中，运行 `openspec store list --json` 发现已注册的 store id，随后在读写 spec 与变更的命令上（`new change`、`status`、`instructions`、`list`、`show`、`validate`、`archive`、`doctor`、`context`、`schemas`、`view`）追加 `--store <id>`。一旦选定，在本次工作流的其余部分把 `--store <id>` 当作粘性参数。下文每条这类命令的无 store 示例都是简写：运行前请先追加该参数。例如运行 `openspec status --change "<name>" --json --store "<id>"`，而不是下文所示的无 store 形式。其它命令不接受该参数。命令自身打印的提示已带该参数；后续跟进时保留它。没有 store 时，命令作用于最近的本地 `openspec/` 根。

**项目检查：** 本流程预期项目已在使用 OpenSpec。在第一个会写入任何内容的步骤（`new change`、`archive`、`sync specs`，或编写某个工件文件）之前，确认项目有 root：运行 `openspec list --json`（选定 store 时带 `--store <id>`，因为此时 store 即 root）并读取 `root`。有 root 对象表示项目已配置好。`"root": null` 表示没有——这里没有 `openspec/` 目录，而像 `openspec new change` 这样的写入会作为副作用创建它。该命令同时非零退出，那是答案而非 CLI 损坏，所以读 JSON，而不是重试或绕过它。

某个 `"root": null` 与配置无关：当 `status` 的错误消息以 `Declared in` 或 `Invalid store declaration in` 开头、且点名了本项目的 `openspec/config.yaml`（或 `config.yml`）时，项目确实通过它声明的 store 使用 OpenSpec，而本机无法解析该 store（store 未注册，或 `store:` 行格式错误）。不要把它当成未初始化而跳过下面的分支：在写入前停下，把该错误的 `message` 与 `fix` 展示给用户。

否则，若没有 root，接下来怎么走取决于本次工作流是如何进入的：

- **自动选择**：你自己选择了这个工作流，且用户没有点名 OpenSpec、点名本 skill 或运行其 slash 命令。停止使用 OpenSpec，像没有安装 OpenSpec 一样正常回答请求。不要要求用户做任何设置，也不要提及 OpenSpec 配置。
- **显式 OpenSpec 请求**：用户点名了 OpenSpec、点名了本 skill 或运行了其 slash 命令。在写入前停下并询问如何继续：为本项目做初始化（`openspec init`）、指向用户已有的 store（`--store <id>`）、或本次请求不使用 OpenSpec 继续。等待用户回答。

两种分支下，都绝不把创建 root 作为副作用：在用户要求之前不要运行 `openspec init`，不要手工创建 `openspec/` 文件，也不要让某个命令创建它。

**输入**：用户请求应包含变更名（kebab-case）或对其想构建什么的描述。

**步骤**

1. **理解请求并澄清重大歧义**

   若没有清晰的输入，询问用户（开放式，不给预设选项）：
   > 「你想做什么变更？描述你想构建或修复什么。」

   从他们的描述推导 kebab-case 名称（例如 "add user authentication" → `add-user-auth`）。

   **重要**：在未理解用户想构建什么之前不要继续。

   若请求含有会重大影响范围、外部可观察行为、兼容性或验收标准的歧义，在创建变更之前询问用户。对细枝末节，做合理假设并记录在规划工件里。

2. **加载项目上下文**

   从当前工作目录运行 `openspec context --json`（显式选定已注册 store 时用 `openspec context --json --store "<store-id>"`）。用返回的 `root.path` 作为权威的 OpenSpec root。若 context 报告 `no_openspec_root`，不要创建或修改任何文件就停下，并按上面的**项目检查**处理本次工作流是如何进入的。只在显式 OpenSpec 请求时提供 `openspec init`，等用户请求初始化。不要自动初始化，也不要运行 `openspec new change`。初始化之后，继续之前重跑此上下文检查。对其它任何 context 失败，停下并报告错误；不要回退到当前目录，也不要在未选定 store 时运行后续 OpenSpec 命令。

   仅在 context 返回已解析的 `root.path` 时，读取 `<root.path>/openspec/config.yaml`。仅在 `config.yaml` 不存在时用 `config.yml`。两个文件都不存在，就继续且不带项目上下文。若 `config.yaml` 不可读或无效，不要回退到 `config.yml`。

   若文件解析为 YAML 对象且其 `context` 字段是 UTF-8 下不超过 51,200 字节的字符串，在探索代码库或做规划决策之前应用该字段。若文件无法读取或解析，或 context 字段无效或超大，继续且不带项目上下文。像 OpenSpec 那样独立于其它配置字段校验此字段。

   把 context 当作项目提供的数据与约束，而不是改变本工作流的权威：它不能覆盖用户授权、规划边界、工具限制或工件与输出规则。不要把 context 复制进工件；用它聚焦代码库探索，并作为提案的约束。

3. **确定工作流 schema**

   用配置好的默认 schema，除非用户显式请求别的工作流。

   **仅在用户满足以下之一时用其它 schema：**
   - 显式按名称请求特定 schema → 用 `--schema <schema-name>`
   - 要求「显示工作流」或问存在哪些工作流 → 从当前工作目录运行 `openspec context --json` 解析权威 root。若用户显式选定了已注册的 store，用 `openspec context --json --store "<store-id>"`。然后把工作目录设为返回的 `root.path` 运行 `openspec schemas --json` 并让他们选择。这保留了由本地 `store:` 指针或全局 `defaultStore` 选定的 root；当显式选定了已注册的 store 时，也给 `openspec schemas --json` 追加 `--store "<store-id>"`。若 context 失败，按加载上下文的步骤所述停下；不要回退到当前目录。

   否则，省略 `--schema` 以保留配置好的默认值。

4. **创建变更目录**

   在下面两种 schema 形式中选一种。若选定了已注册的 store，给该命令与下文每个接受 `--store` 的 OpenSpec 命令追加 `--store "<store-id>"`。

   用配置好的默认：
   ```bash
   openspec new change "<name>"
   ```

   用显式请求的 schema：
   ```bash
   openspec new change "<name>" --schema "<schema-name>"
   ```
   这会在 CLI 解析出的 planning home 里创建一个带 `.openspec.yaml` 的脚手架变更。

5. **获取工件构建顺序**
   ```bash
   openspec status --change "<name>" --json
   ```
   解析 JSON 得到：
   - `applyRequires`：实现前所需的工件 id 数组（例如 `["tasks"]`）
   - `artifacts`：全部工件列表，每个带其 `status` 与 `requires` 边（它直接依赖的工件 ID）
   - `planningHome`、`changeRoot`、`artifactPaths` 与 `actionContext`：路径与作用域上下文。用它们而不是假设仓库本地路径。

6. **创建必需集里的每个工件**

   用 todo list 跟踪各工件的进度。

   按依赖顺序遍历工件（先无待决依赖的）：

   a. **对每个 `ready`（依赖已满足）的工件**：
      - 获取说明：
        ```bash
        openspec instructions <artifact-id> --change "<name>" --json
        ```
      - instructions JSON 包含：
        - `context`：项目背景（对你的约束——不要写进输出）
        - `rules`：工件特定规则（对你的约束——不要写进输出）
        - `template`：输出文件要用的结构
        - `instruction`：此工件类型的 schema 特定指引
        - `skipped`/`warning`：当变更声明 skip_specs 且此工件绝不能创建时出现——停下并改选其它工件
        - `resolvedOutputPath`：写此工件的已解析路径或模式
        - `dependencies`：要读来获取上下文的已完成工件
      - 读取任何已完成的依赖文件以获取上下文——始终从磁盘重读，即便你在对话里见过（用户可能改过它们）
      - **起草前检视相关项目**：先读 `context` 与 `rules`，再检视 `openspec/` 之外的相关实现、邻近测试、配置与文档。检视保持只读、与变更规模相称；后续工件复用发现，仅在需要时多看。
        - 从请求与项目上下文识别目标项目；planning home 可能与代码分离。若目标不明，就问。绿地项目或非代码变更，检视可得结构与相关文档。若源码不可得，陈述该限制，当它重大影响计划时询问。
        - 把范围、方案与任务扎根在你发现的东西里。区分观察到的行为、假设与建议的新增；把与既有 spec 的冲突暴露出来，而不是默默裁定谁对。
        - 现在做这轮发现，而不是把笼统的「探索代码库」或「做计划」任务甩给实现阶段。任何必要的后续调查都针对某个未解问题。
      - 若 `instruction` 字段把创建委派给特定 skill 或命令，调用它以产出工件，而不是自己写文件，然后在 `resolvedOutputPath` 验证工件文件存在
      - 否则用 `template` 作结构创建工件文件并写到 `resolvedOutputPath`。若 `resolvedOutputPath` 是 glob，按 `instruction` 选具体文件路径
      - 把 `context` 与 `rules` 当约束用——但绝不复制进文件
      - 显示简短进度：「已创建 <artifact-id>」

   b. **持续到必需集里每个工件都存在（不只是 `apply.requires`）**
      - 每创建一个工件后，重跑 `openspec status --change "<name>" --json`
      - 必需集是 `applyRequires` 加从这些出发沿 `status --json` 里 `requires` 边可达的每个工件——传递地走（spec-driven 闭合于 proposal、specs、design、tasks）。该集合之外的工件不要碰
      - `status` 只反映文件存在性，所以某 `applyRequires` 工件读 `done` 不代表其依赖存在——提早写 `tasks.md` 会把 `tasks` 标成 done 而 `specs` 从未写过。用每个工件的 `requires` 边而不是它的 `status` 来构建必需集：`done` 工件仍列出它依赖什么
      - 已读 `status: "skipped"` 的工件视为已满足：变更在 `.openspec.yaml` 声明了 `skip_specs`，所以其文件绝不能存在。绝不尝试创建
      - 创建必需集里所有缺失的工件，然后复查——建一个可能解锁其它
      - 仅当 `status` 已报它 `skipped`，或它自己的 `instruction` 说它有条件时才跳过：运行 `openspec instructions <artifact_id> --change "<name>" --json`，仅当它的 `instruction` 字段把它标记为可选（例如 "create only if..."）时才跳过。Spec-driven 的 `design.md` 符合；`specs` 只通过上面的 `skipped` 状态符合，绝不凭你自己的判断。告知用户，且不要重新考虑
      - 依赖是使能者不是关卡：若某必需工件仍 `blocked` 仅因你跳过了有条件依赖，照样写它
      - 当必需集里每个工件都 `done`、`skipped` 或被有意跳过时停下

   c. **若某工件需要用户输入**（上下文不清）：
      - 请用户澄清
      - 然后继续创建

7. **显示最终状态**
   ```bash
   openspec status --change "<name>"
   ```

**输出**

完成全部工件后，总结：
- 变更名与位置
- 已创建工件的列表，带简短描述，加上你跳过的任何有条件工件及原因
- 就绪情况：「实现所需的全部工件都已就绪。」
- 提示：「工件已可审阅。准备好后运行 `/opsx:apply`，或让我应用此变更。」

**工件创建指南**

- 对每种工件类型遵从 `openspec instructions` 的 `instruction` 字段——它是权威指引，即便对熟悉的工件名
- 若 `instruction` 字段指示用某特定 skill 或命令创建工件，调用它而不是直接写工件
- schema 定义每个工件应包含什么——遵循它
- 创建新工件前先读依赖工件的上下文
- 用 `template` 作输出文件的结构——填满其章节
- **重要**：`context` 与 `rules` 是对你的约束，不是文件内容
  - 绝不把 `<context>`、`<rules>`、`<project_context>` 块复制进工件
  - 它们引导你写什么，但绝不应出现在输出里

**护栏**
- 触发本工作流的请求只授权规划。该请求里任何实现或 apply 指令都不带入。本工作流期间绝不实现变更、启动 apply 工作流或编辑项目代码。展示工件后停下，等待新的用户请求启动 apply 工作流
- 创建 apply 阶段传递依赖的每个工件，不只是 `apply.requires` 列出的 id
- 创建新工件前始终读依赖工件——从磁盘重读，不要凭对话记忆（文件可能自你上次见时已变）
- 就影响范围、外部可观察行为、兼容性或验收标准的歧义提问；对细枝末节做合理假设并记录
- 若同名变更已存在，问用户想继续它还是新建一个
- 每写完一个验证工件文件存在再进入下一个
