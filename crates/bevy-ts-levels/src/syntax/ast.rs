//! The tree of a `.bsn` file. Every node keeps its byte span; the tokens beside it keep the
//! trivia, so [`ParsedFile::print`] gives the input back and the writer splices by span.

use super::lex::Token;
use super::{ExtensionUse, Span};

/// A parsed file: its source, its tokens and its top-level entities.
#[derive(Debug, Clone)]
pub struct ParsedFile {
    pub source: String,
    pub tokens: Vec<Token>,
    pub comments: Vec<Span>,
    /// The file's top-level entities, separated by `--`.
    pub entities: EntityList,
    /// Every use of an extension, in file order.
    pub extensions: Vec<ExtensionUse>,
}

impl ParsedFile {
    /// The file, printed from its tokens: trivia then text, token by token.
    pub fn print(&self) -> String {
        let mut out = String::with_capacity(self.source.len());
        for t in &self.tokens {
            out.push_str(&self.source[t.trivia.start..t.span.end]);
        }
        out
    }

    pub fn text(&self, span: Span) -> &str {
        span.text(&self.source)
    }

    /// The index of the token that starts at `offset`.
    pub fn token_at(&self, offset: usize) -> Option<usize> {
        let i = self.tokens.partition_point(|t| t.span.start < offset);
        (self.tokens.get(i)?.span.start == offset).then_some(i)
    }

    /// The trivia before the token starting at `offset`: the whitespace and comments a deletion of
    /// that node takes with it.
    pub fn trivia_before(&self, offset: usize) -> Span {
        self.token_at(offset).map(|i| self.tokens[i].trivia).unwrap_or(Span::new(offset, offset))
    }
}

/// Entities separated by `--`: a file's top level or a relationship list's body.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct EntityList {
    pub items: Vec<Entity>,
    /// The `--` spans; `separators[i]` follows `items[i]`.
    pub separators: Vec<Span>,
}

/// One entity: its entries in order.
#[derive(Debug, Clone, PartialEq)]
pub struct Entity {
    /// From its first entry's start to its last entry's end; empty at its position if it has none.
    pub span: Span,
    pub entries: Vec<Entry>,
}

impl Entity {
    /// The entity's `#Name`, if it has one (the last one, if it has several).
    pub fn name(&self) -> Option<&NameEntry> {
        self.entries.iter().rev().find_map(|e| match e {
            Entry::Name(n) => Some(n),
            _ => None,
        })
    }

    /// The entity's inclusion: its first entry after its name, if that is `:"…"` or `@Prefab`.
    pub fn inclusion(&self) -> Option<&Entry> {
        self.entries.iter().find(|e| !matches!(e, Entry::Name(_))).filter(|e| matches!(e, Entry::Include(_) | Entry::Prefab(_)))
    }

    /// The entity's relationship list named `relationship` (`Children`), by its last path segment.
    pub fn related(&self, relationship: &str) -> Option<&Related> {
        self.entries.iter().find_map(|e| match e {
            Entry::Related(r) if r.path.last() == relationship => Some(r),
            _ => None,
        })
    }

    pub fn patches(&self) -> impl Iterator<Item = &Patch> {
        self.entries.iter().filter_map(|e| match e {
            Entry::Patch(p) => Some(p),
            _ => None,
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Entry {
    /// `#Name`, or E4 `#"quoted"`.
    Name(NameEntry),
    /// `:"path.bsn"`, a cached inclusion of another level.
    Include(Include),
    /// E2 `@path::Comp { @prop: v, field: v }`: a scene component instance (a placeable prefab).
    Prefab(Prefab),
    /// `path::Type`, `Type(a, b)` or `Type { f: v }`.
    Patch(Patch),
    /// `Children [ … ]` or any relationship target.
    Related(Related),
}

impl Entry {
    pub fn span(&self) -> Span {
        match self {
            Entry::Name(n) => n.span,
            Entry::Include(i) => i.span,
            Entry::Prefab(p) => p.span,
            Entry::Patch(p) => p.span,
            Entry::Related(r) => r.span,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct NameEntry {
    pub span: Span,
    pub name: String,
    pub quoted: bool,
    /// The identifier or string after `#`.
    pub name_span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Include {
    pub span: Span,
    pub path: String,
    pub path_span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Prefab {
    pub span: Span,
    pub path: Path,
    /// Named fields, every one a prop (`@hp: 250.0`), or unit.
    pub fields: Fields,
}

/// A type with fields: a component patch as an entry, or a nested value.
#[derive(Debug, Clone, PartialEq)]
pub struct Patch {
    pub span: Span,
    pub path: Path,
    pub fields: Fields,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Related {
    pub span: Span,
    pub path: Path,
    pub open: Span,
    pub close: Span,
    pub list: EntityList,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Path {
    pub span: Span,
    pub leading_colon: bool,
    pub segments: Vec<String>,
}

impl Path {
    /// `a::b::C`, without a leading `::`.
    pub fn joined(&self) -> String {
        self.segments.join("::")
    }

    pub fn last(&self) -> &str {
        self.segments.last().map(String::as_str).unwrap_or("")
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Fields {
    Unit,
    Named { open: Span, close: Span, fields: Vec<Field>, trailing_comma: bool },
    Tuple { open: Span, close: Span, values: Vec<Value>, trailing_comma: bool },
}

impl Fields {
    pub fn named(&self) -> &[Field] {
        match self {
            Fields::Named { fields, .. } => fields,
            _ => &[],
        }
    }

    pub fn get(&self, name: &str) -> Option<&Field> {
        self.named().iter().find(|f| f.name == name)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Field {
    /// From the name (or its `@`) to the value's end.
    pub span: Span,
    /// Written `@name`: a prefab prop.
    pub prop: bool,
    pub name: String,
    pub name_span: Span,
    pub value: Value,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Value {
    pub span: Span,
    pub kind: ValueKind,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ValueKind {
    Float(f64),
    Int(i128),
    Bool(bool),
    Str(String),
    Type(Patch),
}
