//! The prebuilt engine's Rust messages and events for TS: Bevy's own set, registered with
//! bevy-ts's type data so a TS system can read, write, observe and trigger them by type path.
//! `tests/rust_events_coverage.rs` fails when Bevy has one this list leaves out.

use bevy::asset::AssetEvent;
use bevy::input::gamepad::*;
use bevy::input::gestures::*;
use bevy::input::keyboard::{KeyboardFocusLost, KeyboardInput};
use bevy::input::mouse::{MouseButtonInput, MouseMotion, MouseWheel};
use bevy::input::touch::TouchInput;
use bevy::input_focus::{AcquireFocus, FocusGained, FocusLost, FocusedInput};
use bevy::picking::backend::PointerHits;
use bevy::picking::events::*;
use bevy::picking::pointer::PointerInput;
use bevy::prelude::*;
use bevy::render::storage::ShaderBuffer;
use bevy::render::view::window::screenshot::ScreenshotCaptured;
use bevy::sprite_render::{ColorMaterial, SpriteMeshMaterial};
use bevy::ui_widgets::{Activate, MenuEvent, RequestClose, ReselectListRow, SetChecked, SetSelected, SetSliderValue, ToggleChecked, ValueChange};
use bevy::window::*;
use bevy::world_serialization::WorldInstanceReady;
use bevy_ts::rust_events::{register_entity_event as entity_event, register_message as message};

/// Adds [`register_bevy`] to an app.
pub struct RustEventsPlugin;

impl Plugin for RustEventsPlugin {
    fn build(&self, app: &mut App) {
        register_bevy(app);
    }
}

/// Registers Bevy's window, input, picking, focus, UI widget, scene, render and asset messages and
/// events.
pub fn register_bevy(app: &mut App) {
    // Window.
    message::<WindowResized>(app);
    message::<RequestRedraw>(app);
    message::<WindowCreated>(app);
    message::<WindowCloseRequested>(app);
    message::<WindowClosed>(app);
    message::<WindowClosing>(app);
    message::<WindowDestroyed>(app);
    message::<CursorMoved>(app);
    message::<CursorEntered>(app);
    message::<CursorLeft>(app);
    message::<Ime>(app);
    message::<WindowFocused>(app);
    message::<WindowOccluded>(app);
    message::<WindowScaleFactorChanged>(app);
    message::<WindowBackendScaleFactorChanged>(app);
    message::<FileDragAndDrop>(app);
    message::<WindowMoved>(app);
    message::<WindowThemeChanged>(app);
    message::<AppLifecycle>(app);
    message::<WindowEvent>(app);
    message::<AppExit>(app);

    // Input.
    message::<KeyboardInput>(app);
    message::<KeyboardFocusLost>(app);
    message::<MouseButtonInput>(app);
    message::<MouseMotion>(app);
    message::<MouseWheel>(app);
    message::<TouchInput>(app);
    message::<PinchGesture>(app);
    message::<RotationGesture>(app);
    message::<DoubleTapGesture>(app);
    message::<PanGesture>(app);
    message::<GamepadEvent>(app);
    message::<RawGamepadEvent>(app);
    message::<RawGamepadButtonChangedEvent>(app);
    message::<RawGamepadAxisChangedEvent>(app);
    message::<GamepadConnectionEvent>(app);
    message::<GamepadButtonStateChangedEvent>(app);
    message::<GamepadButtonChangedEvent>(app);
    message::<GamepadAxisChangedEvent>(app);
    message::<GamepadRumbleRequest>(app);

    // Picking: each pointer event is both an entity event and a message.
    macro_rules! pointer {
        ($($t:ty),* $(,)?) => {$(
            message::<$t>(app);
            entity_event::<$t>(app);
        )*};
    }
    pointer!(
        PointerCancel,
        PointerOver,
        PointerEnter,
        PointerOut,
        PointerLeave,
        PointerPress,
        PointerRelease,
        PointerClick,
        PointerMove,
        PointerDragStart,
        PointerDrag,
        PointerDragEnd,
        PointerDragEnter,
        PointerDragOver,
        PointerDragLeave,
        PointerDragDrop,
        PointerScroll,
    );
    message::<PointerInput>(app);
    message::<PointerHits>(app);

    // Focus.
    entity_event::<FocusGained>(app);
    entity_event::<FocusLost>(app);
    entity_event::<AcquireFocus>(app);
    entity_event::<FocusedInput<KeyboardInput>>(app);

    // UI widgets. `ValueChange<T>` for the values Bevy's own widgets send.
    entity_event::<Activate>(app);
    entity_event::<ValueChange<bool>>(app);
    entity_event::<ValueChange<f32>>(app);
    entity_event::<ValueChange<Entity>>(app);
    entity_event::<ValueChange<Option<Entity>>>(app);
    entity_event::<SetChecked>(app);
    entity_event::<ToggleChecked>(app);
    entity_event::<RequestClose>(app);
    entity_event::<ReselectListRow>(app);
    entity_event::<SetSelected>(app);
    entity_event::<MenuEvent>(app);
    entity_event::<SetSliderValue>(app);

    // Rendering.
    entity_event::<ScreenshotCaptured>(app);

    // Scenes.
    entity_event::<WorldInstanceReady>(app);

    // Assets.
    message::<AssetEvent<Image>>(app);
    message::<AssetEvent<Mesh>>(app);
    message::<AssetEvent<StandardMaterial>>(app);
    message::<AssetEvent<Font>>(app);
    message::<AssetEvent<Shader>>(app);
    message::<AssetEvent<AnimationClip>>(app);
    message::<AssetEvent<AnimationGraph>>(app);
    message::<AssetEvent<Gltf>>(app);
    message::<AssetEvent<TextureAtlasLayout>>(app);
    message::<AssetEvent<ShaderBuffer>>(app);
    message::<AssetEvent<ColorMaterial>>(app);
    message::<AssetEvent<SpriteMeshMaterial>>(app);
}
