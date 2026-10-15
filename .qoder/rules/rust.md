---
trigger: always_on
alwaysApply: true
---
## 
技术栈 - Rust edition 2024, toolchain 1.99 - 所有对话用中文回复，代码、命令、文件名保持 English - 统一使用rxrust为核心,使用响应式编程风格
 ## 
 ⛔ 禁止项（违反 = 任务失败） 禁止使用GC思维,必须使用rust所有权 
 1. **禁止 Python**：不使用 python3/pip;不使用任何脚本,统一使用rust; 测试用 #[test]
 2. **禁止 println!/eprintln!**：所有代码（含 src/ 和 tests/）一律用 tracing 宏(info!/warn!/error!/debug!)
 3. **必须使用 tracing span**：涉及跨函数/跨阶段的管道处理必须用 tracing::span!(info_span!/debug_span!/trace_span!)记录上下文与耗时，利用 entered/exit 边界追踪调用链，禁止仅用 event! 单一日志。字段命名遵循 stage、module、id、elapsed_ms 等约定 
 4. **禁止内联测试**:src/ 保持纯生产代码，所有测试放在 tests/ 目录 
 5. **单文件 ≤ 500 行**：源码和测试分别计算，超出必须拆分 
 6. **使用 SSH 方式推送 GitHub**
  ## 
🔬 调试协议（强制 — 违反 = 任务失败） **核心原则**：禁止凭直觉猜测根因。所有 bug 修复必须基于 tracing trace 证据链。 ### 4步流程(不可跳过): 1. SPAN 插桩：疑似路径每个入口/出口加 span 2. TRACE 采集:RUST_LOG=trace 收集证据 3. 根因定位：必须引用具体 span + 字段值 4. 修复验证：修复后清理临时 span ## 🔍 代码查询 - 优先使用 CodeGraph CLI(callers/impact/explore/node) ## 🔄 Git 规范 - 推送:SSH 方式 git push github main - 只提交不推送:commit 后必须等用户确认