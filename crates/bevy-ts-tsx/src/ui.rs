//! bevy_ui intrinsics: `<node>`, `<text>`, `<image>`, `<button>`, `<input>`. Each prop maps to
//! one component (`background` and `border` to a colour or a gradient), so a changed prop writes
//! that component and no other: a bound `<text>` writes `Text` and leaves `TextFont`,
//! `TextColor` and `Node` alone. `<worldText>` shares `<text>`'s font props and parsers, but is a
//! component of its entity (world text, `bevy_ts_material::world_text`), not a UI node.

use bevy::input_focus::AutoFocus;
use bevy::ui::InteractionDisabled;
use bevy::ui_widgets::ActivateOnPress;
use bevy::prelude::*;
use bevy::text::{EditableText, FontSize, FontSource, FontStyle, FontWeight, LetterSpacing, LineBreak, LineHeight, Strikethrough, StrikethroughColor, TextBackgroundColor, TextCursorStyle, Underline, UnderlineColor};
use bevy::camera::RenderTarget;
use bevy::image::{TextureAtlas, TextureAtlasLayout};
use bevy::render::render_resource::TextureFormat;
use bevy::sprite::{BorderRect, SliceScaleMode, TextureSlicer};
use bevy::ui::widget::{NodeImageMode, TextShadow, ViewportNode};
use bevy::ui::{
    AngularColorStop, BackgroundGradient, BorderGradient, BoxShadow, ColorStop, ConicGradient, CornerRadius, Gradient, InlineDirection, LinearGradient, OverflowAxis,
    OverflowClipMargin, RadialGradient, RadialGradientShape, ShadowStyle, UiPosition, UiTransform, Val2, VisualBox,
};
use bevy::ui_widgets::TextInput;
use bevy_ts_material::world_text::{MAX_OUTLINE, WorldText, WorldTextStyle};
use serde_json::{Map, Value};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UiKind {
    Node,
    Text,
    Image,
    Button,
    /// A run of rich text under `<text>` or `<span>`: `TextSpan`, no `Node`.
    Span,
    /// A `<node>` showing a camera's output: `ViewportNode`.
    Viewport,
    Input,
    /// `<worldText>`: text in the 3D world, a component of its entity. No `Node`.
    WorldText,
}

/// What every UI intrinsic but `<span>` and `<worldText>` takes: its `Node`'s look and placement, and `disabled`.
const NODE_PROPS: &[&str] = &["style", "background", "border", "radius", "outline", "shadow", "transform", "camera", "disabled"];

/// The text props Bevy applies per span; `<text>` adds `justify`, `linebreak` and `textShadow`.
const SPAN_PROPS: &[&str] = &["text", "size", "color", "font", "weight", "italic", "lineHeight", "letterSpacing", "textBackground", "underline", "strikethrough"];

/// `<worldText>`'s props: `<text>`'s font props, and world text's own (`WorldTextStyle`).
const WORLD_TEXT_PROPS: &[&str] = &["text", "font", "weight", "italic", "justify", "linebreak", "lineHeight", "color", "size", "letterSpacing", "width", "anchor", "outline", "lit"];

/// The `<worldText>` props `WorldTextStyle` holds.
const WORLD_STYLE_PROPS: &[&str] = &["size", "letterSpacing", "width", "anchor", "outline", "lit"];

impl UiKind {
    pub fn from_tag(tag: &str) -> Option<UiKind> {
        Some(match tag {
            "node" => UiKind::Node,
            "text" => UiKind::Text,
            "image" => UiKind::Image,
            "button" => UiKind::Button,
            "span" => UiKind::Span,
            "viewport" => UiKind::Viewport,
            "input" => UiKind::Input,
            "worldText" => UiKind::WorldText,
            _ => return None,
        })
    }

    /// The props every UI intrinsic but `<span>` and `<worldText>` takes, then this kind's own.
    fn props(self) -> Vec<&'static str> {
        let own: &[&str] = match self {
            UiKind::Span => return SPAN_PROPS.to_vec(),
            // A label in the world, not a node: no node props (its `outline` is its own).
            UiKind::WorldText => return WORLD_TEXT_PROPS.to_vec(),
            UiKind::Node | UiKind::Viewport => &[],
            UiKind::Button => &["activateOnPress"],
            UiKind::Text => &[
                "text", "size", "color", "font", "weight", "italic", "lineHeight", "letterSpacing", "textBackground", "underline", "strikethrough", "justify", "linebreak",
                "textShadow",
            ],
            UiKind::Image => &["src", "color", "flipX", "flipY", "rect", "atlas", "mode"],
            UiKind::Input => &["size", "color", "initialValue", "maxCharacters", "autoFocus"],
        };
        NODE_PROPS.iter().chain(own).copied().collect()
    }
}

/// The component a prop writes. Props sharing one (`style`, `radius`) write it once.
/// `background` and `border` each own two components: the colour and the gradient.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Part {
    Node,
    Background,
    Border,
    Outline,
    Shadow,
    Transform,
    Text,
    Font,
    Color,
    Layout,
    LineHeight,
    LetterSpacing,
    TextShadow,
    TextBackground,
    Underline,
    Strikethrough,
    Image,
    /// `camera` on a root UI node: `UiTargetCamera`.
    Camera,
    /// `camera` on `<viewport>`: `ViewportNode`, the camera it shows.
    Viewport,
    /// `initialValue`: the field's text, written on insert only.
    Value,
    /// `maxCharacters`: `EditableText::max_characters`.
    Limit,
    /// `autoFocus`: `AutoFocus`.
    Focus,
    /// `disabled`: `InteractionDisabled`, which Bevy's widgets read to ignore input.
    Disabled,
    /// `activateOnPress` on `<button>`: `ActivateOnPress`, activate on press, not release.
    ActivateOnPress,
    /// `<worldText>`'s size, spacing, wrap width, anchor, outline and lighting: `WorldTextStyle`.
    WorldStyle,
}

fn part(kind: UiKind, prop: &str) -> Part {
    if kind == UiKind::WorldText && WORLD_STYLE_PROPS.contains(&prop) {
        return Part::WorldStyle;
    }
    match prop {
        "style" | "radius" => Part::Node,
        "background" => Part::Background,
        "border" => Part::Border,
        "outline" => Part::Outline,
        "shadow" => Part::Shadow,
        "transform" => Part::Transform,
        "text" => Part::Text,
        "size" | "font" | "weight" | "italic" => Part::Font,
        "justify" | "linebreak" => Part::Layout,
        "lineHeight" => Part::LineHeight,
        "letterSpacing" => Part::LetterSpacing,
        "textShadow" => Part::TextShadow,
        "textBackground" => Part::TextBackground,
        "underline" => Part::Underline,
        "strikethrough" => Part::Strikethrough,
        "src" | "flipX" | "flipY" | "rect" | "atlas" | "mode" => Part::Image,
        "initialValue" => Part::Value,
        "maxCharacters" => Part::Limit,
        "autoFocus" => Part::Focus,
        "disabled" => Part::Disabled,
        "activateOnPress" => Part::ActivateOnPress,
        "color" if kind == UiKind::Image => Part::Image,
        "camera" if kind == UiKind::Viewport => Part::Viewport,
        "camera" => Part::Camera,
        _ => Part::Color,
    }
}

/// A length: a number is logical pixels; a string is `8px`, `50%`, `10vw`, `10vh`, `5vmin`,
/// `5vmax`, `2em`, `1rem` or `auto` (`px(8)` and friends write these).
pub fn val(v: &Value) -> Result<Val, String> {
    if let Some(n) = v.as_f64() {
        return Ok(Val::Px(n as f32));
    }
    let s = v.as_str().ok_or_else(|| format!("{v} is not a length"))?.trim();
    if s == "auto" {
        return Ok(Val::Auto);
    }
    let split = s.find(|c: char| !(c.is_ascii_digit() || c == '.' || c == '-' || c == '+')).unwrap_or(s.len());
    let (num, unit) = s.split_at(split);
    let n: f32 = num.parse().map_err(|_| format!("{s:?} is not a length (try px(8), \"50%\", \"auto\")"))?;
    Ok(match unit {
        "px" | "" => Val::Px(n),
        "%" => Val::Percent(n),
        "vw" => Val::Vw(n),
        "vh" => Val::Vh(n),
        "vmin" => Val::VMin(n),
        "vmax" => Val::VMax(n),
        "em" => Val::Em(n),
        "rem" => Val::Rem(n),
        other => return Err(format!("{s:?}: unknown unit {other:?}")),
    })
}

/// Sides: one length for all, CSS shorthand arrays (`[v]`, `[y, x]`, `[top, x, bottom]`,
/// `[top, right, bottom, left]`), or `{ top, right, bottom, left }`.
fn rect(v: &Value) -> Result<UiRect, String> {
    if let Some(a) = v.as_array() {
        let vs = a.iter().map(val).collect::<Result<Vec<_>, _>>()?;
        return Ok(match vs.as_slice() {
            [a] => UiRect::all(*a),
            [y, x] => UiRect::axes(*x, *y),
            [t, x, b] => UiRect::new(*x, *x, *t, *b),
            [t, r, b, l] => UiRect::new(*l, *r, *t, *b),
            _ => return Err(format!("{v} has {} sides; give 1 to 4", vs.len())),
        });
    }
    if let Some(o) = v.as_object() {
        let mut r = UiRect::DEFAULT;
        for (k, x) in o {
            let x = val(x)?;
            match k.as_str() {
                "top" => r.top = x,
                "right" => r.right = x,
                "bottom" => r.bottom = x,
                "left" => r.left = x,
                other => return Err(format!("{other} is not a side (top, right, bottom, left)")),
            }
        }
        return Ok(r);
    }
    Ok(UiRect::all(val(v)?))
}

fn word<'a>(v: &'a Value, key: &str) -> Result<&'a str, String> {
    v.as_str().ok_or_else(|| format!("style.{key} takes a string"))
}

fn number(v: &Value, key: &str) -> Result<f32, String> {
    v.as_f64().map(|n| n as f32).ok_or_else(|| format!("style.{key} takes a number"))
}

fn bad(key: &str, v: &str, options: &str) -> String {
    format!("style.{key}: {v:?} is not one of {options}")
}

fn overflow_axis(v: &Value, key: &str) -> Result<OverflowAxis, String> {
    Ok(match word(v, key)? {
        "visible" => OverflowAxis::Visible,
        "clip" => OverflowAxis::Clip,
        "hidden" => OverflowAxis::Hidden,
        "scroll" => OverflowAxis::Scroll,
        w => return Err(bad(key, w, "visible, clip, hidden, scroll")),
    })
}

/// A number and its unit: `8` is `(8, "px")`, `"1fr"` is `(1, "fr")`.
fn amount(v: &Value) -> Option<(f32, &str)> {
    if let Some(n) = v.as_f64() {
        return Some((n as f32, "px"));
    }
    let s = v.as_str()?.trim();
    let split = s.find(|c: char| !(c.is_ascii_digit() || c == '.' || c == '-' || c == '+'))?;
    let (num, unit) = s.split_at(split);
    Some((num.parse().ok()?, unit))
}

const TRACKS: &str = "a length, fr(n), \"auto\", \"min-content\", \"max-content\", minmax(min, max) or { fitContent }";

fn min_sizing(v: &Value, key: &str) -> Result<MinTrackSizingFunction, String> {
    use MinTrackSizingFunction as M;
    match v.as_str() {
        Some("auto") => return Ok(M::Auto),
        Some("min-content") => return Ok(M::MinContent),
        Some("max-content") => return Ok(M::MaxContent),
        _ => {}
    }
    Ok(match amount(v) {
        Some((n, "px" | "")) => M::Px(n),
        Some((n, "%")) => M::Percent(n),
        Some((n, "em")) => M::Em(n),
        Some((n, "rem")) => M::Rem(n),
        Some((n, "vw")) => M::Vw(n),
        Some((n, "vh")) => M::Vh(n),
        Some((n, "vmin")) => M::VMin(n),
        Some((n, "vmax")) => M::VMax(n),
        _ => return Err(format!("{key}: {v} is not a minimum track size (a length, \"auto\", \"min-content\", \"max-content\")")),
    })
}

fn max_sizing(v: &Value, key: &str) -> Result<MaxTrackSizingFunction, String> {
    use MaxTrackSizingFunction as M;
    if let Some(Some((n, unit))) = v.as_object().and_then(|o| o.get("fitContent")).map(amount) {
        return match unit {
            "px" | "" => Ok(M::FitContentPx(n)),
            "%" => Ok(M::FitContentPercent(n)),
            _ => Err(format!("{key}: fitContent takes px or %")),
        };
    }
    match v.as_str() {
        Some("auto") => return Ok(M::Auto),
        Some("min-content") => return Ok(M::MinContent),
        Some("max-content") => return Ok(M::MaxContent),
        _ => {}
    }
    Ok(match amount(v) {
        Some((n, "fr")) => M::Fraction(n),
        Some((n, "px" | "")) => M::Px(n),
        Some((n, "%")) => M::Percent(n),
        Some((n, "em")) => M::Em(n),
        Some((n, "rem")) => M::Rem(n),
        Some((n, "vw")) => M::Vw(n),
        Some((n, "vh")) => M::Vh(n),
        Some((n, "vmin")) => M::VMin(n),
        Some((n, "vmax")) => M::VMax(n),
        _ => return Err(format!("{key}: {v} is not a track ({TRACKS})")),
    })
}

/// One grid track, Bevy's `GridTrack`: `fr(1)` is `"1fr"`, `minmax(a, b)` is `{ minmax: [a, b] }`.
fn track(v: &Value, key: &str) -> Result<GridTrack, String> {
    if let Some(o) = v.as_object() {
        if let Some(mm) = o.get("minmax") {
            let [min, max] = mm.as_array().map(Vec::as_slice).unwrap_or_default() else {
                return Err(format!("{key}: minmax takes [min, max]"));
            };
            return Ok(GridTrack::minmax(min_sizing(min, key)?, max_sizing(max, key)?));
        }
        if o.contains_key("fitContent") {
            return Ok(GridTrack::minmax(MinTrackSizingFunction::Auto, max_sizing(v, key)?));
        }
        return Err(format!("{key}: {v} is not a track ({TRACKS})"));
    }
    let max = max_sizing(v, key)?;
    // A plain size is that size at both ends; `fr` grows from `auto`, as `GridTrack::fr` does.
    let min = match max {
        MaxTrackSizingFunction::Fraction(_) => MinTrackSizingFunction::Auto,
        _ => min_sizing(v, key)?,
    };
    Ok(GridTrack::minmax(min, max))
}

fn list(v: &Value) -> &[Value] {
    v.as_array().map(Vec::as_slice).unwrap_or(std::slice::from_ref(v))
}

fn tracks(v: &Value, key: &str) -> Result<Vec<GridTrack>, String> {
    list(v).iter().map(|t| track(t, key)).collect()
}

/// Template tracks: tracks and `repeat(n | "auto-fill" | "auto-fit", tracks)`.
fn repeated_tracks(v: &Value, key: &str) -> Result<Vec<RepeatedGridTrack>, String> {
    list(v)
        .iter()
        .map(|t| {
            let Some(r) = t.as_object().and_then(|o| o.get("repeat")) else {
                return track(t, key).map(RepeatedGridTrack::from);
            };
            let count = match r {
                Value::String(s) if s == "auto-fill" => GridTrackRepetition::AutoFill,
                Value::String(s) if s == "auto-fit" => GridTrackRepetition::AutoFit,
                n => GridTrackRepetition::Count(n.as_u64().filter(|n| *n > 0 && *n <= u16::MAX as u64).ok_or_else(|| format!("{key}: repeat takes a count, \"auto-fill\" or \"auto-fit\""))? as u16),
            };
            Ok(RepeatedGridTrack::repeat_many(count, tracks(&t["tracks"], key)?))
        })
        .collect()
}

/// A grid line placement: a start line, `span(n)`, `{ start, end, span }` or `"auto"`.
fn placement(v: &Value, key: &str) -> Result<GridPlacement, String> {
    if v.as_str() == Some("auto") {
        return Ok(GridPlacement::auto());
    }
    let line = |x: &Value, what: &str| x.as_i64().filter(|n| *n != 0 && n.unsigned_abs() <= i16::MAX as u64).map(|n| n as i16).ok_or_else(|| format!("{key}.{what} takes a non-zero line number"));
    if v.is_number() {
        return Ok(GridPlacement::start(line(v, "start")?));
    }
    let o = v.as_object().ok_or_else(|| format!("{key} takes a line, span(n), {{ start, end, span }} or \"auto\""))?;
    let (mut start, mut end, mut count) = (None, None, None);
    for (k, x) in o {
        match k.as_str() {
            "start" => start = Some(line(x, k)?),
            "end" => end = Some(line(x, k)?),
            "span" => count = Some(x.as_u64().filter(|n| *n > 0 && *n <= u16::MAX as u64).ok_or_else(|| format!("{key}.span takes a positive count"))? as u16),
            other => return Err(format!("{key} has no {other} (start, end, span)")),
        }
    }
    Ok(match (start, end, count) {
        (Some(s), Some(e), None) => GridPlacement::start_end(s, e),
        (Some(s), None, Some(n)) => GridPlacement::start_span(s, n),
        (None, Some(e), Some(n)) => GridPlacement::end_span(e, n),
        (Some(s), None, None) => GridPlacement::start(s),
        (None, Some(e), None) => GridPlacement::end(e),
        (None, None, Some(n)) => GridPlacement::span(n),
        (None, None, None) => GridPlacement::auto(),
        (Some(_), Some(_), Some(_)) => return Err(format!("{key}: give two of start, end and span")),
    })
}

fn style(node: &mut Node, style: &Value) -> Result<(), String> {
    let o = style.as_object().ok_or("style takes an object")?;
    for (k, v) in o {
        let key = k.as_str();
        match key {
            "width" => node.width = val(v)?,
            "height" => node.height = val(v)?,
            "minWidth" => node.min_width = val(v)?,
            "minHeight" => node.min_height = val(v)?,
            "maxWidth" => node.max_width = val(v)?,
            "maxHeight" => node.max_height = val(v)?,
            "left" => node.left = val(v)?,
            "right" => node.right = val(v)?,
            "top" => node.top = val(v)?,
            "bottom" => node.bottom = val(v)?,
            "padding" => node.padding = rect(v)?,
            "margin" => node.margin = rect(v)?,
            "border" => node.border = rect(v)?,
            "gap" => {
                node.row_gap = val(v)?;
                node.column_gap = node.row_gap;
            }
            "rowGap" => node.row_gap = val(v)?,
            "columnGap" => node.column_gap = val(v)?,
            "flexBasis" => node.flex_basis = val(v)?,
            "flexGrow" => node.flex_grow = number(v, key)?,
            "flexShrink" => node.flex_shrink = number(v, key)?,
            "aspectRatio" => node.aspect_ratio = Some(number(v, key)?),
            "position" => {
                node.position_type = match word(v, key)? {
                    "absolute" => PositionType::Absolute,
                    "relative" => PositionType::Relative,
                    w => return Err(bad(key, w, "absolute, relative")),
                }
            }
            "display" => {
                node.display = match word(v, key)? {
                    "flex" => Display::Flex,
                    "grid" => Display::Grid,
                    "block" => Display::Block,
                    "none" => Display::None,
                    w => return Err(bad(key, w, "flex, grid, block, none")),
                }
            }
            "flexDirection" => {
                node.flex_direction = match word(v, key)? {
                    "row" => FlexDirection::Row,
                    "column" => FlexDirection::Column,
                    "row-reverse" => FlexDirection::RowReverse,
                    "column-reverse" => FlexDirection::ColumnReverse,
                    w => return Err(bad(key, w, "row, column, row-reverse, column-reverse")),
                }
            }
            "flexWrap" => {
                node.flex_wrap = match word(v, key)? {
                    "nowrap" => FlexWrap::NoWrap,
                    "wrap" => FlexWrap::Wrap,
                    "wrap-reverse" => FlexWrap::WrapReverse,
                    w => return Err(bad(key, w, "nowrap, wrap, wrap-reverse")),
                }
            }
            "justifyContent" => {
                node.justify_content = match word(v, key)? {
                    "start" => JustifyContent::Start,
                    "end" => JustifyContent::End,
                    "flex-start" => JustifyContent::FlexStart,
                    "flex-end" => JustifyContent::FlexEnd,
                    "center" => JustifyContent::Center,
                    "stretch" => JustifyContent::Stretch,
                    "space-between" => JustifyContent::SpaceBetween,
                    "space-around" => JustifyContent::SpaceAround,
                    "space-evenly" => JustifyContent::SpaceEvenly,
                    w => return Err(bad(key, w, "start, end, flex-start, flex-end, center, stretch, space-between, space-around, space-evenly")),
                }
            }
            "alignItems" | "alignSelf" => {
                let a = match word(v, key)? {
                    "start" => AlignItems::Start,
                    "end" => AlignItems::End,
                    "flex-start" => AlignItems::FlexStart,
                    "flex-end" => AlignItems::FlexEnd,
                    "center" => AlignItems::Center,
                    "baseline" => AlignItems::Baseline,
                    "stretch" => AlignItems::Stretch,
                    w => return Err(bad(key, w, "start, end, flex-start, flex-end, center, baseline, stretch")),
                };
                if key == "alignItems" {
                    node.align_items = a;
                } else {
                    node.align_self = match a {
                        AlignItems::Start => AlignSelf::Start,
                        AlignItems::End => AlignSelf::End,
                        AlignItems::FlexStart => AlignSelf::FlexStart,
                        AlignItems::FlexEnd => AlignSelf::FlexEnd,
                        AlignItems::Center => AlignSelf::Center,
                        AlignItems::Baseline => AlignSelf::Baseline,
                        _ => AlignSelf::Stretch,
                    };
                }
            }
            "overflow" => {
                node.overflow = match v.as_object() {
                    Some(o) => {
                        let mut ov = Overflow::visible();
                        for (axis, w) in o {
                            match axis.as_str() {
                                "x" => ov.x = overflow_axis(w, "overflow.x")?,
                                "y" => ov.y = overflow_axis(w, "overflow.y")?,
                                other => return Err(format!("style.overflow has no {other} (x, y)")),
                            }
                        }
                        ov
                    }
                    None => {
                        let a = overflow_axis(v, key)?;
                        Overflow { x: a, y: a }
                    }
                }
            }
            "boxSizing" => {
                node.box_sizing = match word(v, key)? {
                    "border-box" => BoxSizing::BorderBox,
                    "content-box" => BoxSizing::ContentBox,
                    w => return Err(bad(key, w, "border-box, content-box")),
                }
            }
            "justifyItems" => {
                node.justify_items = match word(v, key)? {
                    "default" => JustifyItems::Default,
                    "start" => JustifyItems::Start,
                    "start-safe" => JustifyItems::StartSafe,
                    "end" => JustifyItems::End,
                    "end-safe" => JustifyItems::EndSafe,
                    "center" => JustifyItems::Center,
                    "center-safe" => JustifyItems::CenterSafe,
                    "baseline" => JustifyItems::Baseline,
                    "stretch" => JustifyItems::Stretch,
                    w => return Err(bad(key, w, "default, start, start-safe, end, end-safe, center, center-safe, baseline, stretch")),
                }
            }
            "justifySelf" => {
                node.justify_self = match word(v, key)? {
                    "auto" => JustifySelf::Auto,
                    "start" => JustifySelf::Start,
                    "start-safe" => JustifySelf::StartSafe,
                    "end" => JustifySelf::End,
                    "end-safe" => JustifySelf::EndSafe,
                    "center" => JustifySelf::Center,
                    "center-safe" => JustifySelf::CenterSafe,
                    "baseline" => JustifySelf::Baseline,
                    "stretch" => JustifySelf::Stretch,
                    w => return Err(bad(key, w, "auto, start, start-safe, end, end-safe, center, center-safe, baseline, stretch")),
                }
            }
            "alignContent" => {
                node.align_content = match word(v, key)? {
                    "default" => AlignContent::Default,
                    "start" => AlignContent::Start,
                    "start-safe" => AlignContent::StartSafe,
                    "end" => AlignContent::End,
                    "end-safe" => AlignContent::EndSafe,
                    "flex-start" => AlignContent::FlexStart,
                    "flex-start-safe" => AlignContent::FlexStartSafe,
                    "flex-end" => AlignContent::FlexEnd,
                    "flex-end-safe" => AlignContent::FlexEndSafe,
                    "center" => AlignContent::Center,
                    "center-safe" => AlignContent::CenterSafe,
                    "stretch" => AlignContent::Stretch,
                    "space-between" => AlignContent::SpaceBetween,
                    "space-evenly" => AlignContent::SpaceEvenly,
                    "space-around" => AlignContent::SpaceAround,
                    w => return Err(bad(key, w, "default, start, end, flex-start, flex-end, center (each also -safe), stretch, space-between, space-evenly, space-around")),
                }
            }
            "direction" => {
                node.direction = match word(v, key)? {
                    "ltr" => InlineDirection::Ltr,
                    "rtl" => InlineDirection::Rtl,
                    w => return Err(bad(key, w, "ltr, rtl")),
                }
            }
            "scrollbarWidth" => node.scrollbar_width = number(v, key)?,
            "overflowClipMargin" => {
                let o = v.as_object().ok_or("style.overflowClipMargin takes { visualBox, margin }")?;
                let mut m = OverflowClipMargin::default();
                for (k, x) in o {
                    match k.as_str() {
                        "visualBox" => {
                            m.visual_box = match word(x, "overflowClipMargin.visualBox")? {
                                "content-box" => VisualBox::ContentBox,
                                "padding-box" => VisualBox::PaddingBox,
                                "border-box" => VisualBox::BorderBox,
                                w => return Err(bad("overflowClipMargin.visualBox", w, "content-box, padding-box, border-box")),
                            }
                        }
                        "margin" => m.margin = number(x, "overflowClipMargin.margin")?,
                        other => return Err(format!("style.overflowClipMargin has no {other} (visualBox, margin)")),
                    }
                }
                node.overflow_clip_margin = m;
            }
            "gridAutoFlow" => {
                node.grid_auto_flow = match word(v, key)? {
                    "row" => GridAutoFlow::Row,
                    "column" => GridAutoFlow::Column,
                    "row-dense" => GridAutoFlow::RowDense,
                    "column-dense" => GridAutoFlow::ColumnDense,
                    w => return Err(bad(key, w, "row, column, row-dense, column-dense")),
                }
            }
            "gridTemplateRows" => node.grid_template_rows = repeated_tracks(v, key)?,
            "gridTemplateColumns" => node.grid_template_columns = repeated_tracks(v, key)?,
            "gridAutoRows" => node.grid_auto_rows = tracks(v, key)?,
            "gridAutoColumns" => node.grid_auto_columns = tracks(v, key)?,
            "gridRow" => node.grid_row = placement(v, key)?,
            "gridColumn" => node.grid_column = placement(v, key)?,
            other => return Err(format!("style has no {other}")),
        }
    }
    Ok(())
}

pub fn color(v: &Value) -> Result<Color, String> {
    bevy_ts::color::parse(v)
}

/// One length for every corner, `[topLeft, topRight, bottomRight, bottomLeft]` or
/// `{ topLeft, topRight, bottomRight, bottomLeft }`.
fn radius(v: &Value) -> Result<BorderRadius, String> {
    if let Some(a) = v.as_array() {
        let [tl, tr, br, bl] = a.as_slice() else {
            return Err(format!("radius has {} corners; give 4 [topLeft, topRight, bottomRight, bottomLeft] or one length", a.len()));
        };
        return Ok(BorderRadius::new(val(tl)?, val(tr)?, val(br)?, val(bl)?));
    }
    if let Some(o) = v.as_object() {
        let mut r = BorderRadius::ZERO;
        for (k, x) in o {
            let c = CornerRadius::from(val(x)?);
            match k.as_str() {
                "topLeft" => r.top_left = c,
                "topRight" => r.top_right = c,
                "bottomRight" => r.bottom_right = c,
                "bottomLeft" => r.bottom_left = c,
                other => return Err(format!("radius has no corner {other} (topLeft, topRight, bottomRight, bottomLeft)")),
            }
        }
        return Ok(r);
    }
    Ok(BorderRadius::all(val(v)?))
}

fn build_node(props: &Map<String, Value>) -> Result<Node, String> {
    let mut n = Node::default();
    if let Some(s) = props.get("style") {
        style(&mut n, s)?;
    }
    if let Some(r) = props.get("radius") {
        n.border_radius = radius(r)?;
    }
    Ok(n)
}

/// A gradient helper's value (`linearGradient`...) or an array of them; `None` for a colour.
fn is_gradient(v: &Value) -> bool {
    let tagged = |x: &Value| x.as_object().is_some_and(|o| o.contains_key("gradient"));
    tagged(v) || v.as_array().is_some_and(|a| !a.is_empty() && a.iter().all(tagged))
}

fn degrees(v: &Value, what: &str) -> Result<f32, String> {
    v.as_f64().map(|d| (d as f32).to_radians()).ok_or_else(|| format!("{what} takes degrees"))
}

/// `[x, y]` from the top left of the node, as CSS's `at x y`; centred when absent.
fn position(o: &Map<String, Value>) -> Result<UiPosition, String> {
    let Some(at) = o.get("at") else { return Ok(UiPosition::CENTER) };
    let [x, y] = at.as_array().map(Vec::as_slice).unwrap_or_default() else {
        return Err("a gradient's at takes [x, y]".into());
    };
    Ok(UiPosition::new(Vec2::new(-0.5, -0.5), val(x)?, val(y)?))
}

fn color_stops(o: &Map<String, Value>) -> Result<Vec<ColorStop>, String> {
    let stops = o.get("stops").and_then(Value::as_array).ok_or("a gradient takes stops")?;
    stops
        .iter()
        .map(|s| match s.as_array().map(Vec::as_slice) {
            Some([c, at]) => Ok(ColorStop::new(color(c)?, val(at)?)),
            _ => Ok(ColorStop::auto(color(s)?)),
        })
        .collect()
}

fn gradient(v: &Value) -> Result<Gradient, String> {
    let o = v.as_object().ok_or("not a gradient")?;
    match o.get("gradient").and_then(Value::as_str) {
        Some("linear") => {
            let angle = o.get("angle").map(|a| degrees(a, "linearGradient's angle")).transpose()?.unwrap_or(LinearGradient::TO_BOTTOM);
            Ok(LinearGradient::new(angle, color_stops(o)?).into())
        }
        Some("radial") => {
            let shape = match o.get("shape") {
                None => RadialGradientShape::default(),
                Some(Value::String(w)) => match w.as_str() {
                    "closest-side" => RadialGradientShape::ClosestSide,
                    "farthest-side" => RadialGradientShape::FarthestSide,
                    "closest-corner" => RadialGradientShape::ClosestCorner,
                    "farthest-corner" => RadialGradientShape::FarthestCorner,
                    w => return Err(format!("radialGradient shape {w:?} is not one of closest-side, farthest-side, closest-corner, farthest-corner, {{ circle }}, {{ ellipse }}")),
                },
                Some(Value::Object(s)) if s.contains_key("circle") => RadialGradientShape::Circle(val(&s["circle"])?),
                Some(Value::Object(s)) if s.contains_key("ellipse") => {
                    let [x, y] = s["ellipse"].as_array().map(Vec::as_slice).unwrap_or_default() else {
                        return Err("radialGradient shape { ellipse } takes [x, y]".into());
                    };
                    RadialGradientShape::Ellipse(val(x)?, val(y)?)
                }
                Some(other) => return Err(format!("radialGradient shape {other} is not a shape")),
            };
            Ok(RadialGradient::new(position(o)?, shape, color_stops(o)?).into())
        }
        Some("conic") => {
            let stops = o.get("stops").and_then(Value::as_array).ok_or("a gradient takes stops")?;
            let stops = stops
                .iter()
                .map(|s| match s.as_array().map(Vec::as_slice) {
                    Some([c, a]) => Ok(AngularColorStop::new(color(c)?, degrees(a, "a conic stop's angle")?)),
                    _ => Ok(AngularColorStop::auto(color(s)?)),
                })
                .collect::<Result<Vec<_>, String>>()?;
            let start = o.get("from").map(|a| degrees(a, "conicGradient's from")).transpose()?.unwrap_or(0.0);
            Ok(ConicGradient::new(position(o)?, stops).with_start(start).into())
        }
        _ => Err(format!("{v} is not a gradient (linearGradient, radialGradient, conicGradient)")),
    }
}

fn gradients(v: &Value) -> Result<Vec<Gradient>, String> {
    list(v).iter().map(gradient).collect()
}

/// `border`'s colour: one for every side, or `{ top, right, bottom, left }`. A one-key object
/// naming a `Color` variant (`{ Srgba: ... }`) is one colour.
fn border_color(v: &Value) -> Result<BorderColor, String> {
    let variant = |o: &Map<String, Value>| o.len() == 1 && o.keys().all(|k| k.starts_with(char::is_uppercase));
    let Some(o) = v.as_object().filter(|o| !variant(o)) else { return Ok(BorderColor::all(color(v)?)) };
    let mut b = BorderColor::DEFAULT;
    for (k, x) in o {
        let c = color(x)?;
        match k.as_str() {
            "top" => b.top = c,
            "right" => b.right = c,
            "bottom" => b.bottom = c,
            "left" => b.left = c,
            other => return Err(format!("border has no side {other} (top, right, bottom, left)")),
        }
    }
    Ok(b)
}

fn fields<'a>(v: &'a Value, prop: &str, keys: &[&str]) -> Result<&'a Map<String, Value>, String> {
    let o = v.as_object().ok_or_else(|| format!("{prop} takes {{ {} }}", keys.join(", ")))?;
    if let Some(k) = o.keys().find(|k| !keys.contains(&k.as_str())) {
        return Err(format!("{prop} has no {k} ({})", keys.join(", ")));
    }
    Ok(o)
}

/// A missing field is Bevy's `Outline` default (1 px wide, no offset, white).
fn outline(v: &Value) -> Result<Outline, String> {
    let o = fields(v, "outline", &["width", "offset", "color"])?;
    let mut out = Outline::default();
    for (k, x) in o {
        match k.as_str() {
            "width" => out.width = val(x)?,
            "offset" => out.offset = val(x)?,
            _ => out.color = color(x)?,
        }
    }
    Ok(out)
}

/// One shadow or several; a missing field is Bevy's `ShadowStyle` default.
fn shadow(v: &Value) -> Result<BoxShadow, String> {
    let styles = list(v)
        .iter()
        .map(|x| {
            let o = fields(x, "shadow", &["color", "x", "y", "spread", "blur"])?;
            let mut s = ShadowStyle::default();
            for (k, x) in o {
                match k.as_str() {
                    "color" => s.color = color(x)?,
                    "x" => s.x_offset = val(x)?,
                    "y" => s.y_offset = val(x)?,
                    "spread" => s.spread_radius = val(x)?,
                    _ => s.blur_radius = val(x)?,
                }
            }
            Ok(s)
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(BoxShadow(styles))
}

/// `{ translation: [x, y], scale: n | [x, y], rotation: radians }` (`deg(45)` gives radians).
fn transform(v: &Value) -> Result<UiTransform, String> {
    let o = fields(v, "transform", &["translation", "scale", "rotation"])?;
    let mut t = UiTransform::IDENTITY;
    if let Some(x) = o.get("translation") {
        let [a, b] = x.as_array().map(Vec::as_slice).unwrap_or_default() else {
            return Err("transform.translation takes [x, y]".into());
        };
        t.translation = Val2::new(val(a)?, val(b)?);
    }
    if let Some(x) = o.get("scale") {
        t.scale = match (x.as_f64(), x.as_array().map(Vec::as_slice)) {
            (Some(n), _) => Vec2::splat(n as f32),
            (_, Some([a, b])) if a.is_number() && b.is_number() => Vec2::new(a.as_f64().unwrap() as f32, b.as_f64().unwrap() as f32),
            _ => return Err("transform.scale takes a number or [x, y]".into()),
        };
    }
    if let Some(x) = o.get("rotation") {
        t.rotation = Rot2::radians(x.as_f64().ok_or("transform.rotation takes radians (or deg(n))")? as f32);
    }
    Ok(t)
}

/// `TextFont`'s props: `size`, `font` (an asset path), `weight` (100 to 900) and `italic`.
struct FontFields<'a> {
    size: Option<f32>,
    font: Option<&'a str>,
    weight: Option<u16>,
    italic: bool,
}

fn font_fields(props: &Map<String, Value>) -> Result<FontFields<'_>, String> {
    let size = props.get("size").map(|v| v.as_f64().map(|n| n as f32).ok_or("size takes a number")).transpose()?;
    let font = props.get("font").map(|v| v.as_str().ok_or("font takes an asset path")).transpose()?;
    let weight = props
        .get("weight")
        .map(|v| v.as_u64().filter(|n| (1..=1000).contains(n)).map(|n| n as u16).ok_or("weight takes a number from 1 to 1000 (400 normal, 700 bold)"))
        .transpose()?;
    let italic = props.get("italic").map(|v| v.as_bool().ok_or("italic takes true or false")).transpose()?.unwrap_or(false);
    Ok(FontFields { size, font, weight, italic })
}

fn layout(props: &Map<String, Value>) -> Result<TextLayout, String> {
    let mut l = TextLayout::default();
    if let Some(v) = props.get("justify") {
        l.justify = match v.as_str() {
            Some("left") => Justify::Left,
            Some("center") => Justify::Center,
            Some("right") => Justify::Right,
            Some("justified") => Justify::Justified,
            Some("start") => Justify::Start,
            Some("end") => Justify::End,
            _ => return Err(format!("justify: {v} is not one of left, center, right, justified, start, end")),
        };
    }
    if let Some(v) = props.get("linebreak") {
        l.linebreak = match v.as_str() {
            Some("word") => LineBreak::WordBoundary,
            Some("char") => LineBreak::AnyCharacter,
            Some("word-or-char") => LineBreak::WordOrCharacter,
            Some("none") => LineBreak::NoWrap,
            _ => return Err(format!("linebreak: {v} is not one of word, char, word-or-char, none")),
        };
    }
    Ok(l)
}

/// A number is relative to the font size, as CSS's unitless `line-height`; `px(n)` is pixels.
fn line_height(v: &Value) -> Result<LineHeight, String> {
    if let Some(n) = v.as_f64() {
        return Ok(LineHeight::RelativeToFont(n as f32));
    }
    match amount(v) {
        Some((n, "px")) => Ok(LineHeight::Px(n)),
        _ => Err(format!("lineHeight: {v} is not a multiple of the font size or px(n)")),
    }
}

/// A number or `px(n)` is pixels; `rem(n)` is relative to the root font size.
fn letter_spacing(v: &Value) -> Result<LetterSpacing, String> {
    match amount(v) {
        Some((n, "px")) => Ok(LetterSpacing::Px(n)),
        Some((n, "rem")) => Ok(LetterSpacing::Rem(n)),
        _ => Err(format!("letterSpacing: {v} is not px(n) or rem(n)")),
    }
}

/// `{ x, y, color }`; a missing field is Bevy's `TextShadow` default.
fn text_shadow(v: &Value) -> Result<TextShadow, String> {
    let o = fields(v, "textShadow", &["x", "y", "color"])?;
    let mut s = TextShadow::default();
    for (k, x) in o {
        match k.as_str() {
            "x" | "y" => {
                let n = x.as_f64().ok_or_else(|| format!("textShadow.{k} takes a number of pixels"))? as f32;
                if k == "x" { s.offset.x = n } else { s.offset.y = n }
            }
            _ => s.color = color(x)?,
        }
    }
    Ok(s)
}

/// A positive, finite number of world units.
fn positive(v: &Value, prop: &str) -> Result<f32, String> {
    v.as_f64().map(|n| n as f32).filter(|n| n.is_finite() && *n > 0.0).ok_or_else(|| format!("{prop}: {v} is not a positive number of world units"))
}

/// `"center"`, a side or a corner, or `[x, y]` from -0.5 to 0.5, `[-0.5, -0.5]` the bottom left.
fn anchor(v: &Value) -> Result<Vec2, String> {
    const NAMES: &str = "center, top, bottom, left, right, top-left, top-right, bottom-left, bottom-right, or [x, y] from -0.5 to 0.5";
    if let Some(name) = v.as_str() {
        let (x, y) = match name {
            "center" => (0.0, 0.0),
            "top" => (0.0, 0.5),
            "bottom" => (0.0, -0.5),
            "left" => (-0.5, 0.0),
            "right" => (0.5, 0.0),
            "top-left" => (-0.5, 0.5),
            "top-right" => (0.5, 0.5),
            "bottom-left" => (-0.5, -0.5),
            "bottom-right" => (0.5, -0.5),
            _ => return Err(format!("anchor: {v} is not one of {NAMES}")),
        };
        return Ok(Vec2::new(x, y));
    }
    let n: Option<Vec<f32>> = v.as_array().and_then(|a| a.iter().map(|x| x.as_f64().map(|n| n as f32)).collect());
    match n.as_deref() {
        Some([x, y]) if (-0.5..=0.5).contains(x) && (-0.5..=0.5).contains(y) => Ok(Vec2::new(*x, *y)),
        _ => Err(format!("anchor: {v} is not one of {NAMES}")),
    }
}

/// `<worldText>`'s `WorldTextStyle` from its props; a missing prop is the default.
fn world_style(props: &Map<String, Value>) -> Result<WorldTextStyle, String> {
    let mut s = WorldTextStyle::default();
    if let Some(v) = props.get("size") {
        s.size = positive(v, "size")?;
    }
    if let Some(v) = props.get("letterSpacing") {
        s.letter_spacing = v.as_f64().map(|n| n as f32).filter(|n| n.is_finite()).ok_or_else(|| format!("letterSpacing: {v} is not a number (a fraction of size)"))?;
    }
    if let Some(v) = props.get("width") {
        s.width = Some(positive(v, "width")?);
    }
    if let Some(v) = props.get("anchor") {
        s.anchor = anchor(v)?;
    }
    if let Some(v) = props.get("outline") {
        let o = fields(v, "outline", &["width", "color"])?;
        let w = o.get("width").ok_or("outline takes { width, color }")?;
        let width = w.as_f64().map(|n| n as f32).filter(|n| (0.0..=MAX_OUTLINE).contains(n)).ok_or_else(|| format!("outline.width: {w} is outside 0 to {MAX_OUTLINE} (a fraction of size)"))?;
        let c = o.get("color").map(|c| color(c).map_err(|e| format!("outline.color: {e}"))).transpose()?.unwrap_or(Color::BLACK);
        s.outline = Some((width, c));
    }
    if let Some(v) = props.get("lit") {
        s.lit = flag(v, "lit")?;
    }
    Ok(s)
}

/// `<worldText>`'s `lineHeight`: a multiple of `size` only.
fn world_line_height(v: &Value) -> Result<LineHeight, String> {
    v.as_f64().map(|n| LineHeight::RelativeToFont(n as f32)).ok_or_else(|| format!("lineHeight: {v} is not a multiple of size (a number)"))
}

/// `underline` and `strikethrough`: `true` (Bevy's colour), a colour, or `false` (none).
fn decoration(v: &Value, prop: &str) -> Result<Option<Option<Color>>, String> {
    match v {
        Value::Bool(true) => Ok(Some(None)),
        Value::Bool(false) => Ok(None),
        Value::String(_) => Ok(Some(Some(color(v)?))),
        _ => Err(format!("{prop} takes true, false or a colour")),
    }
}

/// `camera`: an entity (a ref's, as bits), or `null` while the ref is not mounted.
fn camera(v: &Value) -> Result<Option<Entity>, String> {
    if v.is_null() {
        return Ok(None);
    }
    let bits = v.as_u64().ok_or("camera takes a ref (or an entity)")?;
    Entity::try_from_bits(bits).map(Some).ok_or_else(|| format!("camera: {bits} is not an entity"))
}

/// Gives a viewport's camera an image of its own to render into, as Bevy's viewport example
/// does by hand, unless it already renders to an image other than the window's. Bevy's
/// `update_viewport_render_target_size` then sizes it to the node. This takes the camera off the
/// window: a viewport's camera is a camera of its own.
fn own_image_target(world: &mut World, camera: Entity) -> Result<(), String> {
    let window = world.get_resource::<bevy_ts::WindowImage>().map(|w| w.0.id());
    let Ok(em) = world.get_entity(camera) else { return Ok(()) };
    let own = em.get::<RenderTarget>().and_then(RenderTarget::as_image).is_some_and(|h| Some(h.id()) != window);
    if own {
        return Ok(());
    }
    let image = Image::new_target_texture(1, 1, TextureFormat::Bgra8UnormSrgb, None);
    let handle = world.get_resource_mut::<Assets<Image>>().ok_or("<viewport> needs the image plugin (no Assets<Image>)")?.add(image);
    world.entity_mut(camera).insert(RenderTarget::Image(handle.into()));
    Ok(())
}

/// A grid atlas layout, Bevy's `TextureAtlasLayout::from_grid`.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct AtlasGrid {
    tile: UVec2,
    columns: u32,
    rows: u32,
    padding: UVec2,
    offset: UVec2,
}

/// `<image>`'s `ImageNode` fields beside `src`.
struct ImageFields {
    color: Option<Color>,
    flip_x: bool,
    flip_y: bool,
    rect: Option<Rect>,
    atlas: Option<(AtlasGrid, usize)>,
    mode: NodeImageMode,
}

fn uvec2(v: &Value, what: &str) -> Result<UVec2, String> {
    let [x, y] = v.as_array().map(Vec::as_slice).unwrap_or_default() else {
        return Err(format!("{what} takes [x, y]"));
    };
    let n = |c: &Value| c.as_u64().filter(|n| *n <= u32::MAX as u64).map(|n| n as u32).ok_or_else(|| format!("{what} takes whole pixels"));
    Ok(UVec2::new(n(x)?, n(y)?))
}

fn flag(v: &Value, what: &str) -> Result<bool, String> {
    v.as_bool().ok_or_else(|| format!("{what} takes true or false"))
}

fn image_fields(props: &Map<String, Value>) -> Result<ImageFields, String> {
    let mut f = ImageFields { color: None, flip_x: false, flip_y: false, rect: None, atlas: None, mode: NodeImageMode::Auto };
    if let Some(c) = props.get("color") {
        f.color = Some(color(c)?);
    }
    if let Some(v) = props.get("flipX") {
        f.flip_x = flag(v, "flipX")?;
    }
    if let Some(v) = props.get("flipY") {
        f.flip_y = flag(v, "flipY")?;
    }
    if let Some(v) = props.get("rect") {
        let n: Option<Vec<f32>> = v.as_array().and_then(|a| a.iter().map(|x| x.as_f64().map(|n| n as f32)).collect());
        let Some([x, y, w, h]) = n.as_deref() else { return Err("rect takes [x, y, width, height] in pixels".into()) };
        f.rect = Some(Rect::new(*x, *y, x + w, y + h));
    }
    if let Some(v) = props.get("atlas") {
        let o = fields(v, "atlas", &["layout", "index"])?;
        let l = fields(o.get("layout").ok_or("atlas takes a layout")?, "atlas.layout", &["tile", "columns", "rows", "padding", "offset"])?;
        let count = |k: &str| l.get(k).and_then(Value::as_u64).filter(|n| *n > 0 && *n <= u32::MAX as u64).map(|n| n as u32).ok_or_else(|| format!("atlas.layout.{k} takes a positive count"));
        let grid = AtlasGrid {
            tile: uvec2(l.get("tile").ok_or("atlas.layout takes tile: [width, height]")?, "atlas.layout.tile")?,
            columns: count("columns")?,
            rows: count("rows")?,
            padding: l.get("padding").map(|p| uvec2(p, "atlas.layout.padding")).transpose()?.unwrap_or(UVec2::ZERO),
            offset: l.get("offset").map(|p| uvec2(p, "atlas.layout.offset")).transpose()?.unwrap_or(UVec2::ZERO),
        };
        let index = o.get("index").and_then(Value::as_u64).ok_or("atlas.index takes a tile number")? as usize;
        if index >= (grid.columns * grid.rows) as usize {
            return Err(format!("atlas.index {index} is past the layout's {} tiles", grid.columns * grid.rows));
        }
        f.atlas = Some((grid, index));
    }
    if let Some(v) = props.get("mode") {
        f.mode = image_mode(v)?;
    }
    Ok(f)
}

/// `"auto"`, `"stretch"`, `sliced({ border, center, sides, maxCornerScale })` or
/// `tiled({ x, y, stretch })`, Bevy's `NodeImageMode`.
fn image_mode(v: &Value) -> Result<NodeImageMode, String> {
    match v.as_str() {
        Some("auto") => return Ok(NodeImageMode::Auto),
        Some("stretch") => return Ok(NodeImageMode::Stretch),
        _ => {}
    }
    let o = v.as_object().ok_or_else(|| format!("mode: {v} is not one of \"auto\", \"stretch\", sliced(...), tiled(...)"))?;
    match o.get("mode").and_then(Value::as_str) {
        Some("sliced") => {
            let o = fields(v, "sliced", &["mode", "border", "center", "sides", "maxCornerScale"])?;
            let mut s = TextureSlicer { border: slice_border(o.get("border").ok_or("sliced takes a border")?)?, ..default() };
            if let Some(c) = o.get("center") {
                s.center_scale_mode = scale_mode(c, "sliced center")?;
            }
            if let Some(c) = o.get("sides") {
                s.sides_scale_mode = scale_mode(c, "sliced sides")?;
            }
            if let Some(m) = o.get("maxCornerScale") {
                s.max_corner_scale = m.as_f64().ok_or("sliced maxCornerScale takes a number")? as f32;
            }
            Ok(NodeImageMode::Sliced(s))
        }
        Some("tiled") => {
            let o = fields(v, "tiled", &["mode", "x", "y", "stretch"])?;
            Ok(NodeImageMode::Tiled {
                tile_x: o.get("x").map(|x| flag(x, "tiled x")).transpose()?.unwrap_or(true),
                tile_y: o.get("y").map(|x| flag(x, "tiled y")).transpose()?.unwrap_or(true),
                stretch_value: o.get("stretch").map(|x| x.as_f64().ok_or("tiled stretch takes a number")).transpose()?.unwrap_or(1.0) as f32,
            })
        }
        _ => Err(format!("mode: {v} is not one of \"auto\", \"stretch\", sliced(...), tiled(...)")),
    }
}

/// A slice border in pixels: one for every side, or `{ left, right, top, bottom }`.
fn slice_border(v: &Value) -> Result<BorderRect, String> {
    if let Some(n) = v.as_f64() {
        return Ok(BorderRect::all(n as f32));
    }
    let o = fields(v, "sliced border", &["left", "right", "top", "bottom"])?;
    let side = |k: &str| o.get(k).map(|x| x.as_f64().ok_or_else(|| format!("sliced border.{k} takes pixels"))).transpose().map(|x| x.unwrap_or(0.0) as f32);
    Ok(BorderRect::from([side("left")?, side("right")?, side("top")?, side("bottom")?]))
}

/// `"stretch"` or `{ tile: stretchValue }`, Bevy's `SliceScaleMode`.
fn scale_mode(v: &Value, what: &str) -> Result<SliceScaleMode, String> {
    if v.as_str() == Some("stretch") {
        return Ok(SliceScaleMode::Stretch);
    }
    match v.as_object().and_then(|o| o.get("tile")).and_then(Value::as_f64) {
        Some(n) => Ok(SliceScaleMode::Tile { stretch_value: n as f32 }),
        None => Err(format!("{what} takes \"stretch\" or {{ tile: n }}")),
    }
}

/// Equal grids share one `TextureAtlasLayout`, as equal meshes share one mesh.
#[derive(Resource, Default)]
struct AtlasLayouts(std::collections::HashMap<AtlasGrid, Handle<TextureAtlasLayout>>);

fn atlas_layout(world: &mut World, grid: AtlasGrid) -> Result<Handle<TextureAtlasLayout>, String> {
    if let Some(h) = world.get_resource::<AtlasLayouts>().and_then(|l| l.0.get(&grid)) {
        return Ok(h.clone());
    }
    let layout = TextureAtlasLayout::from_grid(grid.tile, grid.columns, grid.rows, Some(grid.padding), Some(grid.offset));
    let h = world.get_resource_mut::<Assets<TextureAtlasLayout>>().ok_or("atlas needs the image plugin (no Assets<TextureAtlasLayout>)")?.add(layout);
    world.get_resource_or_insert_with(AtlasLayouts::default).0.insert(grid, h.clone());
    Ok(h)
}

/// Checks every prop of a UI intrinsic; bound props are checked when their value arrives.
pub fn check(kind: UiKind, props: &Map<String, Value>) -> Result<(), String> {
    for k in props.keys() {
        if !kind.props().contains(&k.as_str()) {
            return Err(format!("has no prop {k} (props: {})", kind.props().join(", ")));
        }
    }
    let mut parts: Vec<Part> = props.keys().map(|k| part(kind, k)).collect();
    parts.sort();
    parts.dedup();
    for p in parts {
        check_part(kind, props, p)?;
    }
    Ok(())
}

fn check_part(kind: UiKind, props: &Map<String, Value>, p: Part) -> Result<(), String> {
    match p {
        Part::Node => build_node(props).map(|_| ()),
        Part::Background if is_gradient(&props["background"]) => gradients(&props["background"]).map(|_| ()),
        Part::Background => color(&props["background"]).map(|_| ()),
        Part::Border if is_gradient(&props["border"]) => gradients(&props["border"]).map(|_| ()),
        Part::Border => border_color(&props["border"]).map(|_| ()),
        Part::Outline => outline(&props["outline"]).map(|_| ()),
        Part::Shadow => shadow(&props["shadow"]).map(|_| ()),
        Part::Transform => transform(&props["transform"]).map(|_| ()),
        Part::Text => props["text"].as_str().map(|_| ()).ok_or_else(|| "text takes a string".into()),
        Part::Font => font_fields(props).map(|_| ()),
        Part::Color => color(&props["color"]).map(|_| ()).map_err(|e| format!("color: {e}")),
        Part::Layout => layout(props).map(|_| ()),
        Part::LineHeight if kind == UiKind::WorldText => world_line_height(&props["lineHeight"]).map(|_| ()),
        Part::LineHeight => line_height(&props["lineHeight"]).map(|_| ()),
        Part::LetterSpacing => letter_spacing(&props["letterSpacing"]).map(|_| ()),
        Part::TextShadow => text_shadow(&props["textShadow"]).map(|_| ()),
        Part::TextBackground => color(&props["textBackground"]).map(|_| ()),
        Part::Underline => decoration(&props["underline"], "underline").map(|_| ()),
        Part::Strikethrough => decoration(&props["strikethrough"], "strikethrough").map(|_| ()),
        Part::Image => {
            if kind == UiKind::Image {
                props.get("src").and_then(Value::as_str).ok_or("<image> needs src=\"path\"")?;
                image_fields(props)?;
            }
            Ok(())
        }
        Part::Camera | Part::Viewport => camera(&props["camera"]).map(|_| ()),
        Part::Value => props["initialValue"].as_str().map(|_| ()).ok_or_else(|| "initialValue takes a string".into()),
        Part::Limit => limit(props).map(|_| ()),
        Part::Focus => props["autoFocus"].as_bool().map(|_| ()).ok_or_else(|| "autoFocus takes a boolean".into()),
        Part::Disabled => props["disabled"].as_bool().map(|_| ()).ok_or_else(|| "disabled takes a boolean".into()),
        Part::ActivateOnPress => props["activateOnPress"].as_bool().map(|_| ()).ok_or_else(|| "activateOnPress takes a boolean".into()),
        Part::WorldStyle => world_style(props).map(|_| ()),
    }
}

/// An `<input>`'s cursor, in its text's colour: Bevy draws none without `TextCursorStyle`.
fn cursor(world: &mut World, e: Entity, kind: UiKind, props: &Map<String, Value>) -> Result<(), String> {
    if kind == UiKind::Input {
        let c = props.get("color").map(color).transpose()?.unwrap_or(TextColor::default().0);
        world.entity_mut(e).insert(TextCursorStyle { color: c, ..default() });
    }
    Ok(())
}

fn limit(props: &Map<String, Value>) -> Result<Option<usize>, String> {
    match props.get("maxCharacters") {
        None => Ok(None),
        Some(v) => v.as_u64().map(|n| Some(n as usize)).ok_or_else(|| format!("maxCharacters takes a whole number, not {v}")),
    }
}

/// Writes (or removes, when its props are gone) the component behind one part.
fn write_part(world: &mut World, e: Entity, kind: UiKind, props: &Map<String, Value>, p: Part) -> Result<(), String> {
    let present = props_of(kind, props, p);
    if present {
        check_part(kind, props, p)?;
    }
    match p {
        Part::Node => {
            let n = build_node(props)?;
            world.entity_mut(e).insert(n);
        }
        // `background` and `border` each own a colour and a gradient: writing one removes the other.
        Part::Background if present && is_gradient(&props["background"]) => {
            world.entity_mut(e).remove::<BackgroundColor>().insert(BackgroundGradient(gradients(&props["background"])?));
        }
        Part::Background if present => {
            world.entity_mut(e).remove::<BackgroundGradient>().insert(BackgroundColor(color(&props["background"])?));
        }
        Part::Background => {
            world.entity_mut(e).remove::<(BackgroundColor, BackgroundGradient)>();
        }
        Part::Border if present && is_gradient(&props["border"]) => {
            world.entity_mut(e).remove::<BorderColor>().insert(BorderGradient(gradients(&props["border"])?));
        }
        Part::Border if present => {
            world.entity_mut(e).remove::<BorderGradient>().insert(border_color(&props["border"])?);
        }
        Part::Border => {
            world.entity_mut(e).remove::<(BorderColor, BorderGradient)>();
        }
        Part::Outline if present => {
            world.entity_mut(e).insert(outline(&props["outline"])?);
        }
        Part::Outline => {
            world.entity_mut(e).remove::<Outline>();
        }
        Part::Shadow if present => {
            world.entity_mut(e).insert(shadow(&props["shadow"])?);
        }
        Part::Shadow => {
            world.entity_mut(e).remove::<BoxShadow>();
        }
        Part::Transform if present => {
            world.entity_mut(e).insert(transform(&props["transform"])?);
        }
        Part::Transform => {
            world.entity_mut(e).remove::<UiTransform>();
        }
        Part::Text => {
            let s = props.get("text").and_then(Value::as_str).unwrap_or_default();
            if kind == UiKind::Span {
                world.entity_mut(e).insert(TextSpan::new(s));
            } else if kind == UiKind::WorldText {
                world.entity_mut(e).insert(WorldText::new(s));
            } else {
                world.entity_mut(e).insert(Text::new(s));
            }
        }
        Part::Font => {
            let f = font_fields(props)?;
            // World text's em size is `WorldTextStyle::size`; `font_size` is not read.
            let f = if kind == UiKind::WorldText { FontFields { size: None, ..f } } else { f };
            let mut font = TextFont { font_size: FontSize::Px(f.size.unwrap_or(20.0)), weight: FontWeight(f.weight.unwrap_or(400)), ..default() };
            if f.italic {
                font.style = FontStyle::Italic;
            }
            if let Some(path) = f.font {
                let server = world.get_resource::<AssetServer>().ok_or("font needs the asset plugin (no AssetServer)")?;
                font.font = FontSource::Handle(server.load(path.to_string()));
            }
            world.entity_mut(e).insert(font);
        }
        Part::Layout => {
            let l = layout(props)?;
            world.entity_mut(e).insert(l);
        }
        // `TextSpan` requires these two, so an absent prop writes Bevy's default rather than removing.
        Part::LineHeight => {
            let parse = if kind == UiKind::WorldText { world_line_height } else { line_height };
            let h = props.get("lineHeight").map(parse).transpose()?.unwrap_or_default();
            world.entity_mut(e).insert(h);
        }
        Part::LetterSpacing => {
            let l = props.get("letterSpacing").map(letter_spacing).transpose()?.unwrap_or_default();
            world.entity_mut(e).insert(l);
        }
        Part::TextShadow if present => {
            world.entity_mut(e).insert(text_shadow(&props["textShadow"])?);
        }
        Part::TextShadow => {
            world.entity_mut(e).remove::<TextShadow>();
        }
        Part::TextBackground if present => {
            world.entity_mut(e).insert(TextBackgroundColor(color(&props["textBackground"])?));
        }
        Part::TextBackground => {
            world.entity_mut(e).remove::<TextBackgroundColor>();
        }
        // `underline` owns `Underline` and its colour; `true` is Bevy's default colour.
        Part::Underline => {
            world.entity_mut(e).remove::<(Underline, UnderlineColor)>();
            match props.get("underline").map(|v| decoration(v, "underline")).transpose()?.flatten() {
                Some(Some(c)) => {
                    world.entity_mut(e).insert((Underline, UnderlineColor(c)));
                }
                Some(None) => {
                    world.entity_mut(e).insert(Underline);
                }
                None => {}
            }
        }
        Part::Strikethrough => {
            world.entity_mut(e).remove::<(Strikethrough, StrikethroughColor)>();
            match props.get("strikethrough").map(|v| decoration(v, "strikethrough")).transpose()?.flatten() {
                Some(Some(c)) => {
                    world.entity_mut(e).insert((Strikethrough, StrikethroughColor(c)));
                }
                Some(None) => {
                    world.entity_mut(e).insert(Strikethrough);
                }
                None => {}
            }
        }
        Part::Color if present => {
            world.entity_mut(e).insert(TextColor(color(&props["color"])?));
            cursor(world, e, kind, props)?;
        }
        // A label always has a colour: white unless given.
        Part::Color if kind == UiKind::WorldText => {
            world.entity_mut(e).insert(TextColor::default());
        }
        Part::Color => {
            world.entity_mut(e).remove::<TextColor>();
            cursor(world, e, kind, props)?;
        }
        Part::Image => {
            let src = props.get("src").and_then(Value::as_str).unwrap_or_default().to_string();
            let image = world.get_resource::<AssetServer>().ok_or("<image> needs the asset plugin (no AssetServer)")?.load(src);
            let f = image_fields(props)?;
            let mut node = ImageNode::new(image);
            node.color = f.color.unwrap_or(Color::WHITE);
            node.flip_x = f.flip_x;
            node.flip_y = f.flip_y;
            node.rect = f.rect;
            node.image_mode = f.mode;
            if let Some((grid, index)) = f.atlas {
                node.texture_atlas = Some(TextureAtlas { layout: atlas_layout(world, grid)?, index });
            }
            world.entity_mut(e).insert(node);
        }
        // An entity not mounted yet is `null`: no target until its ref resolves.
        Part::Camera => match props.get("camera").map(camera).transpose()?.flatten() {
            Some(c) => {
                world.entity_mut(e).insert(UiTargetCamera(c));
            }
            None => {
                world.entity_mut(e).remove::<UiTargetCamera>();
            }
        },
        Part::Viewport => {
            // `ViewportNode` brings a picking `PointerId`, whose hook needs picking's map.
            if !world.contains_resource::<bevy::picking::pointer::PointerMap>() {
                return Err("<viewport> needs the picking plugin (no PointerMap)".into());
            }
            let c = props.get("camera").map(camera).transpose()?.flatten();
            if let Some(c) = c {
                own_image_target(world, c)?;
            }
            world.entity_mut(e).insert(ViewportNode { camera: c });
        }
        // The text is Bevy's editor's from mount on: only `insert` writes it.
        Part::Value => {}
        Part::Limit => {
            let max = limit(props)?;
            if let Some(mut text) = world.get_mut::<EditableText>(e) {
                text.max_characters = max;
            }
        }
        Part::Focus if props.get("autoFocus").and_then(Value::as_bool) == Some(true) => {
            world.entity_mut(e).insert(AutoFocus);
        }
        Part::Focus => {
            world.entity_mut(e).remove::<AutoFocus>();
        }
        Part::Disabled => marker::<InteractionDisabled>(world, e, props, "disabled"),
        Part::ActivateOnPress => marker::<ActivateOnPress>(world, e, props, "activateOnPress"),
        Part::WorldStyle => {
            let s = world_style(props)?;
            world.entity_mut(e).insert(s);
        }
    }
    Ok(())
}

/// A boolean prop that is a marker component: present while the prop is `true`.
fn marker<C: Component + Default>(world: &mut World, e: Entity, props: &Map<String, Value>, prop: &str) {
    if props.get(prop).and_then(Value::as_bool) == Some(true) {
        world.entity_mut(e).insert(C::default());
    } else {
        world.entity_mut(e).remove::<C>();
    }
}

fn props_of(kind: UiKind, props: &Map<String, Value>, p: Part) -> bool {
    props.keys().any(|k| part(kind, k) == p)
}

/// Inserts everything a UI intrinsic owns, from its current props.
pub fn insert(world: &mut World, e: Entity, kind: UiKind, props: &Map<String, Value>) -> Result<(), String> {
    if !matches!(kind, UiKind::Span | UiKind::WorldText) {
        write_part(world, e, kind, props, Part::Node)?;
    }
    let mut parts: Vec<Part> = props.keys().map(|k| part(kind, k)).filter(|p| *p != Part::Node).collect();
    match kind {
        UiKind::Text | UiKind::Span => parts.extend([Part::Text, Part::Font]),
        UiKind::WorldText => parts.extend([Part::Text, Part::Font, Part::Color, Part::Layout, Part::LineHeight, Part::WorldStyle]),
        UiKind::Input => {
            if props.contains_key("initialValue") {
                check_part(kind, props, Part::Value)?;
            }
            let text = EditableText::new(props.get("initialValue").and_then(Value::as_str).unwrap_or_default());
            // `EditableText` first, so the one `TextInput` requires does not replace it. One line:
            // no wrapping, so text wider than the field scrolls, focused or not.
            world.entity_mut(e).insert((text, TextLayout::no_wrap())).insert(TextInput);
            cursor(world, e, kind, props)?;
            parts.push(Part::Font);
        }
        _ => {}
    }
    parts.sort();
    parts.dedup();
    for p in parts {
        write_part(world, e, kind, props, p)?;
    }
    if kind == UiKind::Button {
        world.entity_mut(e).insert(bevy::ui_widgets::Button);
    }
    Ok(())
}

/// Rewrites only the components behind props whose value changed between `old` and `new`.
pub fn update(world: &mut World, e: Entity, kind: UiKind, old: &Map<String, Value>, new: &Map<String, Value>) -> Result<usize, String> {
    let mut parts: Vec<Part> = old
        .keys()
        .chain(new.keys())
        .filter(|k| old.get(*k) != new.get(*k))
        .map(|k| part(kind, k))
        // `initialValue` is written on insert only, like every `initial*` prop.
        .filter(|p| *p != Part::Value)
        .collect();
    parts.sort();
    parts.dedup();
    for p in &parts {
        write_part(world, e, kind, new, *p)?;
    }
    Ok(parts.len())
}

pub fn remove(world: &mut World, e: Entity, kind: UiKind) {
    let mut em = world.entity_mut(e);
    em.remove::<(Node, BackgroundColor, BackgroundGradient, BorderColor, BorderGradient, Outline, BoxShadow, UiTransform)>();
    match kind {
        UiKind::Text | UiKind::Span => {
            em.remove::<(Text, TextSpan, TextFont, TextColor, TextLayout, LineHeight, LetterSpacing, TextShadow, TextBackgroundColor)>();
            em.remove::<(Underline, UnderlineColor, Strikethrough, StrikethroughColor)>();
        }
        UiKind::Image => {
            em.remove::<ImageNode>();
        }
        UiKind::Button => {
            em.remove::<bevy::ui_widgets::Button>();
        }
        UiKind::Viewport => {
            em.remove::<ViewportNode>();
        }
        UiKind::Input => {
            em.remove::<(TextInput, EditableText, TextFont, TextColor, TextLayout, TextCursorStyle, AutoFocus)>();
        }
        // The label's mesh, material and bounds go with `WorldText` (world text's own observer);
        // `NotShadowCaster` came with it, so a `<mesh>` in its place casts a shadow again.
        UiKind::WorldText => {
            em.remove::<(WorldText, WorldTextStyle, TextFont, TextColor, TextLayout, LineHeight, bevy::light::NotShadowCaster)>();
        }
        UiKind::Node => {}
    }
    em.remove::<(UiTargetCamera, InteractionDisabled, ActivateOnPress)>();
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn lengths_and_sides() {
        assert_eq!(val(&json!(8)).unwrap(), Val::Px(8.0));
        assert_eq!(val(&json!("50%")).unwrap(), Val::Percent(50.0));
        assert_eq!(val(&json!("2.5rem")).unwrap(), Val::Rem(2.5));
        assert_eq!(val(&json!("auto")).unwrap(), Val::Auto);
        assert!(val(&json!("8pt")).is_err());
        assert_eq!(rect(&json!("8px")).unwrap(), UiRect::all(Val::Px(8.0)));
        assert_eq!(rect(&json!([1, 2])).unwrap(), UiRect::axes(Val::Px(2.0), Val::Px(1.0)));
        assert_eq!(rect(&json!({ "top": 3 })).unwrap().top, Val::Px(3.0));
    }

    #[test]
    fn unknown_props_and_style_keys_are_errors() {
        assert!(check(UiKind::Node, json!({ "colour": "#fff" }).as_object().unwrap()).is_err());
        assert!(check(UiKind::Node, json!({ "style": { "widht": 3 } }).as_object().unwrap()).is_err());
        assert!(check(UiKind::Text, json!({ "text": "a", "size": 3, "color": "#fff" }).as_object().unwrap()).is_ok());
    }

    fn world(props: Value) -> Result<(), String> {
        check(UiKind::WorldText, props.as_object().unwrap())
    }

    #[test]
    fn world_text_takes_its_props() {
        let all = json!({
            "text": "Paris", "font": "fonts/Inter.ttf", "weight": 700, "italic": true, "justify": "center", "linebreak": "word",
            "lineHeight": 1.4, "color": "#ffcc00", "size": 0.5, "letterSpacing": 0.05, "width": 4, "anchor": [0.25, -0.5],
            "outline": { "width": 0.1, "color": "#000" }, "lit": true,
        });
        world(all.clone()).unwrap();
        let s = world_style(all.as_object().unwrap()).unwrap();
        assert_eq!((s.size, s.width, s.anchor, s.lit), (0.5, Some(4.0), Vec2::new(0.25, -0.5), true));
        assert_eq!(s.outline.map(|(w, _)| w), Some(0.1));
        assert_eq!(anchor(&json!("bottom-left")).unwrap(), Vec2::new(-0.5, -0.5));
        assert_eq!(anchor(&json!("top")).unwrap(), Vec2::new(0.0, 0.5));
        assert_eq!(world_style(&Map::new()).unwrap(), WorldTextStyle::default());
    }

    /// Every refusal names the prop it refuses.
    #[test]
    fn world_text_refusals_name_the_prop() {
        let cases = [
            (json!({ "style": { "width": 3 } }), "style"),
            (json!({ "textShadow": { "x": 1 } }), "textShadow"),
            (json!({ "underline": true }), "underline"),
            (json!({ "strikethrough": true }), "strikethrough"),
            (json!({ "textBackground": "#000" }), "textBackground"),
            (json!({ "path": [[0, 0, 0], [1, 0, 0]] }), "path"),
            (json!({ "size": 0 }), "size"),
            (json!({ "size": -1 }), "size"),
            (json!({ "size": "big" }), "size"),
            (json!({ "width": 0 }), "width"),
            (json!({ "width": "4" }), "width"),
            (json!({ "outline": { "width": 0.3, "color": "#000" } }), "outline.width"),
            (json!({ "outline": { "width": -0.1, "color": "#000" } }), "outline.width"),
            (json!({ "outline": { "color": "#000" } }), "outline"),
            (json!({ "outline": 0.1 }), "outline"),
            (json!({ "outline": { "width": 0.1, "color": "nope" } }), "outline.color"),
            (json!({ "anchor": "middle" }), "anchor"),
            (json!({ "anchor": [1, 0] }), "anchor"),
            (json!({ "anchor": [0] }), "anchor"),
            (json!({ "lineHeight": "12px" }), "lineHeight"),
            (json!({ "letterSpacing": "1px" }), "letterSpacing"),
            (json!({ "lit": "yes" }), "lit"),
            (json!({ "color": 3 }), "color"),
            (json!({ "justify": "middle" }), "justify"),
            (json!({ "weight": 0 }), "weight"),
            (json!({ "text": 3 }), "text"),
        ];
        for (props, prop) in cases {
            let e = world(props.clone()).expect_err(&format!("{props} is refused"));
            assert!(e.contains(prop), "{props}: {e} names {prop}");
        }
        assert!(world(json!({ "outline": { "width": 0.3, "color": "#000" } })).unwrap_err().contains("0 to 0.125"));
    }
}
