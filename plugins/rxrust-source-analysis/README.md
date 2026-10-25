# rxrust-source-analysis — Qoder 插件

把 `docs/rxrust-source-analysis/` 这套 rxrust（`=1.0.0-rc.6`）源码级解析文档打包成一个
**可分发的 Qoder 原生插件**，并按 [superpowers](https://github.com/obra/superpowers) 的
`writing-skills` 规范写成 **Reference 型 skill**：任何使用 rxrust 的项目装上它，就能在
写/审管线代码前查到「某原语到底怎么工作」，而不必重新读 crate 源码或凭记忆猜。

## 它做什么

- 提供一个 skill `rxrust-source-analysis`：`SKILL.md` 只做**触发条件 + 问题→章节 检索表 + 查阅契约**，
  八章原文放在 `skills/rxrust-source-analysis/references/`（superpowers 的
  "Skill with Heavy Reference" 形态）。
- 不注入 rule、不带 hook/MCP/command：这是一份查证资料，不是生命周期自动化。

## 结构

```text
rxrust-source-analysis/
  .qoder-plugin/plugin.json          # Qoder manifest（只声明真实存在的组件）
  README.md                          # 本文件
  SOURCE.json                        # 出处与保真度记录
  assets/avatar.svg                  # 自绘图标
  scripts/validate.sh                # 离线校验（shell + node，见下）
  skills/rxrust-source-analysis/
    SKILL.md                         # 手写：name + description + 检索表（497 words，<500 目标内）
    references/                      # docs/rxrust-source-analysis 的逐字镜像（9 个文件）
      README.md  01-…08-….md
```

## 出处（provenance）

| 项 | 值 |
|----|----|
| 源目录 | `docs/rxrust-source-analysis/`（本仓库，**仍是唯一真源**） |
| 源提交 | `5bea39dbd056e59f6c811304506a806bb26a3a98`（已在 `github/main`） |
| 打包方式 | 逐字复制，`references/` 与源目录 `diff -rq` 干净（见 `SOURCE.json`） |
| 图标 | `assets/avatar.svg` 本次手写生成，未使用任何外部/市场图标 |
| 许可 | MIT（与仓库 `LICENSE` 一致，© 2026 pauljohn21） |
| 未打包 | 无遗漏：源目录 9 个 Markdown 全部进包 |

## 同步纪律（防漂移）

`docs/` 是真源，`references/` 与 `.qoder/skills/` 是镜像 —— **不要直接编辑插件或安装副本里的章节**。
改完文档后：

```sh
cp docs/rxrust-source-analysis/*.md \
   plugins/rxrust-source-analysis/skills/rxrust-source-analysis/references/
sh plugins/rxrust-source-analysis/scripts/validate.sh   # 漂移会直接 FAILED 并给出上面的 cp 命令
```

## 校验

```sh
sh plugins/rxrust-source-analysis/scripts/validate.sh [plugin-root]
```

检查项对齐 `qoder-create-plugin` 自带的 `validate_qoder_plugin.py`：manifest 存在且为合法 JSON 对象、
`name`/`version` 必填、组件路径以 `./` 开头且不逃出包根、声明的 `skills`/`logo` 指向真实文件、
每个 `SKILL.md` 的 frontmatter 有 `name`+`description`（≤1024 字符）且名字只用字母数字连字符、
`SKILL.md` 内的相对链接可解析、包内无 `.codex-plugin`/`.cursor-plugin`/`.claude-plugin`
与 `.DS_Store`/`__MACOSX`/`settings.local.json`、包目录名 == manifest `name`。

本仓库另加两道一致性门（超出 python 版）：

- **门 7**：`references/` 必须是 `docs/rxrust-source-analysis/` 的逐字镜像。
- **门 8**：若本项目 `.qoder/skills/<skill>` 已安装该 skill，它必须与包内副本逐字一致
  （未安装则只 warn 跳过，因此在别的位置校验本包也不会误报）。

**为什么不是那个 python 校验脚本**：本仓库规则（`.qoder/rules/rust.md`）禁 Python，
故用 `sh` + `node`（仅解析 JSON）实现了同一套检查。需要 `sh`/`find`/`diff`/`node`。

## 校验证据（2026-10-24 实测）

| 命令 | 结果 |
|------|------|
| `sh -n scripts/validate.sh` | 语法通过 |
| `sh scripts/validate.sh`（本包） | `OK: no issues found (1 skill)`，exit 0；三条 ok 行覆盖 manifest 解析 / 门 7 镜像 / 门 8 安装副本 |
| 负向用例 17 例（`target/plugin-neg/run.sh`，scratch） | 17/17：control 包通过，其余每例被**指定错误信息**拒绝（缺 manifest、坏 JSON、缺 version、`../` 越界、缺 `./`、logo 不存在、skills 目录缺失、目录内无 SKILL.md、无 description、无 frontmatter、skill 名字符违规、链接断、`.DS_Store`、`__MACOSX`、`.claude-plugin`、包名不匹配） |
| 门 7 漂移实测 | 往 `references/05-operators.md` 追加一行 → `ERROR: mirror drift`（exit 1）；执行同步 `cp` 后 → exit 0 |
| 门 8 漂移实测 | 往 `.qoder/skills/…/references/03-observer-subscription.md` 追加一行 → `ERROR: installed copy drifted` 并给出重装命令（exit 1）；重装后 → exit 0 |
| 检索探针 45 条（`target/plugin-neg/retrieval.sh`） | 45/45 命中：`SKILL.md` 检索表每一行承诺的符号确实能在对应章节找到；对照组证明该探针确实会报 MISS |
| 同一 45 条探针跑安装副本 | 45/45 命中（`target/plugin-neg/retrieval-installed.sh`） |

**未做的验证（如实记录）**：本机 `qodercli` 不在 PATH，未跑安装冒烟；本会话无通用 subagent，
未按 `writing-skills` 要求跑 fresh-agent 的 RED/GREEN 行为基线（那是 discipline 型 skill 的门），
上面用「校验器负向用例 + 检索探针」作为 Reference 型 skill 可得到的最接近证据。

## 安装/启用

- **本仓库（已安装）**：skill 已作为真实目录放进
  `.qoder/skills/rxrust-source-analysis/`（与现有 `.qoder/skills/rxrust-reactive` 同级、同约定），
  Qoder 会作为项目 skill 发现它 —— 新会话生效，当前会话的技能清单不会中途刷新。
  它是包内副本的逐字拷贝，改动请改 `docs/` 真源，然后按顺序重装：

  ```sh
  cp docs/rxrust-source-analysis/*.md plugins/rxrust-source-analysis/skills/rxrust-source-analysis/references/
  rm -rf .qoder/skills/rxrust-source-analysis
  cp -R plugins/rxrust-source-analysis/skills/rxrust-source-analysis .qoder/skills/
  sh plugins/rxrust-source-analysis/scripts/validate.sh   # 门 7 + 门 8 一起兜住漂移
  ```

- **作为插件分发**：整个 `rxrust-source-analysis/` 目录即可拷贝/打包分发；zip 根必须直接含
  `.qoder-plugin/plugin.json`：`cd plugins/rxrust-source-analysis && zip -r ../../rxrust-source-analysis-0.1.0.zip . -x "**/.DS_Store"`。
  安装按你所用 Qoder 版本的本地插件安装入口（本机 `qodercli` 不可用，故此处未实测）。

## 已知边界

- `references/08-recipes-and-pitfalls.md` §5 是来源项目 `sasspile`/`sasslint`/`ledger` 的验收环，
  对其它项目只是**示例**；01–07 与 README 是项目中立的。
- 文档口径锁 `rxrust =1.0.0-rc.6`；rxrust 0.15 系列 API 完全不同，不适用本参考。
