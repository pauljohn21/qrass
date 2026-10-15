# Tasks

## 1. 脚手架与流水线骨架

- [x] 1.1 创建 `crates/qrass/Cargo.toml`（lib+bin，`rxrust = "=1.0.0-rc.5"`、`tracing`、`tracing-subscriber`，edition 2024），验证：`cargo build -p qrass` 通过且 Cargo.lock 不新增顶层依赖
- [x] 1.2 实现 `src/error.rs`（SassError：文件/Span/message 枚举）与 `src/span.rs`（BytePos/LineCol），验证：`tests/error.rs` 断言错误可携带行列并可 Display
- [x] 1.3 实现 `src/tracing_setup.rs`（subscriber init）与 `src/main.rs` CLI（input/`-o`/`--style` 解析、style≠expanded 报 unsupported、退出码），验证：`cargo run --bin qrass -- tests/fixtures/passthrough.scss` 输出与输入等价的纯 CSS（骨架期直通）
- [x] 1.4 实现 `src/lib.rs` 的 `compile()` 编排骨架：load→lex→parse→eval→render 五阶段占位函数，各以 `tracing::span!(INFO, stage=..., module=..., id=...)` 包裹并记录 elapsed_ms，验证：`RUST_LOG=info cargo run --bin qrass -- <fixture>` 在 stderr 出现 5 个 stage span 且 stdout 仅 CSS

## 2. 词法分析（rxrust Token 流）

- [x] 2.1 定义 `src/token.rs`（Token 枚举：ident/var/number/dimension/color hex/named-color/string/punct/at-keyword/hash-brace 插值段/comment 等 + Span），验证：类型编译通过，无业务逻辑
- [x] 2.2 实现 `src/lexer.rs`：`Local::create(&mut Emitter<Result<Token,E>>)` 逐 token 发射（含 `#{}` 内部递归词法、`//` 与 `/* */` 注释区分），排空为 `Vec<Token>`，验证：`tests/lexer.rs` 覆盖嵌套引号、转义、CRLF、多行注释、`$v: 1px + 2` 序列等 ≥10 个 case
- [x] 2.3 词法错误用例：未闭合字符串/注释/插值报 SassError 且含精确行列，验证：`tests/lexer.rs` 错误分支断言行号

## 3. 语法分析（语句 AST）

- [x] 3.1 定义 `src/ast.rs` 语句节点（Ruleset/Declaration/Variable/Each/For/If/While/Mixin/Include/Function/Return/Content/Use/Forward/Import/AtRoot/Media/Supports/Keyframes/Extend/Warn/Error/Debug/Comment/AtRule 兜底），验证：编译通过且 ≤500 行
- [x] 3.2 实现 `src/parser.rs` 语句层：`reduce_initial(ParseState)` 消费 Token 流产出 `Vec<Statement>`；规则集/声明/选择器原文段（含 `&`、`#{}` 序列）/`@mixin`/`@include`（含 `@content` 块与 using 形参）/`@if...@else` 链/`@each`（多变量）/`@for`/`@while`/`@media`/`@supports`/`@at-root`（with/without）/`@keyframes`/`@use`/`@forward`/`@import` 文法，验证：`tests/parser.rs` ≥20 个 fixture 断言 AST 形状（用 Debug 快照比较）
- [x] 3.3 语法错误定位：缺 `}`、缺 `;`、空声明值等在首错处中止并报行列，验证：`tests/parser.rs` 错误分支 ≥5 case

## 4. 语法分析（表达式 AST）

- [x] 4.1 定义 `src/ast_expr.rs` 表达式节点（Literal/VariableRef/FunctionCall{ns}/Binary{op}/Unary/Interp/Map/List(分隔符)/Paren/BinOp 优先级含 and/or/==/</+/-/、*/%），验证：编译通过
- [x] 4.2 实现 `src/parser_expr.rs`：token 流上的优先级爬升解析 + 实参表（位置/命名 `$k: v`/spread `...`），验证：`tests/parser_expr.rs` 覆盖 `1px + 2`、`(a: 1, b: 2)`、`map.get($m, "k")`、`if($a, $b, $c)` 三元、`not true`、字符串插值混排等 ≥15 case

## 5. 值模型与最小求值

- [x] 5.1 实现 `src/value.rs`：SassValue（Null/Bool/Number{f64,unit}/String{quoted}/Color{rgba+hsl 双空间}/List{sep}/Map/Error 转换规则），验证：`tests/value.rs` 断言真值表（null/false 假、0 真）与 color↔hsl 互转
- [x] 5.2 实现 `src/env.rs` 块作用域链（map + parent 索引，无 Arc<Mutex>）与 `!default`/`!global` 语义，验证：`tests/eval.rs` 遮蔽/默认/全局写回 3 组 case
- [x] 5.3 实现 `src/eval.rs`+`src/eval_stmt.rs`+`src/eval_expr.rs` 核心：变量声明/引用、规则集嵌套展平（`&` 替换）、声明输出、数字带单位算术与换算表（px/in/cm/%…），以 `from_iter+scan_map(&mut EvalState)` 管道排空出输出节点树，验证：`tests/eval.rs` 嵌套/算术/单位 case 全绿

## 6. 模块系统（@use/@forward）

- [x] 6.1 实现 `src/loader.rs`：canonicalize（原样/`.scss`/`.css`/`_`前缀/目录 index 次序）+ load + `.css` 透传为 `@import` 语句节点，验证：`tests/scss_modules.rs` partial/index/css 透传/找不到路径候选列表 4 case
- [x] 6.2 实现 `src/module.rs`：模块身份=canonical path 的 arena 存储、一次实例化、@use 命名空间（默认名/`as`/`as *`）、私有成员（`$-`/`@mixin -`/`@function -`）跨模块不可见，验证：`tests/scss_modules.rs` 断言同一文件双 @use 仅执行一次、私有变量报 undefined
- [x] 6.3 实现 `@forward`（show/hide/`as prefix*`）与 `@use ... with`（仅 `!default` 可配置、未消费配置报错），验证：`tests/scss_modules.rs` element-plus `mixins/_config` 式转发链 + with 正反 case
- [x] 6.4 循环 @use 检测与 SCSS `@import` 拒绝（unsupported 错误），验证：`tests/scss_modules.rs` a→b→a 环错误含文件列表

## 7. mixin/function/控制流/extend/at-root

- [x] 7.1 `@mixin`/`@include` 完整实参机制（默认值/命名/`@content` lexical scope/`$args...` 转发），验证：`tests/eval.rs` element-plus `@include b(button)` 式块 mixin ≥6 case
- [x] 7.2 `@function`/`@return`（词法作用域捕获、递归、缺省返回 null、命名空间遮蔽规则），验证：`tests/eval.rs` 含 `calc-main-color` 式多层调用 case
- [x] 7.3 `@if/@else if/@else`、`@each`（list/map 解构）、`@for`(to/through)、`@while` 展开，验证：`tests/eval.rs` 对应循环各 ≥2 case（map 解构、through 边界）
- [x] 7.4 `@media`/`@supports` 嵌套合并（`and` 连接+上浮规则）、`@at-root`（默认脱离/with 保留）、`@keyframes`，验证：`tests/eval.rs` design 场景逐条覆盖
- [x] 7.5 placeholder `@extend` 基本并入（同模块、`!optional`、插值 placeholder 名、空 `%sel` 不输出），验证：`tests/eval.rs` 复刻 element-plus `share-rule`/`extend-rule` 双 mixin 场景

## 8. 内置函数注册表（双分发）

- [x] 8.1 实现 `src/builtins/mod.rs`：namespace+global 双表、name 归一化（`map.get`↔`map-get`）、参数绑定/arity 错误信息，验证：`tests/builtins.rs` 断言未注册函数报 undefined function 含模块名
- [x] 8.2 `map.*`+`math.*`+`color.*` 全集（按 D6 优先级：map get/merge/deep-merge/keys/values/has-key/set/remove；math div/round/ceil/floor/min/max/abs/percentage/pow/sqrt/log + $pi/$e；color channel/adjust/scale/change/mix/alpha/opacity/ie-hex-str/rgb/hsl），验证：`tests/builtins.rs` 每函数 ≥1 case 且 `math.div(1,3)` 序列化为 `0.3333333333`
- [x] 8.3 `string.*`/`list.*`/`selector.*`/`meta.*`（ins 高频 + sass-spec 常见：type-of/inspect/variable-exists/mixin-exists/keyword-gets、unquote/slice/index/length、append/join/nth/index/separator、nest/parse/simple-selectors），验证：`tests/builtins.rs` 对应 case 全绿

## 9. CSS 序列化（expanded）

- [x] 9.1 实现 `src/css.rs`：输出节点树→expanded 文本（两空格缩进、块序=源序、空规则丢弃、`/* */` 保留 `//` 丢弃、`!important`），验证：`tests/css_output.rs` 快照断言
- [x] 9.2 数字/颜色格式化器（10 位有效、去尾零、无指数、负零归一；hex 6 位/rgba 逗号风格/关键字保持），验证：`tests/css_output.rs` 数值边界 case（1/3、0.0、-0.0、1e21、#fff→#ffffff）

## 10. element-plus 端到端与 golden

- [x] 10.1 编译 `element-plus/packages/theme-chalk/src/index.scss` 直到零错误（ 期间按失败 span 反哺 2–9 各模块修复），验证：CLI 产出非空 CSS
- [x] 10.2 实现 `tests/common/css_validate.rs`（括号配对/声明分号/选择器非空轻量校验）与组件覆盖断言（每个 `src/*.scss` 的 `.el-*` 顶层选择器出现），验证：`tests/element_plus_e2e.rs` 两门槛均绿
- [x] 10.3 golden 基线：用 element-plus 官方工具链（sass-embedded/dart-sass 1.105.1，`pnpm -C packages/theme-chalk build` + `sass --style=expanded`）生成并固化 `crates/qrass/tests/golden/element-plus/index.css`（expanded 416K，主对比件）+ `dark/css-vars.css` + `dist/`（122 个官方 min 件）；基线为静态数据，qrass 实现与测试仍纯 Rust（不用脚本）
- [x] 10.4 回归测试接入 `tests/common/css_diff.rs`（两侧经同一 lightningcss 序列化器规范化后再归一化空白、首偏差行报告）对比 10.3 golden，与组件覆盖断言一同挂 `tests/element_plus_e2e.rs`，验证：`cargo test -p qrass element_plus` 二次运行逐行一致（跳过 dart-sass 换行怪癖，lightningcss 终态比对，语义零偏差）

## 11. sass-spec hrx 运行器

- [x] 11.1 实现 `tests/common/hrx.rs`：解析 `<===> path` 块（input.scss/output.css/options.scss/errors）、目录递归发现，验证：对 sass-spec 任一单用例文件解析出正确的 input/output 段（`tests/hrx_parse.rs` 内联三段 + 真实 `css/percent.hrx` 均绿）
- [x] 11.2 实现 `tests/hrx_spec.rs` runner：遍历 `spec/{core_functions,expressions,directives,callable,css}`，`:legacy`/libsass-only 目录按规则 skip，`allowlist.txt` 外全部必须通过，info! 汇总通过率，验证：本地运行输出通过率且失败清单非空时可见首条原因。附带：per-case 多文件临时物化（保 `@use` partial）+ 反哺实现 Sass `--load-path`（`Options.load_paths`→pipeline→`canonicalize_with`，spec-root 加载路径使 `@use "core_functions/color/utils"` 可解析，load error 673→64）；`canonicalize` 以 `catch_unwind` 兜底 lightningcss 对畸形色值的 panic
- [ ] 11.3 迭代收敛：按失败聚类修复编译器（每轮 ≥1 类），更新 allowlist 仅限真实 legacy-only 用例，验证：modern 子集通过率 ≥95% 且 `cargo test -p qrass` 全绿。**进度**：round-1 已修 load-path 类；全量基线 12.04%（passed 1320 / failed 9641，indented 跳过 332），回归地板固化于 `tests/golden/sass-spec/min_pass_rate.txt`。剩余债务高度集中于「现代 CSS Color 4」（`color.<space>()` 构造 1188 + `.to-space` 色域转换矩阵 2544 + gamut 谓词）与 `selector.*` 选择器代数（~700），后续轮次逐类收敛并抬升地板

## 12. 集成门禁

- [ ] 12.1 全量回归：`cargo test -p qrass`（离线、无外部工具）+ `cargo clippy -- -D warnings` 通过，验证：CI 等价命令本机全绿
- [ ] 12.2 项目规范审计：src/ 与 tests/ 无 `println!`/`eprintln!`/内联 `mod tests`、全部源文件 ≤500 行，验证：grep 与 `wc -l` 脚本化检查记录于 PR 描述
