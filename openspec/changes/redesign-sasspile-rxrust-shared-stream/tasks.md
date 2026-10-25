# Tasks — 按 design.md 的 P0–P8 阶段(依赖序;每阶段经唯一出口 `compile` 验 `regress=0`)

> 阶段可跨多次提交推进;每阶段以对应 `sass-spec` 目录簇 hrx 经 `compile` 跑出 `regress=0` + `sasslint` 0 为门。
> "已做" = 本 change 之前的探索性未提交工作中已完成者(见 design「现状处置」)。
>
> **regress 口径(阶段式必需)**:`ledger diff` 的 `regress=0` **按目录簇 scoped 判定**——新阶段刚铺开、
> 尚未实现的域(如 P1 初期 `values/calculation/**` 全 fail)**不计入** regress;仅要求**已过某簇在其后续
> 变更中不回退**。即:每簇维护自己基线,阶段只在自己负责的簇上求 `regress=0` 且单调上升。
>
> **语料驱动排序(见 design「element-plus 真实语料分析」)**:element-plus 端到端主干 =
> **P1(map/插值/单位)+ P3 → P4(mixin/function)→ P5(模块 `@use as *`)→ P7(custom property)**。
> **P6(@extend,真实仅 5 次)、P1.7 legacy 全局裸名、`@while`/`@supports` 对 element-plus
> 非阻塞 → 后置**。custom property 内 `v-bind()`(Vue 构建期)不纳入 sasspile。
>
> **BEM 是 EP 主干(非阻塞项修正)**:element-plus 靠 `b()/e()/m()/when()` mixin DSL 生成 BEM,
> 硬前置 = **`!global` 环境态(3.6)+ mixin `@content` 携选择器上下文(4.1)+ `@use … as *`/`@forward`
> 链(5.1/5.2)+ 选择器插值(2.1)**。⇒ **P3.6/P4/P5 是编译 element-plus 的硬前置;`@forward` 不是后置项**。
> 真正可后置:仅 **P6 @extend** 与 **P1.7 legacy 裸名**。

## P0 — hrx harness(验收安全网)
- [~] 0.1 `tests/hrx.rs` 经 `compile` 跑 `sass-spec` 安全子集、产 `ledger` 快照 + 立基线(seed 560/11486)。**已具雏形,待补 0.2/0.3**
- [ ] 0.2 harness 支持**多文件输入**(partial `_x.scss`/相对 import)与目录型用例
- [ ] 0.3 harness 解析 `options.yml`:尊重 `:todo:`(skip/xfail,不入基线)、`precision`、`--indented` 语法
- [ ] 0.4 harness 断言 `error`/`warning`/`deprecation` 期望(按 P7 诊断范围决定是否计入)

## P1 — 值地基:单位代数 + calculation + 全色彩空间 + list/map
- [~] 1.1 `Value`(带单位 Number/引号态 String/Color/List/Map/Bool/Null)+ `to_css` 规范化。**已有基础,缺单位换算/calc/CSS4 色**
- [x] 1.2 `Expr` 折叠 + 惰性 `eval(owned Env)`;算术/比较/布尔;`/ * %` 跳过、CSS 原生函数保留。**已做(未接入 compile 值路径)**
- [ ] 1.3 **单位换算代数**(length/time/angle/…)+ 精度(`values/numbers/**`)
- [ ] 1.4 **`Calculation` 表达式树**(`calc/clamp/min/max/var()`,可部分化简、否则原式保留)(`values/calculation/**`)
- [ ] 1.5 **CSS Color 4 空间基础**(rgb/hsl/hwb + 通道访问;`Color` 全空间留 P8)(`values/colors/**`、`core_functions/color/**`)
- [ ] 1.6 结构化 token→`Expr` 求值**接入 `compile` 声明值路径**(白名单 + 无歧义触发),经 `core_functions/**` 与 `operators/**` 金样验 `regress=0`
- [ ] 1.7 内置函数各域分派接线(math/string/list/map/color/meta/selector),经 `core_functions/**` 验。**全局裸名 `global/*` deprecation 部分可后置**(真实几乎不用,见语料分析)

## P2 — 结构化 Selector 模型
- [ ] 2.1 `Selector`(compound/combinator/伪类)parse/序列化/`&`(`css/selector/**`、`core_functions/selector/**`)
- [ ] 2.2 unify / superselector / `selector.nest/parse/unify/is-superselector/simple-selectors`

## P3 — 管线 + 控制流 + 作用域 + 媒体/at-root
- [x] 3.1 `async fn compile` 唯一出口;`Shared::from_stream(root.body)` + `scan_map(owned (env,taken))` + `flat_map(Shared::from_iter)` + `reduce_initial` + `into_future().await`;删 `parse::Acc`/`render.rs`。**已做,tests 全绿**
- [~] 3.2 `@if/@else`(filter + 不可变 taken 链)、`@each`/`@for`(flat_map)。**已做 `@each/@for/@if`;缺 `@while`**
- [ ] 3.3 `@while` 条件循环(`directives/while/**`)
- [ ] 3.4 **媒体/supports 冒泡 + 逐查询合并**(全局 pass;`css/media/**`)
- [ ] 3.5 `@at-root (without/with)`(`directives/at_root/**`)
- [ ] 3.6 变量块作用域 + `!global`/`!default`(`variables/**`、`libsass/variable-scoping/**`)

## P4 — 可调用:mixin / function / @content / 参数绑定
- [ ] 4.1 `@mixin`/`@include`/`@content`/`@yield` + 参数(位置/默认/`$r...`/named-splat)绑定 + 动态作用域;注册表 + 一到多多播(`directives/mixin/**`、`callable/**`)
- [ ] 4.2 `@function`/`@return` + 用户函数求值 + `meta.get-function/call`(`directives/function/**`、`callable/**`)

## P5 — 模块系统(唯一真并发处)
- [ ] 5.1 加载器(partial/index/扩展名)+ `@use`(命名空间/`as *`/`with`+`!default`)(`directives/use/**`)
- [ ] 5.2 `@forward`(show/hide/as/with 透传)(`directives/forward/**`)。**BEM 前置**:theme-chalk 内部
  (`mixins.scss` `@forward 'config'/'function'/'_var'`)必须能解析
- [ ] 5.3 legacy `@import`(弃用路径)+ CSS import 排序(`directives/import/**`)
- [ ] 5.4 独立模块**并发编译** + order 键稳定合并 + `ref_count`/`behavior_subject` 多播;跨线程只 `Arc<只读>`
- [ ] 5.5 `meta.load/load-css/get-module/css`(module 为一等 `Value`;`core_functions/meta/**`)

## P6 — @extend(依赖 P2 选择器代数;**可后置,element-plus 不卡此**)
- [ ] 6.1 `@extend` 图 + 链式不动点 + `!optional` + 超选择器裁剪 + 选择器伪类并集(`directives/extend/**`);`:todo: dart-sass` 例作 skip

## P7 — 原生 CSS 透传 + 诊断
- [ ] 7.1 未知 at-rule/`@charset`/`@font-face`/`@propset`/`@keyframes`/custom property(`--*`,`var()`)/unicode-range/`!important`/转义 原样透传(`css/**`)
- [ ] 7.2 `@debug`/`@warn`/`@error` + deprecation:**精确诊断格式**(双源 trace:invocation+declaration);诊断**已定为 v1 纳入**(见已定决策),error/warning 走 scoped 基线

## P8 — CSS Color 4
- [ ] 8.1 全色彩空间(lab/lch/oklab/oklch/srgb/display-p3/…)adjust/change/construct + **gamut mapping**(`core_functions/color/**`)

## 收尾(每阶段末与整体)
- [ ] 9.1 `element-plus` theme-chalk 端到端编译(**经 P1/P3/P4/P5/P7 主干即应达成,不依赖 P6**),无 panic、输出与自基线一致不劣化(`ledger` 归一化)
- [ ] 9.2 全量 `sass-spec`(排除 `non_conformant`/`libsass-*`/`:todo:`)通过率相对基线单调上升、`regress=0`;`sasslint` 0;clippy `-D warnings` 绿
