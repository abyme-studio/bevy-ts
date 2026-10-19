//! The recursive-descent parser. Its shape follows upstream's (`bevy_bsn/src/parse.rs`) rule for
//! rule, so an extension-free file parses to the same tree; the four extensions are the marked
//! branches, and every other construct of the `bsn!` macro is refused by name.

use super::lex::{Kind, Token, lex, unescape};
use super::{Diagnostic, Entity, EntityList, Entry, Extension, ExtensionUse, Field, Fields, Include, NameEntry, ParsedFile, Patch, Path, Prefab, Related, Span, Value, ValueKind};

/// Parses a `.bsn` file. The top level is a list of entities separated by `--`, as a relationship
/// list's body is.
pub fn parse(src: &str) -> Result<ParsedFile, Diagnostic> {
    let (tokens, comments) = lex(src)?;
    let mut p = Parser { src, tokens: &tokens, pos: 0, extensions: Vec::new() };
    let entities = p.entity_list()?;
    let t = p.peek();
    if t.kind != Kind::Eof {
        return Err(Diagnostic::new(format!("unexpected {}", t.kind.describe()), t.span));
    }
    let mut extensions = p.extensions;
    extensions.extend(comments.iter().map(|&span| ExtensionUse { extension: Extension::Comments, span }));
    child_overrides(&entities, &mut extensions);
    extensions.sort_by_key(|u| u.span.start);
    Ok(ParsedFile { source: src.to_string(), tokens, comments, entities, extensions })
}

/// E3 uses: the named children of an entity that has an inclusion.
fn child_overrides(list: &EntityList, out: &mut Vec<ExtensionUse>) {
    for e in &list.items {
        for entry in &e.entries {
            if let Entry::Related(r) = entry {
                if e.inclusion().is_some() {
                    for child in &r.list.items {
                        if let Some(n) = child.name() {
                            out.push(ExtensionUse { extension: Extension::ChildOverride, span: n.span });
                        }
                    }
                }
                child_overrides(&r.list, out);
            }
        }
    }
}

struct Parser<'a> {
    src: &'a str,
    tokens: &'a [Token],
    pos: usize,
    extensions: Vec<ExtensionUse>,
}

impl<'a> Parser<'a> {
    fn peek(&self) -> Token {
        self.tokens[self.pos]
    }

    fn peek2(&self) -> Token {
        self.tokens[(self.pos + 1).min(self.tokens.len() - 1)]
    }

    fn bump(&mut self) -> Token {
        let t = self.tokens[self.pos];
        if t.kind != Kind::Eof {
            self.pos += 1;
        }
        t
    }

    /// The end of the last token taken.
    fn last_end(&self) -> usize {
        if self.pos == 0 { 0 } else { self.tokens[self.pos - 1].span.end }
    }

    fn expect(&mut self, kind: Kind, what: &str) -> Result<Token, Diagnostic> {
        let t = self.peek();
        if t.kind == kind {
            Ok(self.bump())
        } else {
            Err(Diagnostic::new(format!("expected {what}, found {}", t.kind.describe()), t.span))
        }
    }

    fn at_list_end(&self) -> bool {
        matches!(self.peek().kind, Kind::Eof | Kind::RBracket | Kind::RParen | Kind::RBrace)
    }

    fn text(&self, span: Span) -> &'a str {
        span.text(self.src)
    }

    /// Upstream's `BsnSceneListItems`: entities until a closing delimiter, `--` between them, an
    /// optional trailing `--`.
    fn entity_list(&mut self) -> Result<EntityList, Diagnostic> {
        let mut list = EntityList::default();
        while !self.at_list_end() {
            list.items.push(self.entity()?);
            if self.at_list_end() {
                break;
            }
            list.separators.push(self.expect(Kind::DoubleMinus, "`--` between entities")?.span);
        }
        Ok(list)
    }

    fn entity(&mut self) -> Result<Entity, Diagnostic> {
        let start = self.peek().span.start;
        let mut entries: Vec<Entry> = Vec::new();
        while !(self.at_list_end() || self.peek().kind == Kind::DoubleMinus) {
            let entry = self.entry()?;
            if matches!(entry, Entry::Include(_) | Entry::Prefab(_)) && entries.iter().any(|e| !matches!(e, Entry::Name(_))) {
                return Err(Diagnostic::new("an inclusion must be the first entry", entry.span()));
            }
            entries.push(entry);
        }
        let end = entries.last().map_or(start, |e| e.span().end);
        Ok(Entity { span: Span::new(start, end), entries })
    }

    fn entry(&mut self) -> Result<Entry, Diagnostic> {
        let t = self.peek();
        match t.kind {
            Kind::Hash => {
                self.bump();
                let n = self.peek();
                let (name, quoted) = match n.kind {
                    Kind::Ident => (self.text(n.span).to_string(), false),
                    // E4
                    Kind::Str => {
                        self.extensions.push(ExtensionUse { extension: Extension::QuotedName, span: t.span.to(n.span) });
                        (unescape(self.text(n.span)), true)
                    }
                    _ => return Err(Diagnostic::new(format!("expected a name after `#`, found {}", n.kind.describe()), n.span)),
                };
                self.bump();
                Ok(Entry::Name(NameEntry { span: t.span.to(n.span), name, quoted, name_span: n.span }))
            }
            Kind::Colon => {
                self.bump();
                let s = self.expect(Kind::Str, "a quoted level path after `:`")?;
                Ok(Entry::Include(Include { span: t.span.to(s.span), path: unescape(self.text(s.span)), path_span: s.span }))
            }
            // E2
            Kind::At => {
                self.bump();
                let path = self.path()?;
                let fields = self.fields(true)?;
                let span = t.span.to(Span::new(0, self.last_end()));
                if let Fields::Tuple { open, .. } = fields {
                    return Err(Diagnostic::new("scene component fields are named: `@Comp { @prop: value, field: value }`", open));
                }
                self.extensions.push(ExtensionUse { extension: Extension::Prefab, span });
                Ok(Entry::Prefab(Prefab { span, path, fields }))
            }
            Kind::Ident | Kind::DoubleColon => {
                let path = self.path()?;
                if self.peek().kind == Kind::LBracket {
                    let open = self.bump().span;
                    let list = self.entity_list()?;
                    let close = self.expect(Kind::RBracket, "`]`")?.span;
                    return Ok(Entry::Related(Related { span: path.span.to(close), path, open, close, list }));
                }
                if path.segments.len() == 1 && path.segments[0] == "on" && self.peek().kind == Kind::LParen {
                    return Err(Diagnostic::new("observers (`on(…)`) are not supported in .bsn files", path.span));
                }
                let fields = self.fields(false)?;
                Ok(Entry::Patch(Patch { span: path.span.to(Span::new(0, self.last_end())), path, fields }))
            }
            Kind::LBrace => Err(Diagnostic::new("expressions are not supported in .bsn files", t.span)),
            Kind::Comma => Err(Diagnostic::new("`,` between entries is not supported: separate entries with whitespace", t.span)),
            Kind::DotDot => Err(Diagnostic::new("struct update (`..`) is not supported in .bsn files", t.span)),
            k => Err(Diagnostic::new(format!("expected an entry, found {}", k.describe()), t.span)),
        }
    }

    fn path(&mut self) -> Result<Path, Diagnostic> {
        let start = self.peek().span;
        let leading_colon = self.peek().kind == Kind::DoubleColon;
        if leading_colon {
            self.bump();
        }
        let first = self.expect(Kind::Ident, "a type path")?;
        let mut segments = vec![self.text(first.span).to_string()];
        while self.peek().kind == Kind::DoubleColon {
            self.bump();
            let seg = self.expect(Kind::Ident, "a path segment after `::`")?;
            segments.push(self.text(seg.span).to_string());
        }
        Ok(Path { span: start.to(Span::new(0, self.last_end())), leading_colon, segments })
    }

    /// `{ a: v, … }`, `(v, …)` or nothing. In a scene component instance (`props`), `@name: v` is a
    /// scene prop and `name: v` the component's own field, as in the 0.20 `bsn!` macro; elsewhere
    /// `@name` is refused, as there.
    fn fields(&mut self, props: bool) -> Result<Fields, Diagnostic> {
        match self.peek().kind {
            Kind::LBrace => {
                let open = self.bump().span;
                let mut fields = Vec::new();
                let mut trailing_comma = false;
                while !self.at_list_end() {
                    trailing_comma = false;
                    fields.push(self.field(props)?);
                    if self.at_list_end() {
                        break;
                    }
                    self.expect(Kind::Comma, "`,` between fields")?;
                    trailing_comma = true;
                }
                let close = self.expect(Kind::RBrace, "`}`")?.span;
                let mut seen = std::collections::HashSet::new();
                for f in &fields {
                    if !seen.insert(f.name.as_str()) {
                        return Err(Diagnostic::new(format!("duplicate field `{}`", f.name), f.name_span));
                    }
                }
                Ok(Fields::Named { open, close, fields, trailing_comma })
            }
            Kind::LParen => {
                let open = self.bump().span;
                let mut values = Vec::new();
                let mut trailing_comma = false;
                while !self.at_list_end() {
                    trailing_comma = false;
                    values.push(self.value()?);
                    if self.at_list_end() {
                        break;
                    }
                    self.expect(Kind::Comma, "`,` between values")?;
                    trailing_comma = true;
                }
                let close = self.expect(Kind::RParen, "`)`")?.span;
                Ok(Fields::Tuple { open, close, values, trailing_comma })
            }
            _ => Ok(Fields::Unit),
        }
    }

    fn field(&mut self, props: bool) -> Result<Field, Diagnostic> {
        let start = self.peek().span;
        if self.peek().kind == Kind::DotDot {
            return Err(Diagnostic::new("struct update (`..`) is not supported in .bsn files", start));
        }
        let prop = self.peek().kind == Kind::At;
        if prop {
            self.bump();
        }
        let name_tok = self.expect(Kind::Ident, "a field name")?;
        let name = self.text(name_tok.span).to_string();
        if !props && prop {
            return Err(Diagnostic::new("`@` props are only valid in a scene component instance (`@Comp { @prop: value }`)", start));
        }
        if self.peek().kind != Kind::Colon {
            return Err(Diagnostic::new(format!("expected `:` after `{name}`: field shorthand is not supported"), self.peek().span));
        }
        self.bump();
        let value = self.value()?;
        Ok(Field { span: start.to(value.span), prop, name, name_span: name_tok.span, value })
    }

    fn value(&mut self) -> Result<Value, Diagnostic> {
        let t = self.peek();
        let text = self.text(t.span);
        let kind = match t.kind {
            Kind::Float => {
                self.bump();
                if text.starts_with('-') {
                    self.extensions.push(ExtensionUse { extension: Extension::NegativeNumber, span: t.span });
                }
                ValueKind::Float(text.parse().map_err(|_| Diagnostic::new("invalid float", t.span))?)
            }
            Kind::Int => {
                self.bump();
                if text.starts_with('-') {
                    self.extensions.push(ExtensionUse { extension: Extension::NegativeNumber, span: t.span });
                }
                let (neg, rest) = match text.as_bytes()[0] {
                    b'-' => (true, &text[1..]),
                    b'+' => (false, &text[1..]),
                    _ => (false, text),
                };
                let (radix, digits) = match rest.strip_prefix("0x").or_else(|| rest.strip_prefix("0X")) {
                    Some(h) => (16, h),
                    None => (10, rest),
                };
                let v = i128::from_str_radix(digits, radix).map_err(|_| Diagnostic::new("integer out of range", t.span))?;
                ValueKind::Int(if neg { -v } else { v })
            }
            Kind::True | Kind::False => {
                self.bump();
                ValueKind::Bool(t.kind == Kind::True)
            }
            Kind::Str => {
                self.bump();
                ValueKind::Str(unescape(text))
            }
            Kind::Ident => {
                let path = self.path()?;
                let fields = self.fields(false)?;
                let span = path.span.to(Span::new(0, self.last_end()));
                return Ok(Value { span, kind: ValueKind::Type(Patch { span, path, fields }) });
            }
            Kind::LBrace => return Err(Diagnostic::new("expressions are not supported in .bsn files", t.span)),
            Kind::LBracket => return Err(Diagnostic::new("list values are not supported in .bsn files", t.span)),
            Kind::LParen => return Err(Diagnostic::new("tuple values are not supported in .bsn files: name the type", t.span)),
            Kind::Hash => return Err(Diagnostic::new("entity references (`#Name`) are not supported as values in .bsn files", t.span)),
            Kind::DotDot => return Err(Diagnostic::new("ranges are not supported in .bsn files", t.span)),
            k => return Err(Diagnostic::new(format!("expected a value, found {}", k.describe()), t.span)),
        };
        if self.peek().kind == Kind::DotDot {
            return Err(Diagnostic::new("ranges are not supported in .bsn files", t.span.to(self.peek2().span)));
        }
        Ok(Value { span: t.span, kind })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn err(src: &str) -> String {
        let e = parse(src).unwrap_err();
        let (l, c) = super::super::LineIndex::new(src).line_col(src, e.span.start);
        format!("{l}:{c} {}", e.message)
    }

    #[test]
    fn parses_upstream_shapes() {
        let f = parse("#A\n:\"inherited.bsn\"\nbevy_transform::Transform { x: 1.0 }\nChildren [\n  Marker\n  Score(0, 1)\n  --\n  #B\n]\n").unwrap();
        let a = &f.entities.items[0];
        assert_eq!(a.name().unwrap().name, "A");
        assert!(matches!(a.inclusion(), Some(Entry::Include(i)) if i.path == "inherited.bsn"));
        let kids = &a.related("Children").unwrap().list;
        assert_eq!(kids.items.len(), 2);
        assert_eq!(kids.items[1].name().unwrap().name, "B");
        assert_eq!(f.print(), f.source);
    }

    #[test]
    fn prefab_props_follow_the_macro() {
        let f = parse("#EnemyB\n@game::Enemy { @hp: 250.0, team: 1 }\nTransform { translation: Vec3 { x: 3.0, z: -4.0 } }").unwrap();
        let Some(Entry::Prefab(p)) = f.entities.items[0].inclusion() else { panic!() };
        assert_eq!(p.path.joined(), "game::Enemy");
        assert_eq!(p.fields.get("hp").unwrap().value.kind, ValueKind::Float(250.0));
        let codes: Vec<_> = f.extensions.iter().map(|u| u.extension.code()).collect();
        assert_eq!(codes, ["E2", "U1"]);
        assert!(!p.fields.get("team").unwrap().prop && p.fields.get("hp").unwrap().prop);
        assert!(err("Transform { @x: 1.0 }").contains("only valid in a scene component instance"));
    }

    #[test]
    fn refuses_what_files_lack_by_name() {
        assert_eq!(err("#A\nTransform { translation: {Vec3::ZERO} }"), "2:26 expressions are not supported in .bsn files");
        assert!(err("A Transform :\"room.bsn\"").contains("an inclusion must be the first entry"));
        assert!(err("Foo { ..Default::default() }").contains("struct update"));
        assert!(err("Foo { r: 0..5 }").contains("ranges"));
        assert!(err("Foo { l: [1, 2] }").contains("list values"));
        assert!(err("on(Foo)").contains("observers"));
        assert!(err("on(|e| {})").contains("closures"));
        assert!(err("~Foo").contains("templates"));
        assert!(err("A, B").contains("`,` between entries"));
        assert!(err("Foo { x: 1, x: 2 }").contains("duplicate field"));
        assert!(err("Children [ A ").contains("expected `]`"));
    }

    #[test]
    fn lists_and_separators_mirror_upstream() {
        let f = parse("A -- B --").unwrap();
        assert_eq!(f.entities.items.len(), 2);
        assert_eq!(f.entities.separators.len(), 2);
        assert_eq!(parse("-- A").unwrap().entities.items.len(), 2);
        assert_eq!(parse("").unwrap().entities.items.len(), 0);
    }

    #[test]
    fn extensions_are_listed() {
        let f = parse("// c\n#\"mesh#0\" A\n--\n#R :\"room.bsn\" Children [ #door Open ]").unwrap();
        let codes: Vec<_> = f.extensions.iter().map(|u| u.extension.code()).collect();
        assert_eq!(codes, ["E1", "E4", "E3"]);
    }
}
