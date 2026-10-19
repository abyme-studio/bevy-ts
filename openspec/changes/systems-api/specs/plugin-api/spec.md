## ADDED Requirements

### Requirement: Component declarations and scene components by name
A plugin SHALL be able to read a live TS component's declaration by its name or its `module#export`: its fields with types and defaults and, for a scene component, its scene props, each with its type (a component field type, or `string`), its literal default or the fact that it has a computed one, and whether it is optional. A plugin SHALL be able to list every live scene component, whether or not a system names it.

#### Scenario: A level places a prefab by name
- **WHEN** `src/enemies.tsx` exports `class Grunt extends SceneComponent { team: u8 = 0; static scene({ speed = 2 }: { speed?: f32 }) { ... } }` and a plugin asks for `enemies::Grunt`
- **THEN** it gets the field `team` (`u8`, default 0) and the prop `speed` (`f32`, default 2, optional)
