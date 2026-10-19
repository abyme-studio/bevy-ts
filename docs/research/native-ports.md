# Native ports after rapier3d (2026-10-20, plan)

The `rapier3d` port (crates/bevy-ts-rapier3d) set the shape every port follows: a crate
`bevy-ts-<name>` whose `PortPlugin` adds the third-party plugin the way TS gameplay needs it,
exposes its sets with `ts_set`, exports read-only params with `#[export]` + `ts_param`, ships a
built-in module with `ts_module` (classes declared with `__bevyts.data`, helpers, a `.d.ts`), and
forwards Rust messages TS reads with `plugin_api::write_message`. Reflection types the plain-data
components with no port code. This note plans the next three.

## leafwing-input-manager 0.22 (on Bevy 0.20)

The obstacle: `Actionlike` is a trait the game implements on its own Rust enum (`Debug + Eq +
Hash + Reflect + Typed + TypePath + FromReflect`), and every leafwing type is generic over it
(`InputMap<A>`, `ActionState<A>`, `InputManagerPlugin<A>`). A TS game has no Rust enum.

Plan:

- The port defines one Rust action type, `TsAction(u16)`, implementing `Actionlike` with
  `input_control_kind` read from a table TS fills. Its names come from TS.
- `bevy-ts/input` exports `class Actions extends Resource` built from a TS declaration,
  `actions({ jump: "button", move: "dualAxis", throttle: "axis" })`, which a section sends to
  Rust; Rust assigns each name a `u16` once per engine (names keep their ids across reloads,
  new names append), and rebuilds the kind table.
- Bindings: `inputMap({ jump: [KeyCode.Space, GamepadButton.South], move: [VirtualDPad.wasd()] })`
  as a section; Rust converts it into an `InputMap<TsAction>` on the player entity. Changing it
  on reload replaces the map, no restart.
- Reading: an exported param `ReadActions` with `pressed(name)`, `justPressed(name)`,
  `value(name)`, `axisPair(name)`; names are resolved to ids once per generation in the JS glue,
  so each call is one fast op with a `u16`. A batched `state(out: Float32Array)` fills every
  action's value in one call for systems that read many.
- `InputManagerPlugin::<TsAction>::default()` in the port's plugin, in `PreUpdate` as leafwing
  ships it; its `InputManagerSystem` sets are exposed as `input:update`.

Risk: leafwing's `Reflect` derive on a newtype action is fine, but its `typetag`-based
serialization of user inputs is not needed and stays off (`default-features = false` minus
`asset`). Size: a few hundred lines, like rapier.

## avian3d and bevy_hanabi

Both are still on Bevy 0.19 (avian3d 0.7.0, bevy_hanabi 0.19.0, last released June 2026). They
wait for releases on Bevy 0.20; nothing to build before then.

- avian: the port mirrors rapier3d: `PhysicsPlugins` in `FixedUpdate`, sets
  `avian:prepare`/`avian:step`/`avian:writeback`, `SpatialQuery` exported as `castRay`/`castRays`
  (avian's own param, read-only), `Collider` constructors, `CollisionStart`/`CollisionEnd`
  forwarded. Avian components (`LinearVelocity(Vec3)`) are tuple structs of plain data, so they
  are layout tier with no port code.
- hanabi: effects are assets; the port exports a `spawnEffect(path, transform)` command and
  exposes spawner settings (`EffectSpawner` fields that are plain data) as columns. GPU
  simulation stays in Rust.
