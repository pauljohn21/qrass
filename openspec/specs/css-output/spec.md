# css-output Specification

## Purpose
定义 CSS 序列化层的可观察行为：expanded 风格的输出树形状、语句顺序、值文本格式化规则。流视角：`reduce_initial(owned acc)` 折叠 CSS 事件流为文本。序列化行为由 sass 规范定义、golden 基线约束验证。

## Requirements

### Requirement: expanded 风格结构
输出 MUST 为 expanded 风格：每条规则选择器独立成行、声明逐行缩进两空格、块间空行分隔；嵌套 MUST 已全部展平；空规则集（无任何声明）MUST 被丢弃（`&` 展开后为空的除外行为由 sass 规范定义并经 golden 验证）。

#### Scenario: 简单规则
- **WHEN** 求值完成得到 `.a { color: red; }`
- **THEN** 输出文本为 `.a {\n  color: red;\n}`（末尾换行）

### Requirement: 顺序保持
规则与声明的输出顺序 MUST 等于源文档（含模块实例化拓扑序、mixin 展开点、控制流迭代序）的语义顺序，MUST NOT 排序或去重相同选择器。

#### Scenario: 同选择器多次出现
- **WHEN** `.a` 在源中出现两次且声明不同
- **THEN** 输出保留两个 `.a` 块，顺序与源一致

### Requirement: 数字文本格式化
数字序列化 MUST 符合 sass 规范：省略小数前导零（`0.5`→`.5` 仅当规范如此要求——expanded 实际保留 `0.5`，以 element-plus golden 基线为准）、去除多余尾零、最多 10 位有效小数、负零规范化为 `0`、非常大的数不进入科学计数法。

#### Scenario: 计算结果格式化
- **WHEN** 声明值为 `math.div(10px, 3)`
- **THEN** 输出 `3.3333333333px`

### Requirement: 颜色文本序列化
颜色 MUST 按 sass 规范序列化：来自 hex 字面量的颜色输出 `#rrggbb`（可缩写为 3 位时仍输出 6 位，以 element-plus golden 基线为准）；带 alpha 输出 `rgba(r, g, b, a)`（逗号风格，与 expanded 基线一致）；CSS 关键字色名保持可识别等价（`red` 输入输出 `red`）；计算产生的颜色输出 hex 或 rgba。

#### Scenario: 半透明计算色
- **WHEN** `rgba(13, 110, 253, 0.5)` 经 color.adjust alpha 后输出
- **THEN** 文本为 `rgba(13, 110, 253, <alpha>)` 逗号风格且 alpha 精度符合数字规则

### Requirement: 注释与特殊值透传
`/* */` 注释 MUST 保留在输出（位置与源一致）；`//` 注释 MUST NOT 出现；`!important` MUST 输出为 ` !important`；插值产生的未引号字符串（如 `calc(...)`、`var(--x)`、数据 URI）MUST 原样透传不二次加工。

#### Scenario: calc 透传
- **WHEN** 属性值为 `calc(100% - #{$sidebar})` 且插值结果含单位
- **THEN** 输出 `calc(100% - 20rem)` 完整原样
