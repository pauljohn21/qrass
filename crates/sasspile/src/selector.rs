//! Selector / PropName —— 规则块头部与属性名作为**结构化值**。
//!
//! 本形状取代的做法把块头渲染成 `String`，再用 `child.replace('&', parent)` 产出嵌套选择器。那让
//! 父引用成了*不透明字符串里的一个字符*，于是关于它的一切问题 —— 有几个 `&`、某个是否粘着
//! 标识符、它是否来自 `#{&}` —— 都只能扫文本回答。这里 `&` 是从 lexer 发出的词素
//! （lexer 的 `STRUCTURAL` 常量）建出的 `Piece::Parent`，嵌套把 pieces 拼接进来：选择器与属性名
//! 是结构化值，合并 `&` 不是字符串替换。
//!
//! 这里建模的正是 `Exec` 按顺序把一条规则放进文档所需的全部。P2 的完整代数（复合/组合子/伪类
//! 解析、`unify`、超选择器）日后在此原地生长，无需重新解析。

use crate::lexer::canon;

/// 选择器头部的一个 piece，按源码顺序。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Piece {
    /// 一段词素 run 解析后的文本；永不含父引用
    Text(String),
    /// 父选择器，来自 `&` **词素**（`#{&}` 不同：语料在那里把父作为文本代入，然后照常嵌套，
    /// 所以它以 `Text` piece 到达）
    Parent,
}

/// 作为 pieces 的选择器。故意不做成解析树：`Exec` 只需要知道父引用落在哪、整体怎么印出。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Selector {
    pieces: Vec<Piece>,
}

impl Selector {
    pub fn from_pieces(pieces: Vec<Piece>) -> Self {
        Self { pieces }
    }

    /// 把 pieces 交回去，给用它们建名字而不是建选择器的调用方。
    pub fn into_pieces(self) -> Vec<Piece> {
        self.pieces
    }

    /// `a & b`、`&:hover`、`&--x`：只要有任何父引用，子选择器就是*替换*它，而不是作为后代
    /// 追加。
    pub fn references_parent(&self) -> bool {
        self.pieces.contains(&Piece::Parent)
    }

    /// 把这条子选择器嵌进 `parent` —— Sass 两条规则的结构形式：持 `&` 的子选择器在每个出现处
    /// 替换父，否则它是后代选择器。
    ///
    /// 在 owned pieces 上做 `flat_map`：不建字符串再搜它，而自身被嵌套过的父已经解析完，所以它的
    /// pieces 不带 `Parent`。空的父（root 层的 `&`）让引用保持未解析，[`Selector::to_css`] 把
    /// 它印成 `&` —— `css/selector/parent::alone/first` 期望的形式。
    pub fn nest(&self, parent: &Self) -> Self {
        if parent.pieces.is_empty() {
            return self.clone();
        }
        if self.references_parent() {
            return Self::from_pieces(
                self.pieces
                    .iter()
                    .flat_map(|piece| match piece {
                        Piece::Parent => parent.pieces.clone(),
                        text => vec![text.clone()],
                    })
                    .collect(),
            );
        }
        Self::from_pieces(
            parent
                .pieces
                .iter()
                .cloned()
                .chain([Piece::Text(" ".to_string())])
                .chain(self.pieces.iter().cloned())
                .collect(),
        )
    }

    /// 头部是否不带任何可供规则印于其下的东西（root scope 的空选择器）。
    pub fn is_empty(&self) -> bool {
        self.to_css().is_empty()
    }

    /// 选择器变成文本的唯一处；[`canon`] 的空白段收束与渲染字符串形式逐字相同，所以嵌套产物不被
    /// 这次重构改变。边缘空白早已不在（头部在 piece 层修剪过），这正是 `a { &.b {} }` 印 `a.b`
    /// 而不是 `a .b` 的原因。
    pub fn to_css(&self) -> String {
        canon(
            &self
                .pieces
                .iter()
                .map(|piece| match piece {
                    Piece::Text(text) => text.clone(),
                    // 只在无父的 `&`（即 root 层的）上可达
                    Piece::Parent => "&".to_string(),
                })
                .collect::<String>(),
        )
    }

    /// 边缘只带空白的头部：pieces 就是选择器本身，周围的填充属于源码布局，不属于名字。在这里
    /// 修剪（而不是拼接之后）正是 `a` 与 `&.b` 能接上成 `a.b` 的原因。
    pub fn trimmed(self) -> Self {
        let blank = |piece: &Piece| match piece {
            Piece::Text(text) => text.chars().all(char::is_whitespace),
            Piece::Parent => false,
        };
        let end = self
            .pieces
            .iter()
            .rposition(|p| !blank(p))
            .map_or(0, |i| i + 1);
        let start = self.pieces.iter().position(|p| !blank(p)).unwrap_or(end);
        let mut pieces = self.pieces;
        let kept = pieces.drain(start..end).collect();
        Self { pieces: kept }
    }
}

/// 属性名，把一条 declaration 需要的那个语义判定**一次、在构造时**做完：它是否是自定义属性
/// （`--x`），其值是原始 CSS 文本而不是 SassScript。
///
/// 调用方问 [`PropName::is_custom`] 而不是重读名字的形状，所以这条律住在结构化 pieces 变成
/// 名字的边界上。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PropName {
    text: String,
    custom: bool,
}

impl PropName {
    /// 从一条 declaration 左侧的 pieces 建名字。`--` 判定跑在规范化文本上，因为前导空白属于
    /// *词素 run*，不属于名字（`  --x: y`）。
    pub fn from_pieces(pieces: &[Piece]) -> Self {
        let text = Selector::from_pieces(pieces.to_vec()).to_css();
        let custom = text.starts_with("--");
        Self { text, custom }
    }

    pub fn is_custom(&self) -> bool {
        self.custom
    }

    pub fn as_str(&self) -> &str {
        &self.text
    }
}
