# sasspile 全语言框架设计 — 完成报告

> 说明:目标完成后系统要求生成 canvas 报告,但项目级 canvas 目录
> `~/.qoder/projects/-Users-panglijun-rust-qrass/canvases` 在沙箱可写范围之外,且本机无
> canvas SDK 声明可核对。故以等价 Markdown 交付(数据全部内联,无截图 —— 本阶段是文档/设计工作)。

## 1 · 成果摘要

系统性吃透整个 `sass-spec`(值/单位/calculation、选择器/extend、callable、模块 use/forward/import、
core_functions、css at-rules、directives、variables 作用域),产出**覆盖全语言、自洽、可定稿**的 OpenSpec
框架设计。核心架构定为**混合式**:普通求值是冷流算子,`@extend` / 媒体冒泡 / 模块是**物化 owned 结构上的
全局 pass**;真并发只在模块加载(order 键保序);入口 `async compile` 为唯一出口,终点 `into_future().await`。
**本阶段未新增产品实现**,产品树保持绿色。

| 指标 | 值 |
|---|---|
| `openspec validate --strict` | ✅ valid |
| `design.md` | 153 行 / 10 节 |
| 三份 spec 需求数 | 33(scss-compilation 20 + reactive 6 + conformance 7) |
| 遗留 Open Questions | 0(已裁定) |

## 2 · 关键步骤(逐域取证 → 六要素 → 全 change 对齐 → 裁定)

| 步骤 | 内容 | 证据(实测样本) |
|---|---|---|
| 逐域取证 | 吃透 8+ 域写入契约 | calc 不可化简原式保留;`1/2`=斜杠 list;`@extend` 链式 + `:is` + `:todo: dart-sass` |
| 模块/导入 | 加载器与排序契约 | `@use … with`/`!default` 多文件;`@import` CSS 上提排序;`@at-root (without: media)` |
| 可调用 | 参数/实参绑定 | `meta.inspect($args)` → positional `(1,)`、named `(c: 1)`、空 `()` |
| design 六要素 | 数据模型/6相管线/并发保序/子系统契约/P0–P8/已定决策 | `design.md` 10 节齐备 |
| 对齐全 change | 去旧局部教条 | proposal/README/tasks/3 specs 统一“混合式 + 唯一出口 compile” |
| 裁定 2 问 | Open Questions → 已定决策 | 依“唯一参照禁 dart-sass”铁律 + sass-spec 硬约束诊断文本 |

## 3 · 变更文件(仅 `openspec/` 文档,无产品码)

- `openspec/changes/redesign-sasspile-rxrust-shared-stream/design.md` — 核心设计 · 153 行 · 10 节
- `…/proposal.md` — 升格全语言;去 parse.rs/全程 Shared 旧述
- `…/tasks.md` — 重生成为 P0–P8 阶段 + scoped regress 口径
- `…/README.md` — 一句话摘要对齐
- `…/specs/scss-compilation/spec.md` — 20 条行为需求(覆盖全语言)
- `…/specs/reactive-stream-architecture/spec.md` — 6 条混合式架构契约
- `…/specs/sasspile-conformance/spec.md` — 7 条唯一出口 + regress 闸

## 4 · 验证证据

| 检查 | 结果 |
|---|---|
| `openspec validate --strict` | Change is valid |
| `cargo build -p sasspile` | 无 error / 无 warning |
| `cargo test --all-targets` | 全过(compile 8 …);0 failed |
| `cargo clippy --all-targets -D warnings` | exit 0 |
| `sasslint crates/sasspile/src` | count = 0 |
| hrx harness 基线(`ledger`) | 560 → 561,regress = 0 |

## 5 · 最终结论

本阶段目标达成并逐项可验;实现属**“用户定稿后”的下一阶段**(从 P0 harness 起步)。

**定稿前请确认两个已定决策(可一票否决,即改):**
1. 诊断纳入 v1 达标:`output.css` / `error` / `warning` 三类别 **scoped 基线**,文本走 `ledger` 归一化比对。
2. `element-plus` 只作**非回归自基线**,**绝不用 dart-sass 生 golden**(守“唯一参照”铁律)。
