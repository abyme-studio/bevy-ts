//! bevy_ui intrinsics: `<node>`, `<text>`, `<image>`, `<button>`. Each prop maps to one
//! component (`background` and `border` to a colour or a gradient), so a changed prop writes
//! that component and no other: a bound `<text>` writes `Text` and leaves `TextFont`,
//! `TextColor` and `Node` alone.

use bevy::prelude::*;
use bevy::text::{FontSize, FontSource, FontStyle, FontWeight, LetterSpacing, LineBreak, LineHeight, Strikethrough, StrikethroughColor, TextBackgroundColor, Underline, UnderlineColor};
use bevy::ui::widget::TextShadow;
use bevy::ui::{
    AngularColorStop, BackgroundGradient, BorderGradient, BoxShadow, ColorStop, ConicGradient, CornerRadius, Gradient, InlineDirection, LinearGradient, OverflowAxis,
    OverflowClipMargin, RadialGradient, RadialGradientShape, ShadowStyle, UiPosition, UiTransform, Val2, VisualBox,
};
use serde_json::{Map, Value};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UiKind {
    Node,
    Text,
    Image,
    Button,
    /// A run of rich text under `<text>` or `<span>`: `TextSpan`, no `Node`.
    Span,
}

/// The text props Bevy applies per span; `<text>` adds `justify`, `linebreak` and `textShadow`.
const SPAN_PROPS: &[&str] = &["text", "size", "color", "font", "weight", "italic", "lineHeight", "letterSpacing", "textBackground", "underline", "strikethrough"];

impl UiKind {
    pub fn from_tag(tag: &str) -> Option<UiKind> {
        Some(match tag {
            "node" => UiKind::Node,
            "text" => UiKind::Text,
            "image" => UiKind::Image,
            "button" => UiKind::Button,
            "span" => UiKind::Span,
            _ => return None,
        })
    }

    fn props(self) -> &'static [&'static str] {
        match self {
            UiKind::Node | UiKind::Button => &["style", "background", "border", "radius", "outline", "shadow", "transform"],
            UiKind::Text => &[
                "style", "background", "border", "radius", "outline", "shadow", "transform", "text", "size", "color", "font", "weight", "italic", "lineHeight", "letterSpacing",
                "textBackground", "underline", "strikethrough", "justify", "linebreak", "textShadow",
            ],
            UiKind::Span => SPAN_PROPS,
            UiKind::Image => &["style", "background", "border", "radius", "outline", "shadow", "transform", "src", "color"],
        }
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
}

fn part(kind: UiKind, prop: &str) -> Part {
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
        "src" => Part::Image,
        "color" if kind == UiKind::Image => Part::Image,
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
    let s = v.as_str().ok_or_else(|| format!("{v} is not a colour (\"#rrggbb\" or \"#rrggbbaa\")"))?;
    Srgba::hex(s).map(Color::from).map_err(|e| format!("colour {s:?}: {e}"))
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

/// `border`'s colour: one for every side, or `{ top, right, bottom, left }`.
fn border_color(v: &Value) -> Result<BorderColor, String> {
    let Some(o) = v.as_object() else { return Ok(BorderColor::all(color(v)?)) };
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

/// `underline` and `strikethrough`: `true` (Bevy's colour), a colour, or `false` (none).
fn decoration(v: &Value, prop: &str) -> Result<Option<Option<Color>>, String> {
    match v {
        Value::Bool(true) => Ok(Some(None)),
        Value::Bool(false) => Ok(None),
        Value::String(_) => Ok(Some(Some(color(v)?))),
        _ => Err(format!("{prop} takes true, false or a colour")),
    }
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
        Part::Color => color(&props["color"]).map(|_| ()),
        Part::Layout => layout(props).map(|_| ()),
        Part::LineHeight => line_height(&props["lineHeight"]).map(|_| ()),
        Part::LetterSpacing => letter_spacing(&props["letterSpacing"]).map(|_| ()),
        Part::TextShadow => text_shadow(&props["textShadow"]).map(|_| ()),
        Part::TextBackground => color(&props["textBackground"]).map(|_| ()),
        Part::Underline => decoration(&props["underline"], "underline").map(|_| ()),
        Part::Strikethrough => decoration(&props["strikethrough"], "strikethrough").map(|_| ()),
        Part::Image => {
            if kind == UiKind::Image {
                props.get("src").and_then(Value::as_str).ok_or("<image> needs src=\"path\"")?;
                if let Some(c) = props.get("color") {
                    color(c)?;
                }
            }
            Ok(())
        }
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
            } else {
                world.entity_mut(e).insert(Text::new(s));
            }
        }
        Part::Font => {
            let f = font_fields(props)?;
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
            let h = props.get("lineHeight").map(line_height).transpose()?.unwrap_or_default();
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
        }
        Part::Color => {
            world.entity_mut(e).remove::<TextColor>();
        }
        Part::Image => {
            let src = props.get("src").and_then(Value::as_str).unwrap_or_default().to_string();
            let image = world.get_resource::<AssetServer>().ok_or("<image> needs the asset plugin (no AssetServer)")?.load(src);
            let mut node = ImageNode::new(image);
            if let Some(c) = props.get("color") {
                node.color = color(c)?;
            }
            world.entity_mut(e).insert(node);
        }
    }
    Ok(())
}

fn props_of(kind: UiKind, props: &Map<String, Value>, p: Part) -> bool {
    props.keys().any(|k| part(kind, k) == p)
}

/// Inserts everything a UI intrinsic owns, from its current props.
pub fn insert(world: &mut World, e: Entity, kind: UiKind, props: &Map<String, Value>) -> Result<(), String> {
    if kind != UiKind::Span {
        write_part(world, e, kind, props, Part::Node)?;
    }
    let mut parts: Vec<Part> = props.keys().map(|k| part(kind, k)).filter(|p| *p != Part::Node).collect();
    if matches!(kind, UiKind::Text | UiKind::Span) {
        parts.extend([Part::Text, Part::Font]);
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
        UiKind::Node => {}
    }
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
}
