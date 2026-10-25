# redesign-sasspile-rxrust-shared-stream

把 sasspile 重设计为**流算子 + 全局 pass 的混合式分层管线**(全语言框架设计:值/单位/calc、selector/extend、callable、模块 use/forward/import、core_functions、css at-rules、directives、作用域、诊断)。真并发仅模块加载(order 键保序),入口 `async fn compile` 唯一出口,终点 `into_future().await`;按 P0–P8 阶段实现,行为唯一参照 sass-spec + element-plus。
