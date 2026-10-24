# scss-diagnostics Specification

## ADDED Requirements

### Requirement: 带位置的编译错误
每一处失败 MUST 是一个 `SassError`，携带错误 kind、消息与源位置（文件 + 字节 span，可解析为
行列），并 MUST 作为 in-stream `Result::Err` 值产出（observable 的 `Err` 通道保持 `Infallible`）。

#### Scenario: 解析错误带位置
- **WHEN** 输入 `a { b: }` 解析失败
- **THEN** 错误指出出错位置并给出与金样匹配的消息

### Requirement: 消息文本匹配金样
渲染出的错误消息 MUST 等于金样 hrx `error` 段的 `Error:` 行（一致性比较以此文本为键），因此措辞、
参数名与值渲染都是契约的一部分。

#### Scenario: 未定义变量的措辞
- **WHEN** 引用未声明的 `$missing`
- **THEN** 消息逐字匹配金样 `Undefined variable` 文本

### Requirement: 首错短路
编译 MUST 在第一个错误停止并返回它，而不累积多个；终端据此把流塌缩为 `Result<String, SassError>`。

#### Scenario: 停在首个失败
- **WHEN** 一个文件里有两处独立错误
- **THEN** 只返回源顺序上的第一个错误
