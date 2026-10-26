# Bevy coverage: what a TS script reaches today

Audit of Bevy 0.20's core crates against bevy-ts on main, for the operator's ask: nothing in core
Bevy without an idiomatic TS translation that is not a hand-written hack. First written at
`542a005` (2026-10-23) by research@bevy-coverage-audit; refreshed at `50340d9ef` (2026-10-27) by
research@coverage-refresh, for lead@engine-core.

Status words:

- **idiomatic**: a Bevy-shaped TS API (types, params, TSX) does it.
- **partial**: idiomatic for part; the missing part is named.
- **hack**: reached only through `spawnNative` recipes, `custom()` ops or JSON blobs.
- **missing**: no way from TS without writing Rust.
- **not needed** / **decline**: no TS path wanted, with the reason.

A row is idiomatic only with a test that proves it, or it says "no test". Owner is "—" when
covered, a running change (`openspec/changes/<name>` on main, or a running team) when one owns the
gap, else "open". Tests are relative to `crates/`; a bare file name is
`bevy-ts-cli/tests/it/<file>`. No test was run for this refresh: a named test exists on main and is
not `#[ignore]`.

The one-line answer: since 2026-10-23 every proposed change but one has landed (all but
custom-relationships). Behaviour is now covered too: schedules, conditions, Rust messages and
observers, input devices, assets, gizmos, audio, animation, diagnostics, saves and 2D sprites and
meshes. What is left is small: a few typing and test gaps a game team hits, and a handful of
Bevy features with no owner. They are ranked at the end.

## ECS

| Capability | Today | Evidence / test | Owner |
|---|---|---|---|
| TS components (layout columns) | idiomatic | `systems.rs` `ts_writes_reach_rust_and_only_written_rows_are_changed` | — |
| TS component, resource, `Local`, message and event `String` / `Vec<T>` fields; lifecycle values carry a copy | partial: whole assignment and in-place list edits (`push`, `[i] =`, sent as item edits), on components, resources, `Local`, messages and events; `.field[i]` and `.field[i].member` over the protocol. Lists of numbers, bool, strings, vectors, `Entity`, unions and objects. Missing: an `In<E>` input with one, nested lists, heap members of a list's object item, object fields outside a list | `heap_fields.rs` `a_resmut_write_is_seen_by_res_and_an_unchanged_res_makes_no_host_call`, `a_despawn_observer_reads_the_label_as_it_was`, `a_list_reads_frozen_on_a_read_row_and_is_written_whole`, `in_place_edits_reach_rust_as_edits_and_mark_only_a_real_change`, `edits_on_res_mut_a_local_and_an_observers_mut_are_seen_after_them`; `bevy-ts/src/heap.rs` `each_list_edit_applies_in_place_and_one_past_the_end_changes_nothing`; `bevy-ts/src/memory.rs` `a_locals_string_is_freed_once_on_redeclare_restart_and_drop`; `conditions.rs` `resource_equals_compares_a_string_by_value`; `protocol.rs` `string_and_list_fields_read_edit_and_type_on_contract`; `bevy-ts/tests/it/heap_fields.rs` `a_failed_reload_keeps_unread_heap_messages_and_frees_nothing_twice`, `an_event_over_two_observers_and_two_hops_is_read_by_each_and_freed_once`; `bevy-ts/src/messages.rs` `a_heap_queue_frees_each_record_once_on_update_copy_and_drop`; `heap_fields.rs` `a_message_string_is_read_by_two_readers_and_set_on_next`, `an_event_list_is_seen_by_two_observers`; `bevy-ts/tests/it/heap_fields.rs` `every_list_item_kind_is_freed_once_on_spawn_clone_reload_and_drop`, `a_rust_system_reads_vec3_and_struct_lists_and_pushes_onto_one`; `heap_fields.rs` `each_list_item_kind_reads_frozen_and_a_rust_push_is_seen` | follow-ups after ts-heap-fields-2 |
| Rust layout-tier components as query data | idiomatic | `native_components.rs` `a_ts_write_is_seen_by_rust_the_same_frame_and_changed_only_on_written_rows` | — |
| Layout-tier holes (`Color`, enums with data, `Option`) | idiomatic in TSX and systems (`light.color`), and bound in TSX (`<DistanceFog color={() => …}>`, layout-bindable); no system test of a data-enum or `Option` hole | `reflect_systems.rs` `a_hole_reads_and_writes_and_column_views_stay_valid`; `reflect_values.rs` `a_light_colour_is_a_prop_written_through_reflection`, `every_kind_is_a_tsx_prop_and_a_list_is_replaced`, `a_bound_fog_colour_follows_its_signal`; tsc: `packages/bevy-ts/test/tsx/reflect.tsx` `Bound` | — |
| Reflect-tier components in TSX, `With`/`Without` | idiomatic | `native_components.rs` `a_reflect_tier_component_is_a_tsx_element`; `ecs_queries.rs` `probe_disabled_and_sparse_filters` | — |
| Reflect-tier TSX props bound to a function (`<Projection value={() => …}>`) | idiomatic: typed `T \| (() => T)` and written through reflection as each value arrives | `reflect_values.rs` `a_bound_projection_follows_its_signal`, `an_opaque_hole_loads_and_a_bad_colour_fails_naming_it`; tsc: `packages/bevy-ts/test/tsx/reflect.tsx` `Bound` | — |
| Reflect-tier components from `Commands` (`Camera3d`, `Projection`, `DirectionalLight`, `Name`, `Disabled`) | idiomatic: `Class.of(json)` over `OP_INSERT_REFLECT`; `Name` and `Disabled`: no test | `reflect_scripts.rs` `an_orthographic_camera_from_ts`, `a_warm_sun_from_ts`, `a_bad_value_fails_its_command_naming_type_and_field` | — |
| Reflect-tier components as query data | idiomatic; computed components read and refuse writes | `reflect_systems.rs` `a_camera_switches_to_orthographic_and_changed_sees_it`, `a_computed_component_refuses_a_write`, `ref_get_reads_reflected_values_and_reruns_on_change` | — |
| `bevy.d.ts` reflect-tier types carry fields | idiomatic: generated `ReflectClass<…>` with `Holes<>` | `reflect_coverage.rs` `prebuilt_engine_has_no_unlisted_gap`, `codec_output_equals_read_for_every_covered_type` | — |
| Reflected members read with their read type (holes, reflect-tier `value` and fields, on `Mut`, read rows, `Res`/`ResMut`) | idiomatic: a getter of `read`'s shape (`sprite.texture_atlas.index: number`, `light.color: ColorRead`) and a setter of `write`'s; Rust message and event fields keep the written shape | `bevy-ts-cli/src/types.rs` `every_kind_reads_as_read_gives_it`, `color_read_is_bevys_color`; `packages/bevy-ts/test/reflect.ts` (tsc) | — |
| TS resources (`Res`, `ResMut`) | idiomatic | `surface.rs` `a_score_resource_is_written_in_place_and_kept_across_reloads` | — |
| Rust resources (layout and reflect tier) | idiomatic: `Res`/`ResMut`/`Option<Res>` by path or short name; `GlobalsUniform` (render world) fails every run; a resource named as query data fails the load saying to take `Res<T>` | `rust_resources.rs` `a_ts_write_to_a_bevy_resource_is_seen_by_rust_that_frame_and_changed_once`, `unusable_resources_fail_the_load_naming_the_system_and_resource`, `an_absent_resource_fails_the_run_and_an_optional_one_is_undefined`; `reflect_systems.rs` `res_mut_of_a_reflect_tier_resource_writes_back_with_change_detection` | — |
| Filters `With`, `Without`, `Changed`, `Added` | idiomatic | `systems.rs` `changed_and_added_filters_visit_only_matching_rows` | — |
| Filters `Or`, `Spawned`, `Has`, `AnyOf`, `Ref`; sparse-set filters | idiomatic | `ecs_queries.rs` `or_mixes_archetype_and_row_terms`, `spawned_sees_new_entities_once`, `has_is_a_bool_and_never_filters`, `any_of_needs_one_part`, `ref_reports_ticks_and_refuses_writes`, `probe_disabled_and_sparse_filters` | — |
| `Option`, `Entity`, `get`, `single`, `each`, iteration | idiomatic | `systems.rs` `option_entity_single_get_and_iteration` | — |
| `Single`, `Populated`, `RemovedComponents`, `SystemChangeTick`, `iterMany`, `getMany`, `iterCombinations`, `iterAncestors`, `contains`, `isEmpty` | idiomatic | `ecs_queries.rs` `single_skips_without_exactly_one`, `populated_skips_when_empty`, `removed_components_sees_remove_and_despawn_once`, `system_change_tick_last_run_is_previous_this_run`, `iter_many_and_get_many`, `iter_combinations_yields_each_set_once`, `contains_and_is_empty`, `iter_ancestors_and_descendants` | — |
| `ParamSet` | not needed: overlapping queries share one column | `systems.rs` `overlapping_queries_alias_one_column` | — |
| Hierarchy: `setParent`, `ChildOf`, `withChildren`, `addChildren`, `detachChildren`, `despawnChildren`, `insertRecursive`, `removeRecursive` | idiomatic | `systems.rs` `commands_spawn_parent_overflow_and_discard`; `ecs_queries.rs` `children_commands`, `insert_and_remove_recursive`, `despawn_takes_children` | — |
| Hierarchy: walk down (`Children`), sibling order | partial: `Children` is reflect-tier query data, no test reads it; no `iterDescendants`; no sibling-order helpers | — | ecs-queries 6.1 (`iterDescendants`); sibling order: open |
| Custom relationships | missing: `Relationship`/`RelationshipTarget` not exported | — | custom-relationships (unstarted, after ecs-queries) |
| Commands `spawn`, `insert`, `remove`, `despawn`, `trigger`, `write` | idiomatic | `systems.rs` as above; `surface.rs` `an_observer_triggered_from_ts_runs_before_the_next_system`; `ecs_queries.rs` `remove_takes_only_named`, `despawn_takes_children` | — |
| `spawn(<element/>)` returns its root; `entity(e).insert(<X/>)` lands on `e` | idiomatic | `ecs_queries.rs` `spawn_element_returns_its_root`, `insert_element_lands_on_the_entity` | — |
| `spawnBatch`, `insertBatch`, `insertIfNew`, `tryInsert`, `retain`, `clear`, `removeWithRequires`, `cloneAndSpawn` | idiomatic | `ecs_queries.rs` `spawn_batch_returns_ids_in_order`, `insert_batch_inserts_each`, `insert_if_new_and_try_insert`, `retain_and_clear`, `remove_with_requires`, `clone_and_spawn_copies_components` | — |
| Spawns per system run | idiomatic: the 256-id pool refills through `reserve(n)` | `ecs_queries.rs` `a_thousand_spawns_in_one_run` | — |
| `spawnNative` recipes `camera3d`, `directionalLight`, `cuboid` | removed; `spawnNative` stays for plugin recipes | `reflect_scripts.rs` `a_removed_recipe_names_its_replacement` | — |
| `custom(kind, payload)` | hack, kept for plugin opcodes | `bevy-ts/tests/it/plugin_api.rs` `plugin_ops_apply_in_order_with_built_in_ones` | — (a plugin seam) |
| TS messages | idiomatic (TS queues, not Bevy `Messages<T>`) | `surface.rs` `two_readers_see_the_same_messages_once_and_they_expire` | — |
| Reading Rust messages (`KeyboardInput`, `AssetEvent`, `WindowResized`) | idiomatic (`MessageReader<M>`); `CollisionEvent` has no `Reflect` and stays forwarded by `plugin_api::write_message` | `rust_events.rs` `a_ts_reader_sees_keyboard_input_once`, `window_messages_reach_a_ts_reader` | — |
| Writing Rust messages (`GamepadRumbleRequest`, `RequestRedraw`) | idiomatic (`MessageWriter<M>`, `commands.write`) | `rust_events.rs` `a_ts_writer_sends_a_rumble_request_a_rust_reader_sees`, `an_unlisted_reflect_message_is_writable` | — |
| A project's own Rust message or event types in TS | partial: reachable, declared by hand (`rustMessage("game_native::Scored")`); the project's `native.d.ts` does not emit them (`docs/native.md:303`) | `rust_events.rs` `a_project_message_is_read_and_written_from_ts`; tsc: `packages/bevy-ts/test/rust_events.ts` | open |
| Observers: TS `Event`, `EntityEvent`, bubbling | idiomatic | `surface.rs` `an_observer_triggered_from_ts_runs_before_the_next_system`; `ecs_queries.rs` `entity_event_bubbles_until_stopped` | — |
| Lifecycle `Add`, `Insert`, `Remove` | idiomatic, at the next stage boundary; `Insert`: no test | `surface.rs` `a_lifecycle_observer_on_a_rust_insert_runs_at_the_next_stage`; `ecs_queries.rs` `remove_observer_reads_the_value` | — |
| Lifecycle `Discard`, `Despawn` | idiomatic, in Bevy's order, with the old value | `ecs_queries.rs` `discard_sees_the_old_value`, `despawn_observers_run_in_bevy_order` | — |
| Entity-scoped observers (`entity(e).observe`) | idiomatic, TS and Rust entity events | `rust_events.rs` `an_entity_observer_runs_for_its_entity_only_and_dies_with_it`, `an_entity_observer_of_a_rust_event_runs_for_its_entity_only` | — |
| Observers on Rust events (`Pointer*`, `WorldInstanceReady`, `FocusGained`) | idiomatic (`On<E>`); `FocusGained`, `WorldInstanceReady`: registration check only | `rust_events.rs` `a_global_observer_sees_a_bubbling_pointer_click`, `propagate_false_drops_the_rest_of_a_rust_triggers_hops` | — |
| Component hooks | not needed: a hook runs inside the structural change, where JS cannot; observers with the copied value cover it (`docs/ecs.md:215`) | — | — |
| Required components on TS components | idiomatic: `static require`, transitive, cycle error | `ecs_queries.rs` `required_components_from_every_path` | — |
| States, `NextState`, `OnEnter`/`OnExit`, `inState` | idiomatic | `surface.rs` `entering_playing_runs_exit_then_enter_and_unlocks_systems` | — |
| `SubStates`, `ComputedStates`, `PreviousState`, `OnTransition`, `state_changed`, `state_exists` | idiomatic; `PreviousState` undefined for sub and computed states | `states.rs` `a_sub_state_exists_only_in_its_source_value_and_ignores_next_state_otherwise`, `a_computed_state_is_computed_only_when_a_source_transitions`, `on_transition_runs_on_exactly_its_transition_and_previous_state_reads_the_last_value`, `state_changed_is_true_on_the_frame_of_a_transition_only` | — |
| State-scoped entities (`DespawnOnExit`, `DisableOnExit`…) | idiomatic | `states.rs` `state_scoped_components_despawn_on_exit` | — |
| Rust states in TS, Rust reading TS states | idiomatic | `states.rs` `a_ts_system_gates_on_a_rust_state`, `rust_reads_a_ts_state_and_its_transition_event`, `a_rust_next_state_write_runs_ts_transition_systems` | — |
| Schedules `Startup`, `PreUpdate`, `Update`, `FixedUpdate`, `PostUpdate` | idiomatic | `surface.rs` `startup_runs_once_per_identity`; `systems.rs` `chain_after_before_sets_and_stages_order_systems` | — |
| Schedules `First`, `Last`, `PreStartup`, `PostStartup`, the fixed ones | idiomatic; a stage made on first use | `schedules.rs` `a_system_in_last_runs_after_post_update_and_its_stage_is_made_on_load`, `fixed_schedules_run_in_order_once_per_step`, `startup_lists_run_in_order_once_with_commands_between` | — |
| Schedules `RunFixedMainLoop`, `SpawnScene` | idiomatic; their ordering untested | `schedules.rs` `an_unused_schedule_has_no_bevy_ts_system` | — |
| Custom schedules, Rust's with `ts_schedule` | idiomatic | `schedules.rs` `a_custom_schedule_after_update_moves_on_reload`, `run_schedule_runs_it_once_before_the_next_system`, `a_ports_schedule_runs_its_ts_systems_when_the_port_runs_it` | — |
| Sets and ordering, Rust anchors | idiomatic | `systems.rs` `chain_after_before_sets_and_stages_order_systems` | — |
| Conditions `inState`, `resourceExists`, `onMessage`, `not` | idiomatic, evaluated in Rust | `surface.rs` `resource_exists_and_not_conditions`, `a_skipped_system_costs_no_js_call` | — |
| Conditions `runOnce`, `resource*`, `anyWithComponent`, `anyComponentRemoved`, `anyMatchFilter`, `conditionChanged`/`To` | idiomatic, evaluated in Rust | `conditions.rs` `every_rust_condition_skips_with_no_call_into_js`, `resource_changed_runs_on_the_frame_rust_writes_it_and_costs_no_call_otherwise`, `run_once_survives_a_reload_and_resets_on_restart` | — |
| A TS predicate condition, `resource_exists_and` | idiomatic; one JS call per evaluation (~3 µs in a stage already running TS), read-only params checked at load | `conditions.rs` `a_predicate_gates_a_system_with_one_call_per_frame`, `a_writing_predicate_fails_the_load_at_its_function`, `a_group_shares_one_predicate_call_and_distributive_is_one_each`, `resource_exists_and_calls_the_predicate_only_while_it_exists`, `rust_skipped_systems_and_predicates_make_no_call` | — |
| Combinators and collective/distributive `runIf` | idiomatic | `conditions.rs` `short_circuit_skips_b_and_eager_spends_it`, `collective_run_if_is_one_evaluation_and_distributive_is_one_each` | — |
| Rust conditions in TS (`ts_condition`, `nativeCondition`) | idiomatic | `conditions.rs` `a_ports_condition_runs_in_rust`, `a_factory_error_fails_the_load_with_the_call` | — |
| One-shot systems, Rust's `RunTsSystem` | idiomatic | `schedules.rs` `run_system_runs_a_one_shot_after_the_callers_commands`, `run_system_with_passes_its_input`, `rust_runs_a_ts_one_shot_with_run_ts_system` | — |
| Exclusive world access | decline (JS stays batched; `docs/native.md:578`) | — | — |
| `Disabled` entities | idiomatic: queries honour `DefaultQueryFilters`; no test inserts `Disabled` from TS | `ecs_queries.rs` `probe_disabled_and_sparse_filters` | — |

## Transform, time, input, window

| Capability | Today | Evidence / test | Owner |
|---|---|---|---|
| `Transform` columns, `Vec3`/`Quat` helpers | idiomatic | `systems.rs` `a_vector_method_stores_the_tick_once` | — |
| `Transform.lookingAt` | idiomatic, a value for `spawn` (not a method on a live column) | `reflect_scripts.rs` `looking_at_matches_bevy` | — |
| `GlobalTransform` read; `TransformHelper` | `GlobalTransform` idiomatic: `bevy-ts` ships it typed, with Bevy 0.20's read methods (`translation()`, `rotation()`, `scale()`, `computeTransform()`, directions, `transformPoint`, `reparentedTo`, ...); writes refused. `TransformHelper` missing | `reflect_systems.rs` `global_transform_reads_as_bevys`, `a_computed_component_refuses_a_write`; `schedules.rs` `after_propagation_reads_this_frames_global_transform` | `TransformHelper`: open |
| `Res<Time>` | idiomatic, read-only (`docs/time.md`) | `surface.rs` `time_is_a_read_only_view`; `time.rs` `fixed_update_reads_timestep` | — |
| `Time<Real>`, `Time<Virtual>`, `Time<Fixed>` | partial: every clock reads; writes fail the load | `time.rs` `clocks_read_bevy_values`, `game_pause_survives_next_frame`, `clock_writes_fail_the_load` | script-time stage 2 |
| `Timer`, `Stopwatch`, `on_timer`, delayed commands | idiomatic: timers, Bevy's time conditions, and `commands.delayed()` on Bevy's own `DelayedCommandQueue` | `time.rs` `timer_matches_bevy`, `stopwatch_pauses`, `time_conditions_match_bevy`, `paused_follows_the_game_clock`; `delayed_commands.rs` `bevys_test_matches_rust`, `delays_follow_the_game_clock`, `every_command_kind_can_wait` | — |
| Keyboard (`ButtonInput<KeyCode>`, `ButtonInput<Key>`), `InputFocus`, `KeyCode`, `Key` | idiomatic: `Keys`, `LogicalKeys`, `InputFocus`, generated `KeyCode` and `Key` (`bevy-ts/input`) | `text_input.rs` `keys_and_input_focus_read_n_only_while_nothing_has_focus`, `logical_keys_read_what_the_key_types`; `rust_events.rs` `a_ts_reader_sees_keyboard_input_once` | — |
| Mouse buttons, motion, scroll, cursor position | idiomatic: `MouseButtons`, `Cursor`, `AccumulatedMouseMotion`/`Scroll` | `input_devices.rs` `mouse_buttons_read_bevys_button_input`, `the_cursor_is_the_primary_windows`, `mouse_motion_and_scroll_read_as_bevys_resources` | — |
| Touch, gestures | idiomatic: `Touches`; gestures as messages | `input_devices.rs` `touches_list_by_index_with_bevys_positions`; `rust_events.rs` `input_messages_reach_a_ts_reader` | — |
| Gamepads (buttons, axes, sticks, rumble) | idiomatic: `Gamepads`, rumble by `MessageWriter`; `GamepadSettings` writes missing | `input_devices.rs` `gamepads_read_each_pads_state`; `rust_events.rs` `a_ts_writer_sends_a_rumble_request_a_rust_reader_sees` | `GamepadSettings`: open |
| `input_just_pressed` conditions | idiomatic: `inputPressed`, `inputJustPressed`, `inputJustReleased`, `inputToggleActive` | `input_devices.rs` `input_conditions_run_in_rust_and_skipped_systems_cost_no_call`, `input_toggle_active_flips_on_each_press` | — |
| Actions and bindings | not core Bevy; a leafwing port is planned | `docs/research/native-ports.md` | — (port) |
| Tab and directional navigation, focus events | idiomatic, opt-in in `bevy-ts.json` | `ui_focus.rs` `tab_moves_through_three_buttons_with_focus_events`, `a_modal_tab_group_traps_focus`, `arrow_keys_move_focus_through_directional_navigation` (macOS only) | — |
| `Window`, `CursorOptions` | partial: flags are columns, title, resolution and position read and write as holes; no test touches `Window` from TS | `reflect_systems.rs` `a_hole_reads_and_writes_and_column_views_stay_valid` (a stand-in) | open (a test) |
| Window messages | idiomatic (`MessageReader`) | `rust_events.rs` `window_messages_reach_a_ts_reader` | — |
| `CursorIcon` | idiomatic, no test (`bevy.d.ts:1929`) | — | — |

## Assets and scenes

| Capability | Today | Evidence / test | Owner |
|---|---|---|---|
| Handles as asset paths in TSX and levels | idiomatic | `reflect_assets.rs` `a_texture_path_is_the_image_load_of_its_value` | — |
| Hot reload of files | idiomatic (dev only); textures and glTF: no test | `bevy-ts-levels/tests/it/levels.rs` `an_included_file_reloads_every_includer_copy` | — |
| `AssetServer.load`, load state, `isLoadedWithDependencies` | idiomatic (`bevy-ts/assets`) | `script_assets.rs` `a_texture_loads_and_reaches_loaded`, `a_missing_file_fails_with_bevys_error`, `a_gltf_waits_for_its_dependencies` | — |
| `AssetEvent`, `AssetLoadFailedEvent` | `AssetEvent<A>` idiomatic; `AssetLoadFailedEvent` has no `Reflect` in Bevy 0.20, failure reads from `loadState` | `rust_events.rs` `asset_event_image_reaches_a_ts_reader`; `script_assets.rs` `a_load_is_seen_as_an_event`, `a_mesh_update_is_seen_as_modified` | — (upstream) |
| `Assets<T>` add and mutate from TS | partial: meshes, reflected assets (materials) and images from bytes (`images.add(Image.new(...))`, `setData`, `writeRect`; 2D, four 8-bit formats) idiomatic; buffers from bytes (`ShaderBuffer`), custom vertex attributes and morph targets missing | `script_assets.rs` `a_quad_matches_the_same_rust_calls`, `a_mesh_reads_back_as_bevy_named_json`, `remove_frees_at_once`; `script_images.rs`; `reflect_assets.rs` `a_system_adds_reads_and_sets_a_material` | open |
| Processing, meta files, custom loaders | Rust-side by nature | — | — |
| `.bsn` levels, TSX scenes, `SceneComponent` prefabs | idiomatic | `bevy-ts-levels/tests/it/levels.rs` | — |
| glTF scenes (`WorldAssetRoot`) | idiomatic in TSX and levels | `animation.rs` `a_tsx_fox_plays_and_its_bound_clip_cross_fades`, `a_level_fox_plays_with_no_ts` | — |
| `Gltf` asset fields (named scenes, nodes, animations), extras | partial: extras are reflect components; `Gltf` loads but has no reflection, so its fields are unreadable | `script_assets.rs` `a_gltf_waits_for_its_dependencies` | open |
| `DynamicWorld` save and load | idiomatic (`docs/saves.md`) | `saves.rs` `a_script_saves_and_loads_in_the_dev_folder`, `the_whole_arena_saves` | — |

## Render, PBR, core pipeline

| Capability | Today | Evidence / test | Owner |
|---|---|---|---|
| Cameras: `Camera`, `Camera3d`, `Hdr`, `RenderTarget`, `Viewport` | idiomatic; `Hdr`, `RenderTarget`: no test | `reflect_render.rs` `an_orthographic_camera_draws_near_and_far_cubes_the_same_size` | — |
| `Projection`, `ScalingMode` | idiomatic as a value (binding: see the ECS row on bound reflect props) | `reflect_values.rs` `an_orthographic_projection_in_tsx_and_its_reloads`, `a_bad_variant_or_key_fails_the_load_naming_it` | — |
| World to viewport | idiomatic: `Cameras.worldToViewport` | `cameras.rs` `world_points_project_to_logical_pixels` | — |
| Viewport to world, `MeshRayCast` | idiomatic | `cameras.rs` `viewport_points_convert_to_rays_and_back`, `rays_hit_meshes_nearest_first` | — |
| Lights | idiomatic, colour included; only `DirectionalLight` tested | `reflect_values.rs` `a_light_colour_is_a_prop_written_through_reflection`; `reflect_render.rs` `a_warm_light_colour_warms_the_lit_pixel` | — |
| `AmbientLight` | idiomatic | `ambient_light.rs` `a_script_inserted_ambient_light_keeps_its_white_and_its_brightness_lights_the_scene` | — |
| Environment maps, skybox, light probes, fog volumes | idiomatic; only `Skybox` tested | `reflect_values.rs` `an_asset_path_at_a_handle_loads_it_from_tsx_and_of` | — |
| `<material>` color, metallic, roughness, emissive, unlit | idiomatic | `bevy-ts-tsx/tests/it/standard_material.rs` `every_prop_sets_its_field` | — |
| `alpha`, `alphaMode` | idiomatic | `materials.rs` `a_material_alpha_blends_by_default_and_an_explicit_mode_wins`, `a_bound_material_alpha_fades_a_shape_frame_by_frame` | — |
| The other `StandardMaterial` fields | partial: 18 texture and channel fields blocked by Bevy's 16-sampler limit (`docs/materials.md`) | `bevy-ts-tsx/tests/it/standard_material.rs` `every_texture_loads_its_path_in_its_colour_space`, `the_table_covers_standard_material`; `standard_material_limits.rs` `every_texture_fits_wgpus_default_limits` | open (upstream) |
| Script materials | idiomatic | `materials.rs` `a_material_draws_with_its_file_and_values`; `material_params_render.rs` `a_set_redraws_the_same_asset_in_its_new_colour` | — |
| Script material on a glTF scene | idiomatic | `scene_material.rs` `a_scene_mesh_carries_the_script_material` | — |
| Post-process settings | idiomatic; no per-type test | `reflect_coverage.rs` `codec_output_equals_read_for_every_covered_type` | — |
| `RenderLayers`, `Visibility`, `NoFrustumCulling`, shadow markers, wireframes | idiomatic, no test (reflected classes) | `visibility_range.rs` `brp_inserts_and_gets_visibility_range` (`VisibilityRange` only) | — |
| `OcclusionCulling` | idiomatic; removing `DepthPrepass` alone still crashes (bevyengine/bevy#22747) | `occlusion_render.rs` `occlusion_culling_alone_brings_a_depth_prepass_and_draws` | — |
| `<mesh>` primitives | partial: every 3D and 2D area shape; line shapes and rings other than the circle missing (`docs/meshes.md`) | `bevy-ts-tsx/tests/it/primitives.rs` `every_shape_builds_a_valid_mesh`, `builder_settings_reach_bevy` | open |
| Custom meshes from TS | idiomatic | `script_assets.rs` `every_attribute_round_trips`, `a_chunk_remesh_is_six_host_calls`; `bevy-ts-tsx/tests/it/vertex_mesh.rs` `a_quad_from_props_equals_bevys_calls` | — |

## 2D, text, UI, picking

| Capability | Today | Evidence / test | Owner |
|---|---|---|---|
| `Camera2d`, `Sprite`, `Anchor` | idiomatic | `two_d.rs` `a_camera2d_is_orthographic_and_draws_a_sprite`, `a_sprite_takes_an_image_path_and_flips` | — |
| 2D in the editor | missing: the editor camera is a `Camera3d` (`bevy-ts-viewport/src/edit.rs:613`), so 2D is not drawn while editing; `viewport_mode camera:"game"` shows it with the tools off | — | open (script-2d's non-goal, `openspec/changes/script-2d/proposal.md:21`) |
| `Mesh2d` + `ColorMaterial`, 2D primitives | idiomatic: `<mesh2d>`, `<material2d>` | `two_d.rs` `every_2d_shape_builds_a_flat_mesh2d`, `a_material2d_builds_its_color_material_and_equal_props_share_handles` | — |
| Texture atlases, sprite sheet animation | partial: `TextureAtlasLayout.fromGrid`, one layout cache for sprites and UI; sheet animation (a system on a `Timer`) untested | `two_d.rs` `sprites_and_a_ui_image_given_one_grid_share_one_32_tile_layout`, `a_sprite_atlas_index_past_its_grid_names_index_and_count`; `ui_media.rs` `equal_atlas_grids_share_one_layout` | script-2d stage 4 (animation) |
| `Text2d`, `TilemapChunk` | partial: `<text2d>` idiomatic; `TilemapChunk` missing | `two_d.rs` `a_bound_text2d_writes_only_its_text2d_and_its_span_is_a_child` | script-2d stage 5 (`TilemapChunk`) |
| `<worldText>` in the 3D world | idiomatic for straight labels; `path` and `normal` fail the load | `bevy-ts-material/tests/it/world_text.rs`, `world_text_render.rs` | world-text-stage-2 (path text) |
| UI nodes, style, text, images, buttons | idiomatic | `ui_style.rs` `grid_tracks_and_placements`; `ui_text.rs` `each_text_prop_writes_its_component`; `ui_media.rs` `sliced_tiled_and_stretch_modes` | — |
| `<input>` | idiomatic | `text_input.rs` `typed_keys_then_enter_submit_the_text`, `escape_cancels_and_clears_focus` | — |
| UI widgets | idiomatic | `ui_widgets.rs` `a_slider_and_its_thumb_have_bevys_components`, `a_tab_list_selects_by_key`; `ui_overlays.rs` `overlays_have_bevys_components` | — |
| UI materials (`MaterialNode`), `bevy_feathers` | missing, not asked for (`docs/ui.md` "Not available"); bevy-ui-coverage archived without them | — | open (ask-first: a new material target and pipeline) |
| Picking events on entities | idiomatic: every `onPointer*`, bubbling; 3D by `meshPicking`; 2D sprites | `ui_events.rs` `every_picking_event_reaches_its_handler_with_bevys_fields`; `ui_drag.rs` `a_drag_reaches_a_ui_node_a_mesh_and_a_scene_root`; `mesh_picking.rs` `the_headless_pointer_clicks_a_mesh`; `headless_pointer.rs` `a_test_clicks_a_mesh_headless`; `two_d.rs` `clicking_a_sprite_runs_its_handler_once_with_its_entity` | — |
| `Pickable`, `PickingInteraction`, `PointerInteraction`, `Hovered` | idiomatic, no test: reflected classes. No test reads `PointerInteraction` under the headless pointer | `ui_events.rs` `has_pressed_styles_a_button_with_one_call_per_change_and_none_when_quiet` (`Pressed` only) | open (a test) |

## Gizmos, animation, audio, diagnostics, remote, reflect

| Capability | Today | Evidence / test | Owner |
|---|---|---|---|
| Immediate `Gizmos` | idiomatic ([docs/gizmos.md](../gizmos.md)) | `gizmos.rs` `a_system_draws_lines`, `ts_draws_what_rust_draws`, `gizmos_render_headless` | — |
| Gizmo config, `GizmoAsset`, `Show*Gizmo` | idiomatic | `gizmos.rs` `config_round_trips_through_the_store`, `a_retained_gizmo_renders`, `show_aabb_gizmo_draws_its_box` | — |
| Play a glTF clip with transitions | idiomatic: `PlayAnimation` | `animation.rs` `a_tsx_fox_plays_and_its_bound_clip_cross_fades` | — |
| `AnimationPlayer` control, graphs, TS-built clips | idiomatic: `Animations` (`status`, `seek`, `rewind`, `replay`), graphs and clips built in TS, tweens | `animation.rs` `animations_status`, `animations_seek_rewind_replay`, `a_ts_built_graph_blends`; `bevy-ts-animation/tests/it/clip.rs` `a_tween_translates_with_an_ease` | — |
| Animation events, `adjust_speeds`, `set_weight` | decline: Rust closures (`docs/animation.md`) | — | — |
| `AudioPlayer`, `PlaybackSettings`, `SpatialListener` | idiomatic; mp3 refused by name ([docs/audio.md](../audio.md)) | `audio.rs` `tsx_writes_audio_player_and_playback_settings`, `a_script_spawns_audio_player_and_playback_settings` | — |
| `AudioSink` control, `GlobalVolume` | idiomatic | `audio.rs` `audio_sinks_control_sinks_with_no_device`, `global_volume_and_default_spatial_scale_from_ts` | — |
| Diagnostics, `FrameCount` | idiomatic (`docs/diagnostics.md`) | `diagnostics.rs` `ts_reads_the_store_and_the_frame_count_as_rust_does`, `a_script_diagnostic_is_measured_and_survives_a_reload` | — |
| BRP (studio, protocol) | idiomatic for tools | `protocol.rs` `a_watch_stays_quiet_for_rows_a_system_does_not_write` | — |
| Custom BRP methods from TS | decline for now (low demand) | — | — |
| Reflect shapes, hole walker | idiomatic, with a named list (`bevy-ts/src/reflect_value.rs:49` `UNCOVERED`) | `reflect_coverage.rs` `prebuilt_engine_has_no_unlisted_gap`; `bevy-ts/src/reflect_value.rs` `every_kind_round_trips` | — |

## Docs and specs that claim more than the code

The four from 2026-10-23 are fixed (rust-resources, script-time stage 1, reflect-coverage stages 1
and 2), and the three from 2026-10-26 too (reflect-bindable: the ts-systems "Resources" line, the
ignored after-propagation test, script-2d research's claim that reflect-path props cannot be bound).
One is left, in another change's files:

- `openspec/changes/script-2d/design.md` says reflect-path props cannot be bound. Reflect-tier props
  bind (`reflect_values.rs` `a_bound_projection_follows_its_signal`).

## Open, ranked

Missing or partial, no running owner, ranked by which a game team waits on.

1. `PointerInteraction` under the headless pointer: untested; 3D mesh picking is tested only
   through observers. Agent town waits; its build is bisecting whether the empty value is ours.
2. 2D in the editor: the editor camera is a `Camera3d`. script-2d waits and lists it as a non-goal.
3. `Gltf` asset fields unreadable (no reflection).
4. A project's own Rust messages are not emitted to `native.d.ts`.
5. `Image` pixel data from TS (procedural textures); custom vertex attributes and morph targets.
6. `<mesh>` line shapes and rings.
7. Hierarchy sibling-order helpers.
8. `GamepadSettings` writes.
9. UI materials (`MaterialNode`): ask-first, nobody has asked.
10. The 18 `StandardMaterial` textures: upstream, Bevy's sampler limit; no issue filed.
11. Test-only gaps, no code missing: `Window`/`CursorOptions` from TS, `Name`/`Disabled` from
    commands, the `Insert` observer, `RenderTarget`, `Hdr`, `CursorIcon`, render markers.

Owned by a running change, so not ranked: heap fields (ts-heap-fields-2), `iterDescendants`
(ecs-queries 6.1), custom relationships, clock
writes (script-time stage 2), sprite-sheet animation and tilemaps (script-2d), path text
(world-text-stage-2). heimdall-labels has
named no ask.

## Sources

- bevy-ts `main` at `50340d9ef`: `packages/bevy-ts/*.d.ts`, `runtime`, `crates/*/src`,
  `crates/*/tests/it`, `docs/*.md`, `openspec/specs`, `openspec/changes` and its archive.
- `abyme call teams list` on 2026-10-27; game-team asks relayed by lead@engine-core.
- Bevy 0.20.0 sources in the cargo registry (the 2026-10-23 audit's list).

Confidence: high for rows with a named test (each grepped and not ignored; none was run); medium
for "no test" rows, which rest on the type existing. Not checked: running teams' unlanded
branches.
