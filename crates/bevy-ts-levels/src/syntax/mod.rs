//! The `.bsn` syntax bevy-ts reads and writes: upstream `bevy_bsn`'s file grammar plus four named
//! extensions (E1 comments, E2 `@Comp { @prop: v, field: v }`, E3 child overrides by name, E4 quoted
//! names). The parse is lossless: every byte belongs to one token, as its text or as the
//! whitespace and comments before it, and every node keeps its byte span, so the writer edits a
//! file by splicing text and never re-emits it.

pub mod ast;
pub mod lex;
pub mod parse;

pub use ast::*;
pub use parse::parse;

/// A byte range in a file's source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

impl Span {
    pub fn new(start: usize, end: usize) -> Self {
        Span { start, end }
    }

    /// The span from `self`'s start to `other`'s end.
    pub fn to(self, other: Span) -> Span {
        Span { start: self.start, end: other.end }
    }

    pub fn text(self, src: &str) -> &str {
        &src[self.start..self.end]
    }

    pub fn range(self) -> std::ops::Range<usize> {
        self.start..self.end
    }
}

/// 1-based line and column (in chars) of a byte offset.
pub struct LineIndex {
    starts: Vec<usize>,
}

impl LineIndex {
    pub fn new(src: &str) -> Self {
        let mut starts = vec![0];
        starts.extend(src.match_indices('\n').map(|(i, _)| i + 1));
        LineIndex { starts }
    }

    pub fn line_col(&self, src: &str, offset: usize) -> (u32, u32) {
        let line = self.starts.partition_point(|&s| s <= offset) - 1;
        let col = src[self.starts[line]..offset.min(src.len())].chars().count();
        (line as u32 + 1, col as u32 + 1)
    }

    /// The 1-based line of a byte offset.
    pub fn line(&self, offset: usize) -> u32 {
        self.starts.partition_point(|&s| s <= offset) as u32
    }
}

/// An error at a place in a file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub message: String,
    pub span: Span,
}

impl Diagnostic {
    pub fn new(message: impl Into<String>, span: Span) -> Self {
        Diagnostic { message: message.into(), span }
    }

    /// `file:line:col: message`.
    pub fn render(&self, file: &str, src: &str) -> String {
        let (line, col) = LineIndex::new(src).line_col(src, self.span.start);
        format!("{file}:{line}:{col}: {}", self.message)
    }
}

/// What a file uses beyond upstream's file grammar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Extension {
    /// E1: `//` and `/* */` comments.
    Comments,
    /// E2: a scene component instance `@path::Comp { @prop: v, field: v }`.
    Prefab,
    /// E3: a `#Name` child of an inclusion, which merges with the included child of that name.
    ChildOverride,
    /// E4: a quoted name `#"…"`.
    QuotedName,
    /// Not an extension: a negative number literal. Upstream's file lexer at #26043 lexes `-4.0`
    /// as `-` then `4.0` and its parser rejects it; the `bsn!` macro accepts it, so bevy-ts does.
    NegativeNumber,
}

impl Extension {
    /// `E1` to `E4`, or `U1` for the upstream lexer gap.
    pub fn code(self) -> &'static str {
        match self {
            Extension::Comments => "E1",
            Extension::Prefab => "E2",
            Extension::ChildOverride => "E3",
            Extension::QuotedName => "E4",
            Extension::NegativeNumber => "U1",
        }
    }

    pub fn describe(self) -> &'static str {
        match self {
            Extension::Comments => "comment",
            Extension::Prefab => "scene component instance",
            Extension::ChildOverride => "child of an inclusion, merged by name",
            Extension::QuotedName => "quoted name",
            Extension::NegativeNumber => "negative number (upstream's file lexer rejects it)",
        }
    }

    /// Whether upstream's file parser rejects a file using this. E3 parses upstream; it means
    /// something different there (an appended child rather than a merged one).
    pub fn rejected_upstream(self) -> bool {
        !matches!(self, Extension::ChildOverride)
    }
}

/// One use of an [`Extension`], at its place in the file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExtensionUse {
    pub extension: Extension,
    pub span: Span,
}
