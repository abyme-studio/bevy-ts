## 1. Compose

- [x] 1.1 A `Missing` entry (component, reason, message, fields as written, location, expected, misfit, overrides) and a `tolerant` flag on `compose::prepare`, true only in dev mode
- [x] 1.2 Unknown and no-longer-scene components, unknown fields and props, and type errors push entries instead of errors; a mismatch keeps the fields that fit; an unset required prop makes the instance a placeholder
- [x] 1.3 Under a placeholder instance, child entries with no base are kept as `overrides` and not spawned
- [x] 1.4 Parse errors, duplicate names, cycles, missing includes and bound overrides stay errors

## 2. Engine

- [x] 2.1 `host::expand` answers per item; a throwing item becomes a `failed` entry with its location as `expected`; other callers keep failing whole
- [x] 2.2 `MissingComponents`, reflected and registered, written by the reconcile and removed with the last entry
- [x] 2.3 `check_section` fails a generation only on broken-file errors; missing code commits as placeholders with warnings naming the level lines
- [x] 2.4 The empty generation after a failed TS build reports `failed` with the build error
- [x] 2.5 `Levels` keeps the last live decl location per component name, for `expected`
- [x] 2.6 One warning diagnostic per entry on the level and in the terminal

## 3. Writes

- [x] 3.1 `check_world` refuses `missing` and `unknown`; inverse text is exempt; `remove_component` matches a missing patch by its written name

## 4. Protocol

- [x] 4.1 `level.missing` on `tsx_tree` nodes and `tsx_tree+watch`; `missing` on `level_node`
- [x] 4.2 Contract: `MissingComponent`, `TsxTreeNode.level.missing`, `LevelNodeResult.missing`, refusal reasons `missing` and `unknown`; the next free minor on main at landing; regenerated Rust and TS types; CI diff check

## 5. Tests and docs

- [x] 5.1 Tests in `crates/bevy-ts-levels/tests/levels.rs`, one per scenario of `level-placeholders`, the modified `levels` and `level-hot-reload` scenarios, and `level-writing`'s refusals
- [x] 5.2 A test that a non-dev engine fails the level on a missing type
- [x] 5.3 `docs/levels.md`: placeholders, the reasons, what stays strict, the Bevy-principles exception, and the minor the studio needs
