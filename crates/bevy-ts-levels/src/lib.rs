//! Levels for bevy-ts: `.bsn` files that say where things are, loaded as reconciled roots, and
//! the writer the viewport edits them through.
//!
//! TSX defines prefabs, what a thing is; a level places instances of them and overrides their
//! fields per instance. Bevy 0.20 ships no `.bsn` loader, so this crate reads upstream's file
//! grammar (`bevy_bsn`, PR #26043) plus four named extensions, and stays close enough that
//! adopting Bevy's loader deletes [`syntax`] and the loader.

pub mod check;
pub mod syntax;

#[cfg(test)]
#[allow(warnings, clippy::all, clippy::pedantic)]
mod upstream;

#[cfg(test)]
mod differential;
