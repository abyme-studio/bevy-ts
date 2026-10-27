//! `bevy-ts levels check`: what a level file uses and what is wrong with it, without an engine.

use crate::syntax::{self, EntityList, Fields, LineIndex, ValueKind};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Where [`check_with_assets`] looks for asset paths: the asset root, the file's path under it,
/// and each extra source's folder by name, for `name://path`.
pub struct AssetRoots<'a> {
    pub root: &'a Path,
    pub level: &'a str,
    pub sources: &'a BTreeMap<String, PathBuf>,
}

/// The lines `levels check` prints for one file, and whether the file is free of errors.
pub fn check_source(file: &str, src: &str, upstream: bool) -> (Vec<String>, bool) {
    check_with_assets(file, src, upstream, None)
}

/// [`check_source`], also listing asset paths that do not exist under `assets`: the asset root, or
/// a source's folder for `name://path`. A source not in `assets.sources` is an error.
pub fn check_with_assets(file: &str, src: &str, upstream: bool, assets: Option<AssetRoots>) -> (Vec<String>, bool) {
    let parsed = match syntax::parse(src) {
        Ok(p) => p,
        Err(e) => return (vec![format!("{} (error)", e.render(file, src))], false),
    };
    let index = LineIndex::new(src);
    let at = |offset: usize| {
        let (l, c) = index.line_col(src, offset);
        format!("{file}:{l}:{c}")
    };
    let mut out = Vec::new();
    let mut ok = true;
    if upstream {
        for u in &parsed.extensions {
            out.push(format!("{}: {} {}", at(u.span.start), u.extension.code(), u.extension.describe()));
        }
        if parsed.extensions.is_empty() {
            out.push(format!("{file}: no extensions; upstream's parser reads it unchanged"));
        }
    }
    unnamed(&parsed.entities, &mut |offset| {
        out.push(format!("{}: warning: unnamed entity; the writer names it on its first edit", at(offset)));
    });
    if let Some(AssetRoots { root, level, sources }) = assets {
        let mut paths = Vec::new();
        strings(&parsed.entities, &mut paths);
        for (p, offset) in paths {
            let resolved = crate::compose::resolve_path(level, &p);
            let on_disk = resolved.split('#').next().unwrap_or(&resolved);
            let path = match on_disk.split_once("://") {
                None => root.join(on_disk),
                Some((name, rest)) => match sources.get(name) {
                    Some(folder) => folder.join(rest),
                    None => {
                        out.push(format!("{}: unknown asset source \"{name}\" in {resolved}: bevy-ts.json's assetSources has no {name} (error)", at(offset)));
                        ok = false;
                        continue;
                    }
                },
            };
            if !path.exists() {
                out.push(format!("{}: warning: missing asset {resolved}", at(offset)));
            }
        }
    }
    (out, ok)
}

/// String values in component patches that look like asset paths (they have an extension).
fn strings(list: &EntityList, out: &mut Vec<(String, usize)>) {
    fn fields(f: &Fields, out: &mut Vec<(String, usize)>) {
        let values: Vec<&syntax::Value> = match f {
            Fields::Named { fields, .. } => fields.iter().filter(|f| !f.prop).map(|f| &f.value).collect(),
            Fields::Tuple { values, .. } => values.iter().collect(),
            Fields::Unit => Vec::new(),
        };
        for v in values {
            match &v.kind {
                ValueKind::Str(s) if s.rsplit('/').next().is_some_and(|n| n.split('#').next().unwrap_or("").contains('.')) => out.push((s.clone(), v.span.start)),
                ValueKind::Type(p) => fields(&p.fields, out),
                _ => {}
            }
        }
    }
    for e in &list.items {
        for entry in &e.entries {
            match entry {
                syntax::Entry::Patch(p) => fields(&p.fields, out),
                syntax::Entry::Related(r) => strings(&r.list, out),
                _ => {}
            }
        }
    }
}

fn unnamed(list: &EntityList, warn: &mut impl FnMut(usize)) {
    for e in &list.items {
        if e.name().is_none() && !e.entries.is_empty() {
            warn(e.span.start);
        }
        for entry in &e.entries {
            if let syntax::Entry::Related(r) = entry {
                unnamed(&r.list, warn);
            }
        }
    }
}

/// `bevy-ts levels check [--upstream] [--assets <dir>] <file.bsn>…`; the process exit code.
/// Asset paths are checked under `--assets`, default `assets`, and `name://` paths under the
/// folders `sources` gives for the first file (the project's `assetSources`).
pub fn run(args: &[String], sources: impl FnOnce(&Path) -> Result<BTreeMap<String, PathBuf>, String>) -> i32 {
    let upstream = args.iter().any(|a| a == "--upstream");
    let mut root = std::path::PathBuf::from("assets");
    let mut files: Vec<&String> = Vec::new();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--assets" => match it.next() {
                Some(d) => root = d.into(),
                None => {
                    eprintln!("--assets takes a directory");
                    return 2;
                }
            },
            s if s.starts_with("--") => {}
            _ => files.push(a),
        }
    }
    if files.is_empty() {
        eprintln!("usage: bevy-ts levels check [--upstream] [--assets <dir>] <file.bsn>…");
        return 2;
    }
    let sources = match sources(Path::new(files[0])) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("{e}");
            return 1;
        }
    };
    let mut code = 0;
    for f in files {
        match std::fs::read_to_string(f) {
            Ok(src) => {
                let level = Path::new(f).strip_prefix(&root).map(|p| p.to_string_lossy().replace('\\', "/")).unwrap_or_else(|_| f.to_string());
                let (lines, ok) = check_with_assets(f, &src, upstream, Some(AssetRoots { root: &root, level: &level, sources: &sources }));
                for l in lines {
                    println!("{l}");
                }
                if !ok {
                    code = 1;
                }
            }
            Err(e) => {
                eprintln!("{f}: {e}");
                code = 1;
            }
        }
    }
    code
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_extensions_with_locations() {
        let src = "// arena\n#EnemyA\n@game::Enemy\n--\n#EnemyB\n@game::Enemy { @hp: 250.0 }\n";
        let (lines, ok) = check_source("arena.bsn", src, true);
        assert!(ok);
        assert_eq!(lines, ["arena.bsn:1:1: E1 comment", "arena.bsn:3:1: E2 scene component instance", "arena.bsn:6:1: E2 scene component instance"]);
    }

    #[test]
    fn an_upstream_only_file_reports_none() {
        let (lines, _) = check_source("room.bsn", "#Room\nChildren [ #Door Marker -- Marker ]", true);
        assert_eq!(lines, ["room.bsn: no extensions; upstream's parser reads it unchanged", "room.bsn:2:28: warning: unnamed entity; the writer names it on its first edit"]);
    }

    #[test]
    fn lists_missing_assets() {
        let dir = std::env::temp_dir().join(format!("bevy-ts-levels-check-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("levels")).unwrap();
        std::fs::write(dir.join("levels/crate.glb"), "").unwrap();
        std::fs::create_dir_all(dir.join("art/Maps")).unwrap();
        std::fs::write(dir.join("art/Maps/Town.png"), "").unwrap();
        let sources = BTreeMap::from([("stardew".to_string(), dir.join("art"))]);
        let src = "#A Mesh3d(\"./crate.glb#Mesh0\")\n#B Mesh3d(\"gone.png\") Label { text: \"hello\" }\n#C Sprite { image: \"stardew://Maps/Town.png\" }\n#D Sprite { image: \"stardew://Maps/Gone.png\" }";
        let roots = AssetRoots { root: &dir, level: "levels/a.bsn", sources: &sources };
        let (lines, ok) = check_with_assets("levels/a.bsn", src, false, Some(roots));
        assert!(ok);
        assert_eq!(lines, ["levels/a.bsn:2:11: warning: missing asset gone.png", "levels/a.bsn:4:20: warning: missing asset stardew://Maps/Gone.png"]);
        let src = "#E Sprite { image: \"nope://a.png\" }";
        let (lines, ok) = check_with_assets("levels/a.bsn", src, false, Some(AssetRoots { root: &dir, level: "levels/a.bsn", sources: &sources }));
        let _ = std::fs::remove_dir_all(&dir);
        assert!(!ok);
        assert_eq!(lines.len(), 1, "{lines:?}");
        assert!(lines[0].contains("\"nope\"") && lines[0].contains("nope://a.png") && lines[0].ends_with("(error)"), "{lines:?}");
    }

    #[test]
    fn a_parse_error_names_its_place() {
        let (lines, ok) = check_source("a.bsn", "#A\nTransform { translation: {Vec3::ZERO} }", false);
        assert!(!ok);
        assert_eq!(lines, ["a.bsn:2:26: expressions are not supported in .bsn files (error)"]);
    }
}
