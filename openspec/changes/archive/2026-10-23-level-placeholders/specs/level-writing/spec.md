## ADDED Requirements

### Requirement: Writes refuse missing and unknown types
`bevy-ts/level_write` SHALL refuse, with `missing`, a `set` or `revert` on a component the entity has as an `unregistered` or `failed` placeholder (`level-placeholders`), and a `set_prop` or `revert_prop` on an instance whose scene component is one. A `set_prop` on an instance that is a placeholder only because a required prop is unset SHALL write, since it is what fixes it. It SHALL refuse, with `unknown`, any op that would write a type no live code registers: `add_component`, `add_instance`, `add_entity`'s patches, or a `set` that adds a patch. The refusal message SHALL name the type. Saved text an inverse puts back SHALL NOT be refused with `unknown`: an `add_component` with `text` and a `restore`. `remove_component` on a placeholder SHALL remove its patch, matched by the name the file writes, and its inverse SHALL bring the text back. `delete`, `restore`, `rename` and `reparent` of an entity with placeholders SHALL work as for any entity.

#### Scenario: Setting a field of a missing component
- **WHEN** a client sends `set` on `Crate`, `game::Loot.rolls`, `4`, while `game::Loot` is missing
- **THEN** the batch is refused with `reason: "missing"` and the file is unchanged

#### Scenario: Adding a component that does not exist
- **WHEN** a client sends `add_component` on `Crate` with `Lootz`, which no code registers
- **THEN** the batch is refused with `reason: "unknown"`, naming `Lootz`, and the file is unchanged

#### Scenario: Removing a missing component
- **WHEN** a client sends `remove_component` on `Crate` with `game::Loot` while it is missing, then that write's inverse
- **THEN** the patch leaves the file and the crate loses its marker, then the patch comes back byte for byte and the marker with it
