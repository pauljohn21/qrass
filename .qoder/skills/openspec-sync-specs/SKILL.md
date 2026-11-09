---
name: openspec-sync-specs
description: 把 OpenSpec 变更的 delta spec 同步到主 spec。当用户想把 delta spec 里的改动更新到主 spec、且不归档该变更时使用。用户说「openspec sync」或「opsx sync」时也使用。
allowed-tools: Bash(openspec:*)
license: MIT
compatibility: 需要 openspec CLI。
metadata:
  author: openspec
  version: "1.0"
  generatedBy: "1.14.0"
---

把变更的 delta spec 同步到主 spec。

这是一项 **agent 驱动**的操作——你会读取 delta spec 并直接编辑主 spec 来应用变更。这样允许智能合并（例如只添加一个 scenario，而不复制整个 requirement）。

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

   提示时，展示那些拥有 delta spec（`specs/` 目录下）的变更。

   始终宣告：「使用变更：<name>」，并说明如何覆盖（例如 `/opsx:sync <other>`）。

2. **解析变更上下文**

   运行：
   ```bash
   openspec status --change "<name>" --json
   ```

   JSON 含 `planningHome.root`。主 spec 位于 `<planningHome.root>/openspec/specs/` 下——下文每个主 spec 路径都用这个（store 感知的）root，而不是硬编码的仓库路径。选定 store 时它指向该 store，而非当前仓库。

3. **查找 delta spec**

   只用 status JSON 里的 `artifactPaths.specs.existingOutputPaths` 作为 delta spec 路径的唯一来源。若缺少 `specs` 条目或 `existingOutputPaths` 为空，报告没有可同步的 delta spec，不要从其它工件推断，并停在那里——不要请求工件说明，也不要写主 spec。

   同步 `existingOutputPaths` 里的每个路径，除非调用方收窄了集合。调用方收窄指明确列出 `existingOutputPaths` 中完整条目的清单——逐字复制那些绝对值。archive 会在行内这样做，用户也可以（例如选中以 `/specs/billing/invoices/spec.md` 结尾的条目）。随后只同步被点名的路径，其余 delta spec 原样保留：批量归档会排除它找不到实现的 delta，而照样同步它等于写出调用方刻意扣住的主 spec。把收窄后的选择带入第 4 步；绝不放宽回完整清单。若被点名的路径不在 `existingOutputPaths` 里，不要同步它——报告并停下，而不是默默丢弃。若清单为空，报告没有可同步的内容并停下，不要写主 spec。

   每个 delta spec 文件含如下小节：
   - `## ADDED Requirements` - 要新增的 requirement
   - `## MODIFIED Requirements` - 对现有 requirement 的改动
   - `## REMOVED Requirements` - 要移除的 requirement
   - `## RENAMED Requirements` - 要重命名的 requirement（FROM:/TO: 格式）

   若没找到 delta spec，通知用户并停下。

4. **对每个 delta spec，把变更应用到主 spec**

   在首次写主 spec 之前，获取一份当前的 specs 规则快照：
   - 若 archive 在行内调起本工作流，并提供了来自 `openspec instructions specs --change "<name>" --json` 的有效快照，复用它，不要再次获取同一份 instructions。
   - 否则现在用相同的选定 root 参数运行一次该命令。
   - 若直接查询非零退出或返回无效的工件说明 JSON，报告错误并在写任何主 spec 之前停下。不要把该失败当作规则集缺失。
   - 若返回有效但省略了 `rules`，表示未配置工件规则，沿用现有的语义合并。

   把返回的 `rules` 只应用于本次合并产出的主 spec 的内容与形式。工件规则不是操作指引，不能改变选定的 root、delta 路径、CLI 检查或工作流步骤。把它们的文本当作约束使用，不要逐字抄进主 spec 或摘要。

   对第 3 步选中的每个 capability delta spec 路径——完整的 `existingOutputPaths` 清单，或调用方提供收窄后的子集（它们可能属于选定的 store，而非本仓库）：

   a. **读取 delta spec**，理解预期变更

   b. **读取主 spec**：位于 `<planningHome.root>/openspec/specs/<capability-path>/spec.md`（可能尚不存在）

      **若它尚不存在**（新 capability），与 `openspec archive` 的做法一致：只允许应用 ADDED requirement——第 d 步会据此创建 spec。MODIFIED 与 RENAMED 没有可作用的 requirement，因此对该 capability 停止同步，并报告其主 spec 不存在、新 spec 只允许 ADDED；绝不臆造缺失的 requirement。REMOVED 没有可移除的东西——跳过并告警。

   c. **智能应用变更**：

      **ADDED Requirements:**
      - 若主 spec 里没有该 requirement → 添加它
      - 若主 spec 里已有 → 更新为与之一致（视为隐式 MODIFIED）

      **MODIFIED Requirements:**
      - 在主 spec 里找到该 requirement
      - 应用变更——可以是：
        - 添加主 spec 还没有的新 scenario
        - 修改现有 scenario
        - 改写 requirement 的描述
      - 保留 delta 未提及的 scenario/内容

      **REMOVED Requirements:**
      - 从主 spec 移除整个 requirement 块
      - 退役该 capability。删除整个 `spec.md`——并在目录里再没有别的东西时删除目录——仅当以下条件全部满足：
        1. 本次运行移除 requirement 后没有留下任何 requirement 块；
        2. spec 剩余部分结构良好（仍有一个 `## Purpose`）；
        3. 本次同步之前主 spec 并非已空——若你什么都没移除，就什么都不改；
        4. 整个文件里所有其它非空行都能被解释为标题、Purpose、Requirements 头，或某个规范 requirement 的陈述、scenario 与围栏示例；
        5. 变更的 `.openspec.yaml` 声明了 `retire_capabilities: true`；
        6. `spec.md` 解析在真实 specs root 之内（不要跟随 capability 目录的符号链接去删除外部文件）。
        若移除选定的 requirement 会不留任何 requirement 块、且任一退役条件不满足，不要修改主 spec。对该 capability 停止同步，报告阻塞条件，并告诉用户如何解决。绝不写入或留下空的 `## Requirements` 小节。
        若只是缺 marker，也说明这点——它是用户能补齐的唯一一项，能让退役走通。
      - 删除文件也会删掉它的 `## Purpose`；任何其它小节都会阻止退役。报告退役时点明 Purpose。仅当该 spec 位于调用方的检出中时，才附上一段可直接粘贴的 `git checkout`；否则给出针对检出的恢复指引。

      **RENAMED Requirements:**
      - 找到 FROM 的 requirement，重命名为 TO

      **delta 中的 `## Purpose`：**
      - 主 spec 已有自己的 Purpose 且它权威——不要动它（这正是 `openspec archive` 的做法；它告警后继续）。

   d. **若 capability 尚不存在，创建新主 spec：**
      - 仅当 delta 有 ADDED requirement 可放入、且没有 MODIFIED 或 RENAMED 在第 b 步阻塞该 capability 时。否则什么都不创建，让 specs 目录保持原样。对于只有 REMOVED 的 delta：若变更的 `.openspec.yaml` 声明了 `retire_capabilities: true`，报告它已退役并继续，不要重建 spec。没有该 marker 时，把同步报告为阻塞：`openspec archive` 会以 `Spec must have at least one requirement` 拒绝它。空 delta 没有可同步的操作，也报告为阻塞。绝不写入空的 `## Requirements` 小节。
      - 创建 `<planningHome.root>/openspec/specs/<capability-path>/spec.md`
      - 添加 Purpose 小节：若 delta 的 `## Purpose` 有正文则逐字复制（这正是 `openspec archive` 的做法）；若没有，只写一个简短的 TBD 占位
      - 添加 Requirements 小节，含 ADDED requirement
      - 遵循下面的 **主 spec 格式参考**

5. **校验更新后的主 spec**

   用与前面相同的选定 root 参数运行 `openspec validate --specs`。若校验失败，报告问题，不要宣称同步成功。

6. **展示摘要**

   应用全部变更后，总结：
   - 哪些 capability 被更新
   - 做了哪些变更（新增/修改/移除/重命名的 requirement）
   - 是否有新主 spec 留下 TBD Purpose 占位，以便立即补写而不是悬着
   - 是否有 capability 退役，点明被删的 `spec.md`、它的 Purpose，以及一段可直接粘贴的 `git checkout` 或针对检出的恢复指引

**Delta Spec 格式参考**

```markdown
# Spec Delta

## Purpose

Only on a delta that introduces a brand-new capability. Seeds the new main spec.

## ADDED Requirements

### Requirement: New Feature
The system SHALL do something new.

#### Scenario: Basic case
- **WHEN** user does X
- **THEN** system does Y

## MODIFIED Requirements

### Requirement: Existing Feature
The system SHALL keep doing the existing thing, now also handling A.

#### Scenario: Scenario the main spec already has
- **WHEN** user does X
- **THEN** system does Y

#### Scenario: New scenario to add
- **WHEN** user does A
- **THEN** system does B

## REMOVED Requirements

### Requirement: Deprecated Feature

## RENAMED Requirements

- FROM: `### Requirement: Old Name`
- TO: `### Requirement: New Name`
```

**主 spec 格式参考**

主 spec 是 delta 要合并**进入**的目标。它绝不能包含 delta 操作头（`## ADDED/MODIFIED/REMOVED/RENAMED Requirements`）——同步之后，每个 requirement 都住在一个 `## Requirements` 小节下：

```markdown
# <capability> Specification

## Purpose
Short description of what this capability does and why it exists.

## Requirements

### Requirement: New Feature
The system SHALL do something new.

#### Scenario: Basic case
- **WHEN** user does X
- **THEN** system does Y
```

**核心原则：智能合并**

与程序化合并不同，你是合并而不是覆盖：
- 一个 MODIFIED 块携带完整 requirement——正文加上经变更后保留的每个 scenario。`openspec validate` 与 `openspec archive` 都会拒绝丢掉主 spec 仍有 scenario 的合并。
- 保留 delta 未提及的内容，维持主 spec 现有顺序
- 用你的判断合理地合并变更

**成功时的输出**

```markdown
## Specs Synced: <change-name>

Updated main specs:

**<capability-1>**:
- Added requirement: "New Feature"
- Modified requirement: "Existing Feature" (added 1 scenario)

**<capability-2>**:
- Created new spec file
- Added requirement: "Another Feature"

Main specs are now updated. The change remains active - archive when implementation is complete.
```

**护栏**
- 变更前读取 delta 与主 spec 两侧
- 保留 delta 未提及的现有内容
- 绝不把 delta 文件原样复制进主 spec——要合并其内容，使主 spec 保持主 spec 格式参考的结构，且没有 delta 操作头
- 若有不清楚的地方，请求澄清
- 边做边展示你在改什么
- 操作应幂等——运行两次应得到相同结果
- 只用 `artifactPaths.specs.existingOutputPaths`；绝不从无关工件推断 delta spec
- 尊重调用方对 `existingOutputPaths` 的收窄子集；绝不放宽回完整清单
- 直接同步只获取一次 specs instructions；或在行内复用 archive 提供的快照
- 每次写主 spec 前，若 specs 指令响应非零或 JSON 无效，停下
- 工件规则只约束正在写入的 spec，绝不抄进产出文件
