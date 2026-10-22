## Context

A level resolves against `WorldSchema::live` (`engine.rs`): the live generation's TS decls, the native layouts and Bevy's `TypeRegistry`. `compose::prepare` checks every entry, every `@Comp { … }` expands in one host call (`bevy_ts_tsx::host::expand`), and the result reconciles in one go. Any failure fails the whole level: `apply_composed` logs it and leaves the world as it was, and `level_open` answers `failed`/`load`. Rule 9 (`check_section`) also fails a TS generation under which a loaded level no longer composes.

The writer splices text and never re-emits an entity, and the parse is lossless (`bsn-format`). So the file already keeps a missing component's text through every edit. The engine only has to stop failing, and say what is missing.

Research: `level-placeholders-research.md`. Studio design, agreed with design@studio-placeholders (abyme): `level-placeholders-studio.md`. Both in abyme's documents for bevy-ts.

Names: the roadmap's E3 (placeholders) and E4 (Play/Stop) are not `bsn-format`'s extension codes E3 and E4. The specs avoid both labels.

## Goals / Non-Goals

**Goals:** a level with missing, failed or misfit code loads under `bevy-ts dev`, with a placeholder per entry; the saved text survives every edit; the protocol tells the studio what is missing and why; tolerance stops at broken files, writes and shipped games.

**Non-Goals:** a last-good schema cache (`MissingComponent.schema`); typed editing of a missing component's fields; making `levels check` or `build` resolve types; Play/Stop snapshots (E4); the studio side.

## Decisions

- **Placeholders are made in compose, per entry.** `Schema::component`/`scene` failures and per-field misfits push a `Missing` entry onto the composed entity and drop the patch, instead of returning an error. Broken-file errors (parse, duplicate name, cycle, missing include, bound override) stay errors. Alternative: catch failures at reconcile. Rejected: compose is where the location and the saved text are at hand.
- **`expand` answers per item.** `host::expand` returns `Vec<Result<NodeDesc, Vec<ScriptError>>>`. A `None` item becomes a `failed` entry for that instance only, with the error's message and location as `expected`. Other callers keep failing on any error item. The JS runtime already returns `null` per throwing item.
- **One marker, `MissingComponents`.** A reflected Rust component in `bevy-ts-levels`, written by the reconcile like any controlled component, so it comes and goes with the entries. It holds the entries as `level_node` serializes them. Reflected so `bevy_remote` and a world snapshot see it. Alternative: a side table keyed by node id. Rejected: a snapshot or a despawn would lose it.
- **A mismatch is per field.** The component applies with the fields that fit, so a field rename does not take a working component off the entity. A required prop unset is the exception: the scene cannot run, so the instance is a whole placeholder with `reason: "mismatch"`.
- **Kept overrides.** Under a placeholder instance, a child entry with no base cannot be told apart from a merge target, so it is kept on the entry and not spawned. Unity keeps a missing prefab's overrides the same way. A child with an include or an `@Comp` spawns. When the code returns, the next compose merges the overrides by name. Trade-off: a plain added child (only `Transform` and a mesh) under a missing instance is hidden until the code returns. Accepted: it is not lost, and it shows in `overrides`.
- **Rule 9 keeps only broken-file errors.** `check_section` composes each level under the pending schema and fails the generation only on errors compose still raises. Missing code now makes placeholders, so it no longer reaches the check. Expansion throws are seen after commit, as today, and become `failed` entries.
- **A failed TS build is `failed`.** While the live generation is the empty one a failed build left (`hot-reload`), every unresolved name reports `failed`, with the build error's first line as `message` and its file as `expected`.
- **`expected` from memory.** `Levels` keeps the last live decl location per component name, updated on each generation. No disk state.
- **Diagnostics are warnings.** Each entry adds a `LevelDiagnostic` with its location and a `warning:` message to the level's diagnostics, so `level_list`, `level_open` and `loaded` carry it with no new field. The terminal prints it as a warning.
- **Strict in a shipped game.** Compose takes a `tolerant` flag, true only when `BevyTsPlugin` is in dev mode. With it false, a placeholder entry is the error it was. So `build`'s output behaves as before, with no new build step.
- **`level_write` refuses, it does not guess.** `check_world` refuses `set`/`revert`/`set_prop`/`revert_prop` on a placeholder component (`missing`), and new types no live code registers (`unknown`). Inverse text (`add_component` with `text`, `restore`) is exempt, so undo of a removal or a delete works. `remove_component` matches a missing patch by the name written, since it does not resolve.
- **Protocol additions.** `TsxTreeNode.level.missing?`, `LevelNodeResult.missing?`, `MissingComponent`, two refusal reasons. All optional fields, so the next free minor at landing (planned 1.20).

## Risks / Trade-offs

- [A typo no longer fails loudly] → It still warns in the terminal and on `level+watch`, `level_write` refuses unknown types, and a shipped game fails on it. `levels check` cannot catch it, as today, because it has no engine.
- [A deleted class no longer blocks the TS save] → Intended. The text stays in the file, and the class coming back restores it.
- [Bevy-principles exception] → Bounded to bevy-ts's own loader and dev mode; it goes with the loader when Bevy ships one. Needs the operator's go.
- [Play/Stop (E4) loses the marker] → `MissingComponents` is reflected, so a snapshot carries it. E4's spec must keep the scenario "Play and back".
- [Protocol minor collides] → Take the next free minor on main at landing, rebase and regenerate types. lead@bevy-ts names the number.

## Migration Plan

No data migration: files are unchanged. An older engine fails the load as before. The studio checks for `level.missing` and draws nothing when it is absent.

## Open Questions

- The operator's go on the Bevy-principles exception (`level-placeholders/go`).
