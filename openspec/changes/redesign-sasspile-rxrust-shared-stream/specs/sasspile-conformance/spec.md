# Spec Delta — sasspile-conformance(验收/回归闸的完整契约)

## Purpose

约定 `sasspile` 每个增量的**验收纪律**:行为唯一由 `sass-spec` 金样 + `element-plus` 真实语料决定;
harness 如何跑、如何立基线、`:todo:`/`non_conformant`/`libsass-*` 如何排除;`regress=0` 在分阶段下如何
scoped 判定;`sasslint`/clippy/`src` 纯产码/唯一出口 `compile`/诊断类别各闸;`element-plus` 只作非回归自
基线、**绝不用 dart-sass 生 golden**。全部为可观察的合入前置条件。

## ADDED Requirements

### Requirement: 行为唯一参照为 sass-spec 与 element-plus

`sasspile` 的行为 MUST 仅由 `sass-spec`(`.hrx` 金样 `input.scss`→`output.css`/`error`/`warning`,含多
文件 case)与 `element-plus` 真实 SCSS 语料决定。MUST NOT 以 dart-sass 源码/文档推断行为,MUST NOT 以
其"物化 AST→遍历→GC"心智作架构参照,MUST NOT 以已删除的旧 `qrass` 代码/结论为依据。被 `:todo:` 标为
待实现(含 `dart-sass`/`libsass`)、或位于 `non_conformant/`、`libsass-*` 的用例 MUST NOT 作为通过判据。

#### Scenario: 参照集界定
- **WHEN** 确定某特性期望输出
- **THEN** 取自 `sass-spec` 金样或 `element-plus` 结果,而非 dart-sass 文档描述

#### Scenario: 排除非金样用例
- **WHEN** 计算通过率/立基线
- **THEN** `non_conformant/`、`libsass-*`、`:todo:` 用例不计入回归基线

### Requirement: hrx harness 契约

`sasspile` MUST 有一个 `tests/hrx.rs` harness:遍历 `sass-spec` 安全子集、经**唯一出口 `compile`**跑每
例、产 `STATUS\tkey[\tdetail]` 快照供 `crates/ledger` 记录。harness MUST 支持:多文件 case(同目录聚合、
partial `_x.scss`/相对 import 解析);解析 `options.yml`(`:todo:`→skip/xfail、`precision`);当前只跑
`.scss`(`--indented`/`.sass` 归后续子阶段);`sourcemap`/`immutable` 可忽略。harness MUST 把
`output.css`/`error`/`warning` 分**三类别**分别记账(见诊断闸)。harness MUST NOT 用 `eprintln!`,
统计经 `tracing` 输出。

#### Scenario: 多文件模块用例
- **WHEN** 一个 hrx 含 `input.scss` + `_other.scss`
- **THEN** harness 按同 case key 聚合、以 `input.scss` 为入口经 `compile` 跑,结果计入该 key

#### Scenario: :todo: 不计入
- **WHEN** case 的 `options.yml` 含 `:todo:`
- **THEN** 记为 skip,不进 pass/fail 基线

### Requirement: 回归闸 regress=0(scoped,阶段式)

每个合入增量的 hrx 通过集 MUST 相对基线**按目录簇 scoped 零回归**:`crates/ledger` 记 per-case 快照,
合入前 `ledger diff <base> <run>` 对**该增量负责的簇** `regress` MUST 为 `0`;新阶段刚铺开、尚未实现的
域(其失败本就不在旧基线通过集内)MUST NOT 被算作回归,但 MUST NOT 让**已通过簇**回退。达不到 MUST 回退
最近绿色基线,MUST NOT 提交带回归的改动。整体通过率 MUST 随阶段单调不降。

#### Scenario: 已过簇不得回退
- **WHEN** 某 `sass-spec` 簇已在基线通过,新改动使其失败
- **THEN** `ledger diff` 报 `regress>0`,阻断合入

#### Scenario: 新域初铺不算回归
- **WHEN** 新阶段引入尚全失败的域(如初期 `values/calculation/**`)
- **THEN** 其失败不进 regress(旧基线未通过),仅要求本簇内单调上升

### Requirement: 铁律以 sasslint 确定性把关

`sasspile` 源码 MUST 通过 `crates/sasslint`(`src/` 命中数 `0`)作为合入前置:含 `mutable-accumulator-drives`、
`no-from-fn-driver`、`no-work-queue`、`no-tree`、`no-recursive-descent`、`no-relay-vec`、`no-string-machine`、
`no-imperative-loop`。命中任一即视为把控制流写回 GC/命令式,MUST 按 `reactive-stream-architecture` 的
"GC/改造反例索引"出路列改写,而非放宽规则或改名绕过。

#### Scenario: 命中即阻断
- **WHEN** 改动引入帧栈累加子驱动 / 手搓工作队列 / 递归遍历树 / 中途 collect 加工等
- **THEN** `sasslint` 命中 > 0,CI 阻断;MUST 改数据分解后重跑

### Requirement: 每阶段合入门(全绿清单)

每阶段合入 MUST 同时满足:`cargo build -p sasspile` 无 warning;`cargo test -p sasspile --all-targets`
全过;`cargo clippy --all-targets -- -D warnings` exit 0;`sasslint crates/sasspile/src` `count=0`;本阶段
scoped `regress=0`。任一不满足 MUST NOT 合入。测试 MUST 用 `#[test]`/`#[tokio::test]` 置于
`crates/sasspile/tests/`;`src/` MUST 纯生产码(无内联测试);MUST NOT 用 Python/脚本充当测试或构建步骤。

#### Scenario: 门未满即回退
- **WHEN** 六项门任一失败
- **THEN** 不合入;回退或缩小增量后重验

#### Scenario: 无脚本
- **WHEN** 需要生成/比对产物
- **THEN** 用 Rust(`#[test]`/bin)完成,MUST NOT 引入 python/shell 脚本步骤

### Requirement: 诊断类别纳入达标(归一化)

`sass-spec` 以 `error`/`warning` 为期望的用例 MUST 纳入达标:harness 分三类别(output.css / error /
warning)独立 scoped 基线;比对 MUST 经 `ledger` **归一化**(容许行号/标点差异),但**是否报错、是否告警、
deprecation `id`** 为硬判。诊断完整精确格式为 P7 目标;P7 前 error/warning 类 MUST NOT 污染 output.css
簇的 regress。

#### Scenario: 该报错未报错
- **WHEN** 金样期望 error 而 `compile` 返回 Ok
- **THEN** 记该 case 为 fail(error 类别)

#### Scenario: 归一化容差
- **WHEN** 错误消息措辞/标点与金样有差异但语义/`id` 一致
- **THEN** 经归一化判为 pass;`id` 不符判 fail

### Requirement: element-plus 非回归自基线(绝不用 dart-sass 生 golden)

`sasspile` MUST 能端到端编译 `element-plus/packages/theme-chalk`(含 BEM mixin DSL),且随每次核心变更
验证。基准 MUST 是**冻结的自身 `compile` 输出自基线**(非回归语料),MUST NOT 用 dart-sass/其他外部
实现生成 golden(违"唯一参照"铁律)。判据:相对自基线不劣化、无 panic;正确性由 `sass-spec` 簇保证。

#### Scenario: 整库编译不劣化
- **WHEN** 编译 theme-chalk 入口
- **THEN** 输出与其自基线一致或不更差,无 panic

#### Scenario: 禁止外部 golden
- **WHEN** 需要 element-plus 参照
- **THEN** 仅用本实现先前冻结的输出,MUST NOT 引入 dart-sass 产物

### Requirement: 分阶段守门(依赖序 P0–P8)

实现 MUST 按 `design.md` 的 **P0–P8 依赖序**推进;每阶段是经 `compile` 唯一出口可验的纵向增量,以其对应
`sass-spec` 目录簇 hrx 为断言。新增能力 MUST NOT 以"`pub` 内部件 + 只测内部件单测"来验(那是黑盒外的
第二套真相)。接线仅在无歧义处触发(CSS 原生函数/`/ * %` 值跳过、选择器组合子不当比较)。BEM 前置
(P3.6 `!global` / P4 `@content` / P5 `@use as *`·`@forward`)为 element-plus 端到端硬依赖;`@extend`
(P6)与 legacy 全局裸名可后置。

#### Scenario: 阶段增量经出口回归
- **WHEN** 完成一个阶段(如 P1 值/单位/calc)
- **THEN** 其 `sass-spec` 簇经 `compile` 跑出 scoped `regress=0` + `sasslint` 0,不依赖内部件 `pub` 单测

### Requirement: 对外面唯一为 compile

除 `compile`/`SassError` 外,`sasspile` MUST NOT 将 `lexer`/`expr`/`value`/`value_parse`/`structure`/
`eval`/`builtins` 等内部件作为对外 API 暴露;所有验收判据 MUST 只走 `compile` 出口。

#### Scenario: 内部件不作公面
- **WHEN** 审视 crate 公开项与测试入口
- **THEN** 内部件为 `mod` 私有;所有行为断言经 `compile`
