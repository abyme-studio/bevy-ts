//! The differential test: every fixture and every example level that uses no extension upstream
//! rejects parses to the same tree with vendored `bevy_bsn`; every one that does, upstream rejects.
//! Upstream's root rule parses one entity, so the top level is compared as its scene-list body,
//! the rule a relationship list's `[ … ]` uses.

use crate::syntax::{self, Entity, EntityList, Entry, Fields, ParsedFile, Value, ValueKind};
use crate::upstream::parse_stream::{ParseError, ParseStream};
use crate::upstream::span::Span;
use crate::upstream::types as up;
use std::path::{Path, PathBuf};

fn to_path(p: &syntax::Path) -> up::Path {
    up::Path {
        leading_colon: p.leading_colon,
        segments: p.segments.iter().map(|s| up::PathSegment { ident: up::Ident(s.clone()) }).collect(),
    }
}

fn to_fields(f: &Fields) -> up::BsnFields {
    match f {
        Fields::Unit => up::BsnFields::Unit,
        Fields::Named { fields, .. } => {
            up::BsnFields::NamedFields(fields.iter().map(|f| up::BsnNamedField { name: f.name.clone(), value: to_value(&f.value) }).collect())
        }
        Fields::Tuple { values, .. } => up::BsnFields::UnnamedFields(values.iter().map(to_value).collect()),
    }
}

fn to_value(v: &Value) -> up::BsnValue {
    match &v.kind {
        ValueKind::Float(f) => up::BsnValue::Float(*f),
        ValueKind::Int(i) => up::BsnValue::Int(*i),
        ValueKind::Bool(b) => up::BsnValue::Bool(*b),
        ValueKind::Str(s) => up::BsnValue::String(s.clone()),
        ValueKind::Type(p) => up::BsnValue::Type(up::BsnType { path: to_path(&p.path), variant: None, fields: to_fields(&p.fields) }),
    }
}

fn to_bsn(e: &Entity) -> up::Bsn {
    up::Bsn {
        entries: e
            .entries
            .iter()
            .map(|entry| match entry {
                Entry::Name(n) => up::BsnEntry::Name(up::Ident(n.name.clone())),
                Entry::Include(i) => up::BsnEntry::CachedScene(up::BsnScene::Asset(up::StringLit(i.path.clone()))),
                Entry::Patch(p) => up::BsnEntry::FromTemplatePatch(up::BsnType { path: to_path(&p.path), variant: None, fields: to_fields(&p.fields) }),
                Entry::Related(r) => up::BsnEntry::RelatedSceneList {
                    relationship_target_path: to_path(&r.path),
                    scene_list: up::BsnSceneList(to_items(&r.list)),
                },
                Entry::Prefab(_) => panic!("E2 has no upstream form"),
            })
            .collect(),
    }
}

fn to_items(list: &EntityList) -> up::BsnSceneListItems {
    up::BsnSceneListItems(list.items.iter().map(to_bsn).collect())
}

fn upstream_parse(src: &str) -> Result<up::BsnSceneListItems, ParseError> {
    // Upstream's `ParseStream::peek_token` reads a lex error as the end of input, so a file that
    // fails to lex would parse as its prefix. Lex it whole first.
    Span::from(src).lex_all()?;
    let mut input = ParseStream::from(Span::from(src));
    let items = input.parse::<up::BsnSceneListItems>()?;
    if !input.is_empty() {
        return Err(input.error("trailing input"));
    }
    Ok(items)
}

/// Every `.bsn` under the fixtures and `examples/`.
pub(crate) fn corpus() -> Vec<PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut out = Vec::new();
    for dir in [root.join("tests/fixtures"), root.join("../../examples")] {
        walk(&dir, &mut out);
    }
    out.sort();
    out
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            if !matches!(p.file_name().and_then(|n| n.to_str()), Some("node_modules" | "target")) {
                walk(&p, out);
            }
        } else if p.extension().is_some_and(|x| x == "bsn") {
            out.push(p);
        }
    }
}

fn parse_file(p: &Path) -> ParsedFile {
    let src = std::fs::read_to_string(p).unwrap();
    syntax::parse(&src).unwrap_or_else(|e| panic!("{}", e.render(&p.display().to_string(), &src)))
}

#[test]
fn differential_against_vendored_bevy_bsn() {
    let files = corpus();
    assert!(files.len() >= 5, "fixture corpus missing: {files:?}");
    let (mut same, mut rejected) = (0, 0);
    for p in &files {
        let f = parse_file(p);
        let upstream = upstream_parse(&f.source);
        if f.extensions.iter().any(|u| u.extension.rejected_upstream()) {
            assert!(upstream.is_err(), "{}: upstream accepted a file using {:?}", p.display(), f.extensions);
            rejected += 1;
        } else {
            let upstream = upstream.unwrap_or_else(|e| panic!("{}: upstream rejected an extension-free file: {e}", p.display()));
            assert_eq!(to_items(&f.entities), upstream, "{}", p.display());
            same += 1;
        }
    }
    assert!(same >= 2 && rejected >= 2, "{same} same, {rejected} rejected");
}

#[test]
fn every_fixture_prints_back_byte_for_byte() {
    for p in corpus() {
        let f = parse_file(&p);
        assert_eq!(f.print(), f.source, "{}", p.display());
    }
    // Line endings the corpus files cannot pin through git.
    for src in ["#A\r\nTransform { x: 1.0 }\r\n--\r\n#B\r\n", "\t#A\tMarker\t", "#A // no newline at the end"] {
        assert_eq!(syntax::parse(src).unwrap().print(), src);
    }
}

#[test]
fn upstream_lexer_has_no_comments_or_negative_numbers() {
    // Pinned at the vendored commit (task 1.1): re-check whenever it is bumped.
    assert!(upstream_parse("// c\nA").is_err());
    assert!(upstream_parse("A { x: -1.0 }").is_err());
    assert!(upstream_parse("A { x: +1.0 }").is_ok());
}
