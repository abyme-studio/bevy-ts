//! The hole check for Rust messages and events (openspec rust-events, decision 10): every type the
//! prebuilt engine registers with Bevy's `ReflectMessage` or `ReflectEvent` also has bevy-ts's
//! type data, every listed input, window, picking, focus, scene and asset type is registered, and
//! every reflected asset type's `AssetEvent` is readable.

use bevy::asset::ReflectAsset;
use bevy::ecs::reflect::{ReflectEvent, ReflectMessage};
use bevy::reflect::TypeRegistration;
use bevy_ts::engine::EngineIdentity;
use bevy_ts::rust_events::{ReflectTsEvent, ReflectTsMessage};
use bevy_ts_cli::Engine;

/// Types Bevy reflects as messages or events that TS cannot take, with the reason.
const ALLOWED: &[(&str, &str)] = &[];

/// Bevy messages and events with no `Reflect` in 0.20, so no registration can reach them. Kept
/// as the record of what TS cannot see.
const UNREFLECTED: &[(&str, &str)] = &[
    ("bevy_state::state::transitions::StateTransitionEvent<S>", "no Reflect in Bevy 0.20"),
    ("bevy_asset::event::AssetLoadFailedEvent<A>", "no Reflect in Bevy 0.20"),
    ("bevy_asset::event::UntypedAssetLoadFailedEvent", "no Reflect in Bevy 0.20"),
    ("bevy_scene::Ready", "no Reflect in Bevy 0.20; WorldInstanceReady is the reflected one"),
];

/// Types that must be registered for TS: a sample of each family.
const REQUIRED: &[&str] = &[
    "bevy_input::keyboard::KeyboardInput",
    "bevy_input::mouse::MouseButtonInput",
    "bevy_input::mouse::MouseMotion",
    "bevy_input::mouse::MouseWheel",
    "bevy_input::touch::TouchInput",
    "bevy_input::gestures::PinchGesture",
    "bevy_input::gamepad::GamepadEvent",
    "bevy_input::gamepad::GamepadRumbleRequest",
    "bevy_window::event::WindowResized",
    "bevy_window::event::CursorMoved",
    "bevy_window::event::FileDragAndDrop",
    "bevy_window::event::RequestRedraw",
    "bevy_picking::events::PointerClick",
    "bevy_picking::events::PointerMove",
    "bevy_input_focus::gained_and_lost::FocusGained",
    "bevy_world_serialization::world_asset_spawner::WorldInstanceReady",
    "bevy_asset::event::AssetEvent<bevy_image::image::Image>",
    "bevy_asset::event::AssetEvent<bevy_mesh::mesh::Mesh>",
];

fn path(r: &TypeRegistration) -> &str {
    r.type_info().type_path()
}

#[test]
fn every_bevy_message_and_event_is_registered_for_ts() {
    let engine = Engine { identity: EngineIdentity::prebuilt(bevy_ts_cli::engine::VERSION, bevy_ts_cli::engine::TARGET) };
    let app = bevy_ts_cli::types::types_app(&engine, bevy_ts_cli::Prebuilt);
    let registry = app.world().resource::<bevy::ecs::reflect::AppTypeRegistry>().read();
    let allowed = |p: &str| ALLOWED.iter().any(|(a, _)| *a == p);
    let mut holes = Vec::new();
    for r in registry.iter() {
        let p = path(r);
        if allowed(p) {
            continue;
        }
        if r.data::<ReflectMessage>().is_some() && r.data::<ReflectTsMessage>().is_none() {
            holes.push(format!("{p}: a message without ts_message"));
        }
        if r.data::<ReflectEvent>().is_some() && r.data::<ReflectTsEvent>().is_none() {
            holes.push(format!("{p}: an event without ts_event or ts_entity_event"));
        }
        if r.data::<ReflectAsset>().is_some() {
            let event = format!("bevy_asset::event::AssetEvent<{p}>");
            if !allowed(&event) && registry.get_with_type_path(&event).is_none_or(|e| e.data::<ReflectTsMessage>().is_none()) {
                holes.push(format!("{event}: an asset type whose AssetEvent is not registered"));
            }
        }
    }
    for p in REQUIRED {
        let Some(r) = registry.get_with_type_path(p) else {
            holes.push(format!("{p}: not registered"));
            continue;
        };
        if r.data::<ReflectTsMessage>().is_none() && r.data::<ReflectTsEvent>().is_none() {
            holes.push(format!("{p}: registered without bevy-ts type data"));
        }
    }
    for (p, _) in ALLOWED {
        if registry.get_with_type_path(p).is_none_or(|r| r.data::<ReflectTsMessage>().is_some() || r.data::<ReflectTsEvent>().is_some()) {
            holes.push(format!("{p}: stale allowlist entry"));
        }
    }
    holes.sort();
    assert!(holes.is_empty(), "{} holes:\n{}\n(no Reflect, so out of reach: {UNREFLECTED:?})", holes.len(), holes.join("\n"));
}
