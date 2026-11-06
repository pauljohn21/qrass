//! Env —— `Exec` 内核携带的那个 **owned** 词法环境。它是 [`crate::exec::State`]（`{ scopes, env }`）的一半，
//! 后者住在内核的观察者里并穿过 `&mut self` 往前移动；它从来不是一个句柄后面的共享可变态，而表达式求值
//! 只会借用它。
//!
//! P1 只保留一层扁平结构，那正是一个顶层 `$var: value` 绑定所需要的。块作用域、`!global`、`!default` 与
//! `@mixin`/`@function` 本体的动态作用域都是 `directives` 相的工作（tasks 3.6/4.1），它们会把这类型长成一个
//! 作用域栈；查找键现在就定死了，以免那几个相各自漂移：**一个不带 `$` 的名字**。

use std::collections::HashMap;

use crate::value::Value;
use crate::{ErrorKind, SassError};

#[derive(Debug, Clone, Default)]
pub struct Env {
    vars: HashMap<String, Value>,
}

impl Env {
    /// 在当前层绑定 `$name: value`（以后的相会先压一层）。这个 env 只能经流携带的那个 `State` 值拿到，
    /// 所以这次改的是那个值已经拥有的数据 —— 从来不是一个句柄背面的共享态。
    pub fn bind(&mut self, name: &str, value: Value) {
        self.vars.insert(name.to_string(), value);
    }

    /// 查一个变量。一个未绑定的名字被定成 Sass 的**未定义变量**错误，好让 `Exec` 相自己定 —— 按 kind 对
    /// crate 根部那个 `FATAL` 集合 —— 它是打断流还是让那条 declaration 保住自己的逐字文本。
    ///
    /// 消息是语料的那个**错误头**，`Error: Undefined variable.`
    /// （例：`directives/use/error/member/missing.hrx::namespaced/variable_use`、
    /// `css/comment.hrx::error/loud/interpolation/failure`）。它的覆盖面**取决于口径**，两个数都现量在此：
    /// `grep -rho "Error: Undefined variable." sass-spec/spec/ --include='*.hrx'` 全仓（含 `non_conformant`）
    /// 交出 **23 次 / 15 个文件**；按 `crates/sasspile/tests/hrx.rs:24` 的政策排除 `non_conformant` 后是
    /// **20 次 / 12 个文件**。两种口径下都成立的是：只交出**一种**写法，而 23/20 次都**不带**变量名。
    /// 名字不是那个头的一部分：在 Sass 里它是在头下面的*源码 span* 上读出来的，那由 P7 7.2 加上。
    /// 字面上下文根本不应该到这里来查（P0′.3b）。
    pub fn get(&self, name: &str) -> Result<Value, SassError> {
        self.vars
            .get(name)
            .cloned()
            .ok_or_else(|| SassError::typed(ErrorKind::UndefinedVariable, "Undefined variable."))
    }
}
