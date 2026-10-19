//! Overlay roots (levels' T4): a description layered over an `app.scene` root by key path, so a
//! TSX scene's edits that have no literal to rewrite persist in a sidecar level
//! (`levels/<scene>.bsn`, levels decision 11). An overlay node merges into the scene node at its
//! key path: its component props override the scene's (provenance `override`, with the level's
//! location), components the scene lacks are added, and its children merge by key the same way;
//! a node the scene does not have is added.

use crate::desc::{ComponentDesc, NodeDesc, binding_id};

/// `initialHp` for `hp`.
fn initial_name(field: &str) -> String {
    let mut c = field.chars();
    match c.next() {
        Some(f) => format!("initial{}{}", f.to_uppercase(), c.as_str()),
        None => String::new(),
    }
}

fn merge_component(base: &mut ComponentDesc, over: &ComponentDesc) {
    for (k, v) in &over.props {
        // The override is a controlled value: it replaces an initial value or a binding.
        base.props.remove(&initial_name(k));
        if base.props.get(k).is_some_and(|b| binding_id(b).is_some()) {
            base.props.remove(k);
        }
        base.props.insert(k.clone(), v.clone());
    }
    for (k, at) in &over.overrides {
        base.overrides.insert(k.clone(), at.clone());
    }
}

fn merge_node(base: &mut NodeDesc, over: &NodeDesc) {
    for oc in &over.components {
        match base.components.iter_mut().find(|c| c.ty == oc.ty) {
            Some(c) => merge_component(c, oc),
            None => base.components.push(oc.clone()),
        }
    }
    if over.name.is_some() {
        base.name = over.name.clone();
    }
    merge(&mut base.children, &over.children);
}

/// Layers `overlay` over `scene` by key, in place.
pub fn merge(scene: &mut Vec<NodeDesc>, overlay: &[NodeDesc]) {
    for o in overlay {
        match scene.iter_mut().find(|n| n.key == o.key && n.slot.is_none()) {
            Some(n) => merge_node(n, o),
            None => scene.push(o.clone()),
        }
    }
}

/// The sidecar level of a scene with no `level` option: `levels/<scene>.bsn`.
pub fn default_level(scene: &str) -> String {
    format!("levels/{scene}.bsn")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::desc::ComponentKind;
    use bevy_ts::backend::SourceLocation;
    use serde_json::json;

    fn node(key: &str, comps: Vec<ComponentDesc>, children: Vec<NodeDesc>) -> NodeDesc {
        serde_json::from_value(json!({ "key": key, "type": "entity", "components": comps, "children": children })).unwrap()
    }

    fn comp(ty: &str, props: serde_json::Value) -> ComponentDesc {
        ComponentDesc { ty: ty.into(), kind: ComponentKind::Ts, props: props.as_object().unwrap().clone(), source: None, overrides: Default::default() }
    }

    #[test]
    fn an_overlay_overrides_by_key_path_and_adds_what_is_new() {
        let mut scene = vec![node(
            "a",
            vec![comp("Transform", json!({ "translation": [0, 0, 0] })), comp("game::Health", json!({ "initialHp": 100, "max": { "$b": 3 } }))],
            vec![node("eye", vec![], vec![])],
        )];
        let at = SourceLocation { file: "levels/arena.bsn".into(), line: 4, column: 3 };
        let mut over = comp("game::Health", json!({ "hp": 40, "max": 50 }));
        over.overrides.insert("hp".into(), at.clone());
        over.overrides.insert("max".into(), at.clone());
        let overlay = vec![
            node("a", vec![over, comp("game::Tag", json!({}))], vec![node("eye", vec![comp("Transform", json!({ "scale": [2, 2, 2] }))], vec![]), node("hat", vec![], vec![])]),
            node("z", vec![], vec![]),
        ];
        merge(&mut scene, &overlay);
        assert_eq!(scene.iter().map(|n| n.key.as_str()).collect::<Vec<_>>(), ["a", "z"]);
        let h = &scene[0].components[1];
        assert_eq!(h.props, *json!({ "hp": 40, "max": 50 }).as_object().unwrap(), "initialHp and the binding gave way");
        assert_eq!(h.overrides["hp"], at);
        assert_eq!(scene[0].components[2].ty, "game::Tag");
        assert_eq!(scene[0].children.iter().map(|n| n.key.as_str()).collect::<Vec<_>>(), ["eye", "hat"]);
        assert_eq!(scene[0].children[0].components[0].ty, "Transform");
    }
}
