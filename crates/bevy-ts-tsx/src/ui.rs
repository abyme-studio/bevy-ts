//! bevy_ui intrinsics: `<node>`, `<text>`, `<image>`, `<button>`. Each prop maps to one
//! component, so a changed prop writes that component and no other: a bound `<text>` writes
//! `Text` and leaves `TextFont`, `TextColor` and `Node` alone.

use bevy::prelude::*;
use bevy::text::FontSize;
use serde_json::{Map, Value};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UiKind {
    Node,
    Text,
    Image,
    Button,
}

impl UiKind {
    pub fn from_tag(tag: &str) -> Option<UiKind> {
        Some(match tag {
            "node" => UiKind::Node,
            "text" => UiKind::Text,
            "image" => UiKind::Image,
            "button" => UiKind::Button,
            _ => return None,
        })
    }

    fn props(self) -> &'static [&'static str] {
        match self {
            UiKind::Node | UiKind::Button => &["style", "background", "border", "radius"],
            UiKind::Text => &["style", "background", "border", "radius", "text", "size", "color"],
            UiKind::Image => &["style", "background", "border", "radius", "src", "color"],
        }
    }
}

/// The component a prop writes. Props sharing one (`style`, `radius`) write it once.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Part {
    Node,
    Background,
    Border,
    Text,
    Font,
    Color,
    Image,
}

fn part(kind: UiKind, prop: &str) -> Part {
    match prop {
        "style" | "radius" => Part::Node,
        "background" => Part::Background,
        "border" => Part::Border,
        "text" => Part::Text,
        "size" => Part::Font,
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
                node.overflow = match word(v, key)? {
                    "visible" => Overflow::visible(),
                    "clip" => Overflow::clip(),
                    "hidden" => Overflow::hidden(),
                    "scroll" => Overflow::scroll(),
                    w => return Err(bad(key, w, "visible, clip, hidden, scroll")),
                }
            }
            other => return Err(format!("style has no {other}")),
        }
    }
    Ok(())
}

pub fn color(v: &Value) -> Result<Color, String> {
    let s = v.as_str().ok_or_else(|| format!("{v} is not a colour (\"#rrggbb\" or \"#rrggbbaa\")"))?;
    Srgba::hex(s).map(Color::from).map_err(|e| format!("colour {s:?}: {e}"))
}

fn build_node(props: &Map<String, Value>) -> Result<Node, String> {
    let mut n = Node::default();
    if let Some(s) = props.get("style") {
        style(&mut n, s)?;
    }
    if let Some(r) = props.get("radius") {
        n.border_radius = BorderRadius::all(val(r)?);
    }
    Ok(n)
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
        Part::Background => color(&props["background"]).map(|_| ()),
        Part::Border => color(&props["border"]).map(|_| ()),
        Part::Text => props["text"].as_str().map(|_| ()).ok_or_else(|| "text takes a string".into()),
        Part::Font => props["size"].as_f64().map(|_| ()).ok_or_else(|| "size takes a number".into()),
        Part::Color => color(&props["color"]).map(|_| ()),
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
        Part::Background if present => {
            world.entity_mut(e).insert(BackgroundColor(color(&props["background"])?));
        }
        Part::Background => {
            world.entity_mut(e).remove::<BackgroundColor>();
        }
        Part::Border if present => {
            world.entity_mut(e).insert(BorderColor::all(color(&props["border"])?));
        }
        Part::Border => {
            world.entity_mut(e).remove::<BorderColor>();
        }
        Part::Text => {
            let s = props.get("text").and_then(Value::as_str).unwrap_or_default();
            world.entity_mut(e).insert(Text::new(s));
        }
        Part::Font => {
            let size = props.get("size").and_then(Value::as_f64).map(|n| n as f32);
            world.entity_mut(e).insert(TextFont { font_size: FontSize::Px(size.unwrap_or(20.0)), ..default() });
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
    write_part(world, e, kind, props, Part::Node)?;
    let mut parts: Vec<Part> = props.keys().map(|k| part(kind, k)).filter(|p| *p != Part::Node).collect();
    if kind == UiKind::Text {
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
    em.remove::<(Node, BackgroundColor, BorderColor)>();
    match kind {
        UiKind::Text => {
            em.remove::<(Text, TextFont, TextColor)>();
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
