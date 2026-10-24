//! W3C `KeyboardEvent.code` and `key` values to Bevy's `KeyCode` and `Key` and back, as winit
//! maps them, and the numbers `bevy-ts/input` gives each `KeyCode`, `MouseButton`,
//! `GamepadButton` and `GamepadAxis`. One owner for the viewport's key events, the TSX keyboard
//! events and the input enums in TS.
//! Generated from bevy_winit 0.20's converters; W3C names that winit spells differently are
//! aliased, the W3C name first.

use bevy::input::gamepad::{GamepadAxis, GamepadButton};
use bevy::input::keyboard::{Key, KeyCode, NativeKey};
use bevy::input::mouse::MouseButton;

/// Every `KeyCode` without data, in Bevy's order: W3C `code` names (the first is the one
/// [`code_name`] gives), then the variant. A variant's index here is its number in TS.
macro_rules! codes {
    ($($code:literal $(| $alias:literal)* => $variant:ident,)*) => {
        /// The `KeyCode` for a W3C `code`, `None` for one winit does not know.
        pub fn key_code(code: &str) -> Option<KeyCode> {
            Some(match code {
                $($code $(| $alias)* => KeyCode::$variant,)*
                _ => return None,
            })
        }

        /// The W3C `code` for a `KeyCode`; `"Unidentified"` for one carrying a native code.
        pub fn code_name(code: KeyCode) -> &'static str {
            match code {
                $(KeyCode::$variant => $code,)*
                KeyCode::Unidentified(_) => "Unidentified",
            }
        }

        /// `KeyCode`'s variants without data, by name; the index is the number TS uses.
        pub const KEY_CODES: &[(&str, KeyCode)] = &[$((stringify!($variant), KeyCode::$variant),)*];
    };
}

codes! {
    "Backquote" => Backquote,
    "Backslash" => Backslash,
    "BracketLeft" => BracketLeft,
    "BracketRight" => BracketRight,
    "Comma" => Comma,
    "Digit0" => Digit0,
    "Digit1" => Digit1,
    "Digit2" => Digit2,
    "Digit3" => Digit3,
    "Digit4" => Digit4,
    "Digit5" => Digit5,
    "Digit6" => Digit6,
    "Digit7" => Digit7,
    "Digit8" => Digit8,
    "Digit9" => Digit9,
    "Equal" => Equal,
    "IntlBackslash" => IntlBackslash,
    "IntlRo" => IntlRo,
    "IntlYen" => IntlYen,
    "KeyA" => KeyA,
    "KeyB" => KeyB,
    "KeyC" => KeyC,
    "KeyD" => KeyD,
    "KeyE" => KeyE,
    "KeyF" => KeyF,
    "KeyG" => KeyG,
    "KeyH" => KeyH,
    "KeyI" => KeyI,
    "KeyJ" => KeyJ,
    "KeyK" => KeyK,
    "KeyL" => KeyL,
    "KeyM" => KeyM,
    "KeyN" => KeyN,
    "KeyO" => KeyO,
    "KeyP" => KeyP,
    "KeyQ" => KeyQ,
    "KeyR" => KeyR,
    "KeyS" => KeyS,
    "KeyT" => KeyT,
    "KeyU" => KeyU,
    "KeyV" => KeyV,
    "KeyW" => KeyW,
    "KeyX" => KeyX,
    "KeyY" => KeyY,
    "KeyZ" => KeyZ,
    "Minus" => Minus,
    "Period" => Period,
    "Quote" => Quote,
    "Semicolon" => Semicolon,
    "Slash" => Slash,
    "AltLeft" => AltLeft,
    "AltRight" => AltRight,
    "Backspace" => Backspace,
    "CapsLock" => CapsLock,
    "ContextMenu" => ContextMenu,
    "ControlLeft" => ControlLeft,
    "ControlRight" => ControlRight,
    "Enter" => Enter,
    "MetaLeft" | "OSLeft" | "SuperLeft" => SuperLeft,
    "MetaRight" | "OSRight" | "SuperRight" => SuperRight,
    "ShiftLeft" => ShiftLeft,
    "ShiftRight" => ShiftRight,
    "Space" => Space,
    "Tab" => Tab,
    "Convert" => Convert,
    "KanaMode" => KanaMode,
    "Lang1" => Lang1,
    "Lang2" => Lang2,
    "Lang3" => Lang3,
    "Lang4" => Lang4,
    "Lang5" => Lang5,
    "NonConvert" => NonConvert,
    "Delete" => Delete,
    "End" => End,
    "Help" => Help,
    "Home" => Home,
    "Insert" => Insert,
    "PageDown" => PageDown,
    "PageUp" => PageUp,
    "ArrowDown" => ArrowDown,
    "ArrowLeft" => ArrowLeft,
    "ArrowRight" => ArrowRight,
    "ArrowUp" => ArrowUp,
    "NumLock" => NumLock,
    "Numpad0" => Numpad0,
    "Numpad1" => Numpad1,
    "Numpad2" => Numpad2,
    "Numpad3" => Numpad3,
    "Numpad4" => Numpad4,
    "Numpad5" => Numpad5,
    "Numpad6" => Numpad6,
    "Numpad7" => Numpad7,
    "Numpad8" => Numpad8,
    "Numpad9" => Numpad9,
    "NumpadAdd" => NumpadAdd,
    "NumpadBackspace" => NumpadBackspace,
    "NumpadClear" => NumpadClear,
    "NumpadClearEntry" => NumpadClearEntry,
    "NumpadComma" => NumpadComma,
    "NumpadDecimal" => NumpadDecimal,
    "NumpadDivide" => NumpadDivide,
    "NumpadEnter" => NumpadEnter,
    "NumpadEqual" => NumpadEqual,
    "NumpadHash" => NumpadHash,
    "NumpadMemoryAdd" => NumpadMemoryAdd,
    "NumpadMemoryClear" => NumpadMemoryClear,
    "NumpadMemoryRecall" => NumpadMemoryRecall,
    "NumpadMemoryStore" => NumpadMemoryStore,
    "NumpadMemorySubtract" => NumpadMemorySubtract,
    "NumpadMultiply" => NumpadMultiply,
    "NumpadParenLeft" => NumpadParenLeft,
    "NumpadParenRight" => NumpadParenRight,
    "NumpadStar" => NumpadStar,
    "NumpadSubtract" => NumpadSubtract,
    "Escape" => Escape,
    "Fn" => Fn,
    "FnLock" => FnLock,
    "PrintScreen" => PrintScreen,
    "ScrollLock" => ScrollLock,
    "Pause" => Pause,
    "BrowserBack" => BrowserBack,
    "BrowserFavorites" => BrowserFavorites,
    "BrowserForward" => BrowserForward,
    "BrowserHome" => BrowserHome,
    "BrowserRefresh" => BrowserRefresh,
    "BrowserSearch" => BrowserSearch,
    "BrowserStop" => BrowserStop,
    "Eject" => Eject,
    "LaunchApp1" => LaunchApp1,
    "LaunchApp2" => LaunchApp2,
    "LaunchMail" => LaunchMail,
    "MediaPlayPause" => MediaPlayPause,
    "MediaSelect" => MediaSelect,
    "MediaStop" => MediaStop,
    "MediaTrackNext" => MediaTrackNext,
    "MediaTrackPrevious" => MediaTrackPrevious,
    "Power" => Power,
    "Sleep" => Sleep,
    "AudioVolumeDown" => AudioVolumeDown,
    "AudioVolumeMute" => AudioVolumeMute,
    "AudioVolumeUp" => AudioVolumeUp,
    "WakeUp" => WakeUp,
    "Meta" => Meta,
    "Hyper" => Hyper,
    "Turbo" => Turbo,
    "Abort" => Abort,
    "Resume" => Resume,
    "Suspend" => Suspend,
    "Again" => Again,
    "Copy" => Copy,
    "Cut" => Cut,
    "Find" => Find,
    "Open" => Open,
    "Paste" => Paste,
    "Props" => Props,
    "Select" => Select,
    "Undo" => Undo,
    "Hiragana" => Hiragana,
    "Katakana" => Katakana,
    "F1" => F1,
    "F2" => F2,
    "F3" => F3,
    "F4" => F4,
    "F5" => F5,
    "F6" => F6,
    "F7" => F7,
    "F8" => F8,
    "F9" => F9,
    "F10" => F10,
    "F11" => F11,
    "F12" => F12,
    "F13" => F13,
    "F14" => F14,
    "F15" => F15,
    "F16" => F16,
    "F17" => F17,
    "F18" => F18,
    "F19" => F19,
    "F20" => F20,
    "F21" => F21,
    "F22" => F22,
    "F23" => F23,
    "F24" => F24,
    "F25" => F25,
    "F26" => F26,
    "F27" => F27,
    "F28" => F28,
    "F29" => F29,
    "F30" => F30,
    "F31" => F31,
    "F32" => F32,
    "F33" => F33,
    "F34" => F34,
    "F35" => F35,
}

/// The `Key` for a W3C `key` value: a named key, or the characters it types.
pub fn logical_key(key: &str) -> Key {
    match key {
        "Alt" => Key::Alt,
        "AltGraph" => Key::AltGraph,
        "CapsLock" => Key::CapsLock,
        "Control" => Key::Control,
        "Fn" => Key::Fn,
        "FnLock" => Key::FnLock,
        "NumLock" => Key::NumLock,
        "ScrollLock" => Key::ScrollLock,
        "Shift" => Key::Shift,
        "Symbol" => Key::Symbol,
        "SymbolLock" => Key::SymbolLock,
        "Meta" => Key::Meta,
        "Hyper" => Key::Hyper,
        "Super" => Key::Super,
        "Enter" => Key::Enter,
        "Tab" => Key::Tab,
        "Space" => Key::Space,
        "ArrowDown" => Key::ArrowDown,
        "ArrowLeft" => Key::ArrowLeft,
        "ArrowRight" => Key::ArrowRight,
        "ArrowUp" => Key::ArrowUp,
        "End" => Key::End,
        "Home" => Key::Home,
        "PageDown" => Key::PageDown,
        "PageUp" => Key::PageUp,
        "Backspace" => Key::Backspace,
        "Clear" => Key::Clear,
        "Copy" => Key::Copy,
        "CrSel" => Key::CrSel,
        "Cut" => Key::Cut,
        "Delete" => Key::Delete,
        "EraseEof" => Key::EraseEof,
        "ExSel" => Key::ExSel,
        "Insert" => Key::Insert,
        "Paste" => Key::Paste,
        "Redo" => Key::Redo,
        "Undo" => Key::Undo,
        "Accept" => Key::Accept,
        "Again" => Key::Again,
        "Attn" => Key::Attn,
        "Cancel" => Key::Cancel,
        "ContextMenu" => Key::ContextMenu,
        "Escape" => Key::Escape,
        "Execute" => Key::Execute,
        "Find" => Key::Find,
        "Help" => Key::Help,
        "Pause" => Key::Pause,
        "Play" => Key::Play,
        "Props" => Key::Props,
        "Select" => Key::Select,
        "ZoomIn" => Key::ZoomIn,
        "ZoomOut" => Key::ZoomOut,
        "BrightnessDown" => Key::BrightnessDown,
        "BrightnessUp" => Key::BrightnessUp,
        "Eject" => Key::Eject,
        "LogOff" => Key::LogOff,
        "Power" => Key::Power,
        "PowerOff" => Key::PowerOff,
        "PrintScreen" => Key::PrintScreen,
        "Hibernate" => Key::Hibernate,
        "Standby" => Key::Standby,
        "WakeUp" => Key::WakeUp,
        "AllCandidates" => Key::AllCandidates,
        "Alphanumeric" => Key::Alphanumeric,
        "CodeInput" => Key::CodeInput,
        "Compose" => Key::Compose,
        "Convert" => Key::Convert,
        "FinalMode" => Key::FinalMode,
        "GroupFirst" => Key::GroupFirst,
        "GroupLast" => Key::GroupLast,
        "GroupNext" => Key::GroupNext,
        "GroupPrevious" => Key::GroupPrevious,
        "ModeChange" => Key::ModeChange,
        "NextCandidate" => Key::NextCandidate,
        "NonConvert" => Key::NonConvert,
        "PreviousCandidate" => Key::PreviousCandidate,
        "Process" => Key::Process,
        "SingleCandidate" => Key::SingleCandidate,
        "HangulMode" => Key::HangulMode,
        "HanjaMode" => Key::HanjaMode,
        "JunjaMode" => Key::JunjaMode,
        "Eisu" => Key::Eisu,
        "Hankaku" => Key::Hankaku,
        "Hiragana" => Key::Hiragana,
        "HiraganaKatakana" => Key::HiraganaKatakana,
        "KanaMode" => Key::KanaMode,
        "KanjiMode" => Key::KanjiMode,
        "Katakana" => Key::Katakana,
        "Romaji" => Key::Romaji,
        "Zenkaku" => Key::Zenkaku,
        "ZenkakuHankaku" => Key::ZenkakuHankaku,
        "Soft1" => Key::Soft1,
        "Soft2" => Key::Soft2,
        "Soft3" => Key::Soft3,
        "Soft4" => Key::Soft4,
        "ChannelDown" => Key::ChannelDown,
        "ChannelUp" => Key::ChannelUp,
        "Close" => Key::Close,
        "MailForward" => Key::MailForward,
        "MailReply" => Key::MailReply,
        "MailSend" => Key::MailSend,
        "MediaClose" => Key::MediaClose,
        "MediaFastForward" => Key::MediaFastForward,
        "MediaPause" => Key::MediaPause,
        "MediaPlay" => Key::MediaPlay,
        "MediaPlayPause" => Key::MediaPlayPause,
        "MediaRecord" => Key::MediaRecord,
        "MediaRewind" => Key::MediaRewind,
        "MediaStop" => Key::MediaStop,
        "MediaTrackNext" => Key::MediaTrackNext,
        "MediaTrackPrevious" => Key::MediaTrackPrevious,
        "New" => Key::New,
        "Open" => Key::Open,
        "Print" => Key::Print,
        "Save" => Key::Save,
        "SpellCheck" => Key::SpellCheck,
        "Key11" => Key::Key11,
        "Key12" => Key::Key12,
        "AudioBalanceLeft" => Key::AudioBalanceLeft,
        "AudioBalanceRight" => Key::AudioBalanceRight,
        "AudioBassBoostDown" => Key::AudioBassBoostDown,
        "AudioBassBoostUp" => Key::AudioBassBoostUp,
        "AudioFaderFront" => Key::AudioFaderFront,
        "AudioFaderRear" => Key::AudioFaderRear,
        "AudioTrebleDown" => Key::AudioTrebleDown,
        "AudioTrebleUp" => Key::AudioTrebleUp,
        "AudioVolumeDown" => Key::AudioVolumeDown,
        "AudioVolumeUp" => Key::AudioVolumeUp,
        "AudioVolumeMute" => Key::AudioVolumeMute,
        "MicrophoneToggle" => Key::MicrophoneToggle,
        "MicrophoneVolumeUp" => Key::MicrophoneVolumeUp,
        "SpeechInputToggle" => Key::SpeechInputToggle,
        "LaunchApplication1" => Key::LaunchApplication1,
        "LaunchApplication2" => Key::LaunchApplication2,
        "LaunchCalendar" => Key::LaunchCalendar,
        "LaunchContacts" => Key::LaunchContacts,
        "LaunchMail" => Key::LaunchMail,
        "LaunchMediaPlayer" => Key::LaunchMediaPlayer,
        "LaunchMusicPlayer" => Key::LaunchMusicPlayer,
        "LaunchPhone" => Key::LaunchPhone,
        "LaunchScreenSaver" => Key::LaunchScreenSaver,
        "LaunchSpreadsheet" => Key::LaunchSpreadsheet,
        "LaunchWebBrowser" => Key::LaunchWebBrowser,
        "LaunchWebCam" => Key::LaunchWebCam,
        "LaunchWordProcessor" => Key::LaunchWordProcessor,
        "BrowserBack" => Key::BrowserBack,
        "BrowserFavorites" => Key::BrowserFavorites,
        "BrowserForward" => Key::BrowserForward,
        "BrowserHome" => Key::BrowserHome,
        "BrowserRefresh" => Key::BrowserRefresh,
        "BrowserSearch" => Key::BrowserSearch,
        "BrowserStop" => Key::BrowserStop,
        "AppSwitch" => Key::AppSwitch,
        "Call" => Key::Call,
        "Camera" => Key::Camera,
        "CameraFocus" => Key::CameraFocus,
        "EndCall" => Key::EndCall,
        "GoBack" => Key::GoBack,
        "GoHome" => Key::GoHome,
        "HeadsetHook" => Key::HeadsetHook,
        "LastNumberRedial" => Key::LastNumberRedial,
        "Notification" => Key::Notification,
        "MannerMode" => Key::MannerMode,
        "VoiceDial" => Key::VoiceDial,
        "TV" => Key::TV,
        "TV3DMode" => Key::TV3DMode,
        "TVAntennaCable" => Key::TVAntennaCable,
        "TVAudioDescription" => Key::TVAudioDescription,
        "TVContentsMenu" => Key::TVContentsMenu,
        "TVDataService" => Key::TVDataService,
        "TVInput" => Key::TVInput,
        "TVInputComponent1" => Key::TVInputComponent1,
        "TVInputComponent2" => Key::TVInputComponent2,
        "TVInputComposite1" => Key::TVInputComposite1,
        "TVInputComposite2" => Key::TVInputComposite2,
        "TVInputHDMI1" => Key::TVInputHDMI1,
        "TVInputHDMI2" => Key::TVInputHDMI2,
        "TVInputHDMI3" => Key::TVInputHDMI3,
        "TVInputHDMI4" => Key::TVInputHDMI4,
        "TVInputVGA1" => Key::TVInputVGA1,
        "TVMediaContext" => Key::TVMediaContext,
        "TVNetwork" => Key::TVNetwork,
        "TVNumberEntry" => Key::TVNumberEntry,
        "TVPower" => Key::TVPower,
        "TVRadioService" => Key::TVRadioService,
        "TVSatellite" => Key::TVSatellite,
        "TVSatelliteBS" => Key::TVSatelliteBS,
        "TVSatelliteCS" => Key::TVSatelliteCS,
        "TVSatelliteToggle" => Key::TVSatelliteToggle,
        "TVTerrestrialAnalog" => Key::TVTerrestrialAnalog,
        "TVTimer" => Key::TVTimer,
        "AVRInput" => Key::AVRInput,
        "AVRPower" => Key::AVRPower,
        "ColorF0Red" => Key::ColorF0Red,
        "ColorF1Green" => Key::ColorF1Green,
        "ColorF2Yellow" => Key::ColorF2Yellow,
        "ColorF3Blue" => Key::ColorF3Blue,
        "ColorF4Grey" => Key::ColorF4Grey,
        "ColorF5Brown" => Key::ColorF5Brown,
        "ClosedCaptionToggle" => Key::ClosedCaptionToggle,
        "Dimmer" => Key::Dimmer,
        "DisplaySwap" => Key::DisplaySwap,
        "DVR" => Key::DVR,
        "Exit" => Key::Exit,
        "FavoriteClear0" => Key::FavoriteClear0,
        "FavoriteClear1" => Key::FavoriteClear1,
        "FavoriteClear2" => Key::FavoriteClear2,
        "FavoriteClear3" => Key::FavoriteClear3,
        "FavoriteRecall0" => Key::FavoriteRecall0,
        "FavoriteRecall1" => Key::FavoriteRecall1,
        "FavoriteRecall2" => Key::FavoriteRecall2,
        "FavoriteRecall3" => Key::FavoriteRecall3,
        "FavoriteStore0" => Key::FavoriteStore0,
        "FavoriteStore1" => Key::FavoriteStore1,
        "FavoriteStore2" => Key::FavoriteStore2,
        "FavoriteStore3" => Key::FavoriteStore3,
        "Guide" => Key::Guide,
        "GuideNextDay" => Key::GuideNextDay,
        "GuidePreviousDay" => Key::GuidePreviousDay,
        "Info" => Key::Info,
        "InstantReplay" => Key::InstantReplay,
        "Link" => Key::Link,
        "ListProgram" => Key::ListProgram,
        "LiveContent" => Key::LiveContent,
        "Lock" => Key::Lock,
        "MediaApps" => Key::MediaApps,
        "MediaAudioTrack" => Key::MediaAudioTrack,
        "MediaLast" => Key::MediaLast,
        "MediaSkipBackward" => Key::MediaSkipBackward,
        "MediaSkipForward" => Key::MediaSkipForward,
        "MediaStepBackward" => Key::MediaStepBackward,
        "MediaStepForward" => Key::MediaStepForward,
        "MediaTopMenu" => Key::MediaTopMenu,
        "NavigateIn" => Key::NavigateIn,
        "NavigateNext" => Key::NavigateNext,
        "NavigateOut" => Key::NavigateOut,
        "NavigatePrevious" => Key::NavigatePrevious,
        "NextFavoriteChannel" => Key::NextFavoriteChannel,
        "NextUserProfile" => Key::NextUserProfile,
        "OnDemand" => Key::OnDemand,
        "Pairing" => Key::Pairing,
        "PinPDown" => Key::PinPDown,
        "PinPMove" => Key::PinPMove,
        "PinPToggle" => Key::PinPToggle,
        "PinPUp" => Key::PinPUp,
        "PlaySpeedDown" => Key::PlaySpeedDown,
        "PlaySpeedReset" => Key::PlaySpeedReset,
        "PlaySpeedUp" => Key::PlaySpeedUp,
        "RandomToggle" => Key::RandomToggle,
        "RcLowBattery" => Key::RcLowBattery,
        "RecordSpeedNext" => Key::RecordSpeedNext,
        "RfBypass" => Key::RfBypass,
        "ScanChannelsToggle" => Key::ScanChannelsToggle,
        "ScreenModeNext" => Key::ScreenModeNext,
        "Settings" => Key::Settings,
        "SplitScreenToggle" => Key::SplitScreenToggle,
        "STBInput" => Key::STBInput,
        "STBPower" => Key::STBPower,
        "Subtitle" => Key::Subtitle,
        "Teletext" => Key::Teletext,
        "VideoModeNext" => Key::VideoModeNext,
        "Wink" => Key::Wink,
        "ZoomToggle" => Key::ZoomToggle,
        "F1" => Key::F1,
        "F2" => Key::F2,
        "F3" => Key::F3,
        "F4" => Key::F4,
        "F5" => Key::F5,
        "F6" => Key::F6,
        "F7" => Key::F7,
        "F8" => Key::F8,
        "F9" => Key::F9,
        "F10" => Key::F10,
        "F11" => Key::F11,
        "F12" => Key::F12,
        "F13" => Key::F13,
        "F14" => Key::F14,
        "F15" => Key::F15,
        "F16" => Key::F16,
        "F17" => Key::F17,
        "F18" => Key::F18,
        "F19" => Key::F19,
        "F20" => Key::F20,
        "F21" => Key::F21,
        "F22" => Key::F22,
        "F23" => Key::F23,
        "F24" => Key::F24,
        "F25" => Key::F25,
        "F26" => Key::F26,
        "F27" => Key::F27,
        "F28" => Key::F28,
        "F29" => Key::F29,
        "F30" => Key::F30,
        "F31" => Key::F31,
        "F32" => Key::F32,
        "F33" => Key::F33,
        "F34" => Key::F34,
        "F35" => Key::F35,
        "OS" => Key::Meta,
        " " => Key::Space,
        "Dead" => Key::Dead(None),
        "" | "Unidentified" => Key::Unidentified(NativeKey::Unidentified),
        s => Key::Character(s.into()),
    }
}

/// The W3C `key` value for a `Key`: the characters it types, or its name (`"Enter"`, `" "`).
pub fn key_name(key: &Key) -> String {
    match key {
        Key::Character(s) => s.to_string(),
        Key::Space => " ".into(),
        Key::Dead(_) => "Dead".into(),
        Key::Unidentified(_) => "Unidentified".into(),
        // A named key's variant is its W3C name.
        named => format!("{named:?}"),
    }
}

/// The `KeyCode` TS numbers as `n`, `None` past the list.
pub fn key_code_of(n: u32) -> Option<KeyCode> {
    KEY_CODES.get(n as usize).map(|(_, k)| *k)
}

/// Where each input enum's numbers start in TS: one number space, so a condition or method can
/// tell the device from the number. `KeyCode` starts at 0.
pub const MOUSE_BUTTON_BASE: u32 = 0x1000;
pub const GAMEPAD_BUTTON_BASE: u32 = 0x2000;
pub const GAMEPAD_AXIS_BASE: u32 = 0x2100;

/// `MouseButton`'s variants without data, in Bevy's order; TS numbers start at
/// [`MOUSE_BUTTON_BASE`].
pub const MOUSE_BUTTONS: &[(&str, MouseButton)] = &[
    ("Left", MouseButton::Left),
    ("Right", MouseButton::Right),
    ("Middle", MouseButton::Middle),
    ("Back", MouseButton::Back),
    ("Forward", MouseButton::Forward),
];

/// `GamepadButton`'s variants without data, in Bevy's order; TS numbers start at
/// [`GAMEPAD_BUTTON_BASE`].
pub const GAMEPAD_BUTTONS: &[(&str, GamepadButton)] = &[
    ("South", GamepadButton::South),
    ("East", GamepadButton::East),
    ("North", GamepadButton::North),
    ("West", GamepadButton::West),
    ("C", GamepadButton::C),
    ("Z", GamepadButton::Z),
    ("LeftTrigger", GamepadButton::LeftTrigger),
    ("LeftTrigger2", GamepadButton::LeftTrigger2),
    ("RightTrigger", GamepadButton::RightTrigger),
    ("RightTrigger2", GamepadButton::RightTrigger2),
    ("Select", GamepadButton::Select),
    ("Start", GamepadButton::Start),
    ("Mode", GamepadButton::Mode),
    ("LeftThumb", GamepadButton::LeftThumb),
    ("RightThumb", GamepadButton::RightThumb),
    ("DPadUp", GamepadButton::DPadUp),
    ("DPadDown", GamepadButton::DPadDown),
    ("DPadLeft", GamepadButton::DPadLeft),
    ("DPadRight", GamepadButton::DPadRight),
];

/// `GamepadAxis`'s variants without data, in Bevy's order; TS numbers start at
/// [`GAMEPAD_AXIS_BASE`].
pub const GAMEPAD_AXES: &[(&str, GamepadAxis)] = &[
    ("LeftStickX", GamepadAxis::LeftStickX),
    ("LeftStickY", GamepadAxis::LeftStickY),
    ("LeftZ", GamepadAxis::LeftZ),
    ("RightStickX", GamepadAxis::RightStickX),
    ("RightStickY", GamepadAxis::RightStickY),
    ("RightZ", GamepadAxis::RightZ),
];

/// An input enum's TS number, decoded: which device, and the variant.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InputCode {
    Key(KeyCode),
    Mouse(MouseButton),
    GamepadButton(GamepadButton),
    GamepadAxis(GamepadAxis),
}

/// The variant of any input enum TS numbers as `n`, `None` for a number no enum gives.
pub fn input_code_of(n: u32) -> Option<InputCode> {
    fn at<T: Copy>(n: u32, list: &[(&str, T)], base: u32) -> Option<T> {
        n.checked_sub(base).and_then(|i| list.get(i as usize)).map(|(_, v)| *v)
    }
    if let Some(k) = key_code_of(n) {
        return Some(InputCode::Key(k));
    }
    at(n, MOUSE_BUTTONS, MOUSE_BUTTON_BASE)
        .map(InputCode::Mouse)
        .or_else(|| at(n, GAMEPAD_BUTTONS, GAMEPAD_BUTTON_BASE).map(InputCode::GamepadButton))
        .or_else(|| at(n, GAMEPAD_AXES, GAMEPAD_AXIS_BASE).map(InputCode::GamepadAxis))
}

/// The names of an input list, in order.
pub fn names<T>(list: &[(&'static str, T)]) -> Vec<&'static str> {
    list.iter().map(|(n, _)| *n).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::reflect::enums::VariantInfo;
    use bevy::reflect::{PartialReflect, ReflectRef, TypeInfo, Typed};

    #[test]
    fn every_code_round_trips() {
        for (_, k) in KEY_CODES {
            assert_eq!(key_code(code_name(*k)), Some(*k), "{k:?} as {:?}", code_name(*k));
        }
        assert_eq!(code_name(KeyCode::SuperLeft), "MetaLeft");
        assert_eq!(key_code("OSLeft"), Some(KeyCode::SuperLeft));
    }

    #[test]
    fn every_key_round_trips() {
        let TypeInfo::Enum(info) = Key::type_info() else { panic!("Key is an enum") };
        let mut named = 0;
        for v in info.iter() {
            let VariantInfo::Unit(u) = v else { continue };
            let key = logical_key(u.name());
            // Keys winit has no name for read back as the characters of their name.
            if matches!(key, Key::Character(_)) {
                continue;
            }
            assert_eq!(logical_key(&key_name(&key)), key, "{}", u.name());
            named += 1;
        }
        assert!(named > 250, "{named} named keys");
        for k in [Key::Character("h".into()), Key::Space, Key::Enter, Key::Escape, Key::Dead(None)] {
            assert_eq!(logical_key(&key_name(&k)), k);
        }
        assert_eq!(key_name(&Key::Character("é".into())), "é");
    }

    /// Fails when Bevy adds, removes, renames or reorders a `KeyCode` variant: the TS enum and
    /// this list are one.
    #[test]
    fn the_list_is_bevys_keycode() {
        let TypeInfo::Enum(info) = KeyCode::type_info() else { panic!("KeyCode is an enum") };
        let bevy: Vec<&str> = info.iter().filter(|v| matches!(v, VariantInfo::Unit(_))).map(|v| v.name()).collect();
        let ours: Vec<&str> = KEY_CODES.iter().map(|(n, _)| *n).collect();
        assert_eq!(ours, bevy);
        for (name, k) in KEY_CODES {
            let ReflectRef::Enum(e) = k.reflect_ref() else { unreachable!() };
            assert_eq!(e.variant_name(), *name);
        }
        assert_eq!(info.iter().count(), KEY_CODES.len() + 1, "only Unidentified carries data");
        assert_eq!(key_code_of(KEY_CODES.len() as u32), None);
    }

    /// The names of `T`'s unit variants, in Bevy's order.
    fn units<T: Typed>() -> Vec<&'static str> {
        let TypeInfo::Enum(info) = T::type_info() else { panic!("an enum") };
        info.iter().filter(|v| matches!(v, VariantInfo::Unit(_))).map(|v| v.name()).collect()
    }

    /// Fails when Bevy changes a mouse or gamepad enum's variants, or when two enums' numbers
    /// could meet.
    #[test]
    fn the_device_lists_are_bevys_and_never_overlap() {
        assert_eq!(names(MOUSE_BUTTONS), units::<MouseButton>());
        assert_eq!(names(GAMEPAD_BUTTONS), units::<GamepadButton>());
        assert_eq!(names(GAMEPAD_AXES), units::<GamepadAxis>());
        for (name, b) in MOUSE_BUTTONS {
            assert_eq!(format!("{b:?}"), *name);
        }
        for (name, b) in GAMEPAD_BUTTONS {
            assert_eq!(format!("{b:?}"), *name);
        }
        for (name, a) in GAMEPAD_AXES {
            assert_eq!(format!("{a:?}"), *name);
        }
        assert!((KEY_CODES.len() as u32) <= MOUSE_BUTTON_BASE);
        assert!(MOUSE_BUTTON_BASE + MOUSE_BUTTONS.len() as u32 <= GAMEPAD_BUTTON_BASE);
        assert!(GAMEPAD_BUTTON_BASE + GAMEPAD_BUTTONS.len() as u32 <= GAMEPAD_AXIS_BASE);
        assert_eq!(input_code_of(0), Some(InputCode::Key(KEY_CODES[0].1)));
        assert_eq!(input_code_of(MOUSE_BUTTON_BASE), Some(InputCode::Mouse(MouseButton::Left)));
        assert_eq!(input_code_of(GAMEPAD_BUTTON_BASE + 7), Some(InputCode::GamepadButton(GamepadButton::LeftTrigger2)));
        assert_eq!(input_code_of(GAMEPAD_AXIS_BASE), Some(InputCode::GamepadAxis(GamepadAxis::LeftStickX)));
        assert_eq!(input_code_of(MOUSE_BUTTON_BASE + MOUSE_BUTTONS.len() as u32), None);
        assert_eq!(input_code_of(KEY_CODES.len() as u32), None);
    }
}
