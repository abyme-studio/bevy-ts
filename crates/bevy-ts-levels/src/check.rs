//! `bevy-ts levels check`: what a level file uses and what is wrong with it, without an engine.

use crate::syntax::{self, EntityList, LineIndex};

/// The lines `levels check` prints for one file, and whether the file is free of errors.
pub fn check_source(file: &str, src: &str, upstream: bool) -> (Vec<String>, bool) {
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
    (out, true)
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

/// `bevy-ts levels check [--upstream] <file.bsn>…`; the process exit code.
pub fn run(args: &[String]) -> i32 {
    let upstream = args.iter().any(|a| a == "--upstream");
    let files: Vec<&String> = args.iter().filter(|a| !a.starts_with("--")).collect();
    if files.is_empty() {
        eprintln!("usage: bevy-ts levels check [--upstream] <file.bsn>…");
        return 2;
    }
    let mut code = 0;
    for f in files {
        match std::fs::read_to_string(f) {
            Ok(src) => {
                let (lines, ok) = check_source(f, &src, upstream);
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
    fn a_parse_error_names_its_place() {
        let (lines, ok) = check_source("a.bsn", "#A\nTransform { translation: {Vec3::ZERO} }", false);
        assert!(!ok);
        assert_eq!(lines, ["a.bsn:2:26: expressions are not supported in .bsn files (error)"]);
    }
}
