//! Bevy's value widgets as UI intrinsics (bevy-ui-coverage stage 5): `<checkbox>`, `<radioGroup>`,
//! `<radio>`, `<slider>`, `<sliderThumb>`, `<scrollbar>`, `<scrollbarThumb>`, `<tabList>`, `<tab>`,
//! `<listBox>`, `<listItem>`, and `scrollArea` on `<node>`. `ui.rs` owns the props; this module
//! owns what they write that is not a plain component: the self-update observer of an uncontrolled
//! widget (design 7), a radio group's form, settled from the radios mounted under it, and a tab
//! list's selection by key (design 8).

use crate::tree::TsxNode;
use crate::ui::UiKind;
use bevy::prelude::*;
use bevy::ui_widgets::{
    ControlOrientation, RadioGroup, SelectedTab, SliderOrientation, Tab, TabActivation, TrackClick, checkbox_self_update, radio_self_update, slider_self_update,
    tablist_self_update,
};
use serde_json::Value;

/// The observer running Bevy's `*_self_update` on this widget, present while it is uncontrolled:
/// the widget then changes its own state, as `initial*` asks (design 7).
#[derive(Component)]
pub struct SelfUpdate(Entity);

/// Attaches or takes away the widget's self-update observer.
pub fn self_update(world: &mut World, e: Entity, kind: UiKind, on: bool) {
    let current = world.get::<SelfUpdate>(e).map(|s| s.0);
    match (on, current) {
        (true, None) => {
            let observer = match kind {
                UiKind::Checkbox => Observer::new(checkbox_self_update),
                UiKind::RadioGroup => Observer::new(radio_self_update),
                UiKind::Slider => Observer::new(slider_self_update),
                UiKind::TabList => Observer::new(tablist_self_update),
                _ => return,
            };
            let o = world.spawn(observer.with_entity(e)).id();
            world.entity_mut(e).insert(SelfUpdate(o));
        }
        (false, Some(o)) => {
            world.entity_mut(e).remove::<SelfUpdate>();
            if let Ok(em) = world.get_entity_mut(o) {
                em.despawn();
            }
        }
        _ => {}
    }
}

/// How a mounted `<radio>` holds its state: `checked` (the game's) or `initialChecked` (its own).
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub enum RadioForm {
    Controlled,
    Initial,
}

/// Writes a radio's form, then settles its group. The group is decided from the radios mounted
/// under it, wherever they come from (a `<For>`, a bound child): it gets Bevy's self-update only
/// while none is controlled, and radios of both forms are an error naming the group.
pub fn radio_form(world: &mut World, e: Entity, form: Option<RadioForm>) -> Result<(), String> {
    match form {
        Some(f) => world.entity_mut(e).insert(f),
        None => world.entity_mut(e).remove::<RadioForm>(),
    };
    match group_of(world, e) {
        Some(group) => settle_group(world, group),
        None => Ok(()),
    }
}

/// A radio's group: its nearest `RadioGroup` ancestor.
fn group_of(world: &World, e: Entity) -> Option<Entity> {
    let mut at = e;
    while let Some(p) = world.get::<ChildOf>(at).map(ChildOf::parent) {
        if world.get::<RadioGroup>(p).is_some() {
            return Some(p);
        }
        at = p;
    }
    None
}

fn settle_group(world: &mut World, group: Entity) -> Result<(), String> {
    let (mut controlled, mut initial) = (None, None);
    let mut stack: Vec<Entity> = world.get::<Children>(group).map(|c| c.to_vec()).unwrap_or_default();
    while let Some(d) = stack.pop() {
        match world.get::<RadioForm>(d) {
            Some(RadioForm::Controlled) => controlled = controlled.or(Some(d)),
            Some(RadioForm::Initial) => initial = initial.or(Some(d)),
            None => {}
        }
        // A nested group settles its own radios.
        if world.get::<RadioGroup>(d).is_none() {
            stack.extend(world.get::<Children>(d).map(|c| c.to_vec()).unwrap_or_default());
        }
    }
    if let (Some(c), Some(i)) = (controlled, initial) {
        self_update(world, group, UiKind::RadioGroup, false);
        let key = |e| world.get::<TsxNode>(e).and_then(|n| n.key_path.last().cloned()).unwrap_or_default();
        return Err(format!(
            "is in <radioGroup {}>, whose radios mix checked (<{}>) and initialChecked (<{}>): give every radio one form",
            key(group),
            key(c),
            key(i)
        ));
    }
    self_update(world, group, UiKind::RadioGroup, controlled.is_none());
    Ok(())
}

/// A tab list's `selected` or `initialSelected`: a tab's key, written as `SelectedTab` once a
/// `<tab>` with that key is mounted under it, which is after the list itself (design 8).
#[derive(Component, Debug, PartialEq)]
pub struct TabKey {
    pub key: Option<String>,
    pub resolved: bool,
    /// Whether a key no tab has was reported, once per key.
    pub warned: bool,
}

impl TabKey {
    pub fn new(key: Option<String>) -> Self {
        TabKey { key, resolved: false, warned: false }
    }
}

/// Writes `SelectedTab` for each tab list whose key has not found its tab yet, or whose tab was
/// unmounted (a remount under the same key is a new entity).
pub fn select_tabs(
    mut lists: Query<(Entity, &mut TabKey, Option<&SelectedTab>, &TsxNode)>,
    children: Query<&Children>,
    tabs: Query<&TsxNode, With<Tab>>,
    info: Option<Res<bevy_ts::runtime::RuntimeInfo>>,
    mut log: ResMut<bevy_ts::log::ScriptLog>,
    mut commands: Commands,
) {
    for (list, mut want, selected, node) in &mut lists {
        if want.resolved {
            match selected {
                Some(SelectedTab(Some(t))) if tabs.get(*t).is_err() => want.resolved = false,
                _ => continue,
            }
        }
        let Some(key) = want.key.clone() else {
            commands.entity(list).insert(SelectedTab(None));
            want.resolved = true;
            continue;
        };
        if let Some(tab) = children.iter_descendants(list).find(|&c| tabs.get(c).is_ok_and(|n| n.key_path.last() == Some(&key))) {
            commands.entity(list).insert(SelectedTab(Some(tab)));
            want.resolved = true;
            want.warned = false;
        } else if !want.warned {
            want.warned = true;
            let list_key = node.key_path.last().map(String::as_str).unwrap_or("");
            let generation = info.as_ref().map_or(0, |i| i.generation);
            log.push(bevy_ts::log::LogLevel::Warn, format!("<tabList {list_key}> selects {key:?}, but no <tab> under it has that key yet"), None, generation);
        }
    }
}

fn word<'a>(v: &'a Value, prop: &str, options: &str) -> Result<&'a str, String> {
    v.as_str().ok_or_else(|| format!("{prop} takes one of {options}, not {v}"))
}

pub fn slider_orientation(v: &Value) -> Result<SliderOrientation, String> {
    const O: &str = "auto, horizontal, vertical";
    Ok(match word(v, "orientation", O)? {
        "auto" => SliderOrientation::Auto,
        "horizontal" => SliderOrientation::Horizontal,
        "vertical" => SliderOrientation::Vertical,
        w => return Err(format!("orientation: {w:?} is not one of {O}")),
    })
}

pub fn track_click(v: &Value) -> Result<TrackClick, String> {
    const O: &str = "drag, step, snap";
    Ok(match word(v, "trackClick", O)? {
        "drag" => TrackClick::Drag,
        "step" => TrackClick::Step,
        "snap" => TrackClick::Snap,
        w => return Err(format!("trackClick: {w:?} is not one of {O}")),
    })
}

pub fn control_orientation(v: &Value) -> Result<ControlOrientation, String> {
    const O: &str = "horizontal, vertical";
    Ok(match word(v, "orientation", O)? {
        "horizontal" => ControlOrientation::Horizontal,
        "vertical" => ControlOrientation::Vertical,
        w => return Err(format!("orientation: {w:?} is not one of {O}")),
    })
}

pub fn activation(v: &Value) -> Result<TabActivation, String> {
    const O: &str = "manual, automatic";
    Ok(match word(v, "activation", O)? {
        "manual" => TabActivation::Manual,
        "automatic" => TabActivation::Automatic,
        w => return Err(format!("activation: {w:?} is not one of {O}")),
    })
}

pub fn number(v: &Value, prop: &str) -> Result<f32, String> {
    v.as_f64().map(|n| n as f32).ok_or_else(|| format!("{prop} takes a number, not {v}"))
}

/// A tab's key, or `null` for no tab.
pub fn tab_key(v: &Value, prop: &str) -> Result<Option<String>, String> {
    match v {
        Value::Null => Ok(None),
        Value::String(s) => Ok(Some(s.clone())),
        Value::Number(n) => Ok(Some(n.to_string())),
        _ => Err(format!("{prop} takes a tab's key or null, not {v}")),
    }
}
