//! Bevy's `bevy_bsn` syntax module, vendored for the differential test only.
//!
//! Source: bevyengine/bevy `crates/bevy_bsn/src` at 81f4402854f2e427bcf88b110123ba84e07c5457, the
//! merge of PR #26043 "BSN: file format lexing, parsing, and AST" (2026-10-15). MIT OR Apache-2.0,
//! licences beside this file. The only edits: `crate::` paths point at this module, and
//! `alloc::collections` reads `std::collections`. Bump it by re-copying the five files at a new
//! commit and re-running `cargo test -p bevy-ts-levels differential`.

pub mod lex;
pub mod parse;
pub mod parse_stream;
pub mod span;
pub mod types;
