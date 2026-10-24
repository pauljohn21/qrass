# scss-color Specification

## ADDED Requirements

### Requirement: 跨空间的构造器
`rgb()`/`rgba()`/`hsl()`/`hsla()`/`hwb()`、CSS Color 4 空间语法 `color(<space> …)`、
`lab()`/`lch()`/`oklab()`/`oklch()` 与 `color.scale/change/adjust` MUST 产出颜色和=该用例金样
`output.css` 一致的序列化形态与通道语义（含 legacy 逗号形态与 modern 空格分隔形态）。

#### Scenario: hex 直通
- **WHEN** `a { c: #ff0000 }`
- **THEN** 发射该颜色的金样序列化

#### Scenario: 通道 change 保持在原空间
- **WHEN** `color.change(red, $alpha: 0.5)`
- **THEN** 输出匹配金样 `rgba(255, 0, 0, 0.5)`

### Requirement: 跨空间转换精度
颜色空间转换（sRGB↔linear↔XYZ-D65↔D50↔Lab/LCh/OKLab/OKLch）MUST 复现金样数值（按金样精度四舍
五入），任何不符 MUST 当作实现 bug 对着金样修正——而不是照搬其它引擎的常量。

#### Scenario: to-space 往返
- **WHEN** 对 `to_space` 金样执行 `color.to-space($c, lch)`
- **THEN** lch 数值等于金样值

### Requirement: 色域映射与通道访问
`color.is-in-gamut`、`color.to-gamut`（minde/裁剪按金样）与 `color.channel` MUST 匹配金样的判定与
数值，含 `none` 通道（缺失）处理——缺失通道既非具体 0 也非报错，除非金样如此规定。

#### Scenario: 缺失通道可参与计算
- **WHEN** 某空间颜色的 `none` 通道被用于运算
- **THEN** 行为遵循金样（当作可计算，而非字符串直通）
