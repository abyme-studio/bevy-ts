use super::*;

fn apply(src: &str, ops: &[Op]) -> Result<Written, Refused> {
    super::apply(src, ops, false, None)
}

fn apply_sidecar(src: &str, ops: &[Op]) -> Result<Written, Refused> {
    super::apply(src, ops, true, None)
}

fn p(s: &str) -> Vec<String> {
    if s.is_empty() { Vec::new() } else { s.split('/').map(String::from).collect() }
}

fn f(v: f64) -> WValue {
    WValue::Float { v, f32: true }
}

fn set(path: &str, component: &str, field: &str, value: WValue) -> Op {
    let field = p(&field.replace('.', "/"));
    let containers = if field.len() > 1 { vec!["Vec3".to_string(); field.len() - 1] } else { Vec::new() };
    Op::Set { path: p(path), component: component.into(), field, value, containers }
}

/// The lines that differ, as `-old` and `+new`.
fn diff(a: &str, b: &str) -> Vec<String> {
    let (al, bl): (Vec<&str>, Vec<&str>) = (a.lines().collect(), b.lines().collect());
    let mut start = 0;
    while start < al.len() && start < bl.len() && al[start] == bl[start] {
        start += 1;
    }
    let (mut ea, mut eb) = (al.len(), bl.len());
    while ea > start && eb > start && al[ea - 1] == bl[eb - 1] {
        ea -= 1;
        eb -= 1;
    }
    al[start..ea].iter().map(|l| format!("-{l}")).chain(bl[start..eb].iter().map(|l| format!("+{l}"))).collect()
}

fn ok(src: &str, ops: &[Op]) -> Written {
    apply(src, ops).unwrap_or_else(|e| panic!("{e:?}"))
}

/// Applies `ops`, checks the diff, then that the inverse gives the original back.
fn roundtrip(src: &str, ops: &[Op], expect: &[&str]) -> Written {
    let w = ok(src, ops);
    assert_eq!(diff(src, &w.text), expect, "\n{}", w.text);
    let back = ok(&w.text, &w.inverse);
    assert_eq!(back.text, src, "inverse of {ops:?}: {:?}", w.inverse);
    w
}

const ARENA: &str = "// The arena: two enemies.

#EnemyA
@game::Enemy
Transform { translation: Vec3 { x: -3.0, z: -4.0 } }
--
#EnemyB
@game::Enemy { @hp: 250.0 }
Transform { translation: Vec3 { x: 3.0, z: -4.0 } } // placed by hand
--
#Shelf
Transform {
    translation: Vec3 {
        x: 4.0,
        y: 0.0, /* inline */
    },
}
";

#[test]
fn setting_a_value_changes_one_line() {
    let w = roundtrip(ARENA, &[set("EnemyB", "Transform", "translation.x", f(5.0))], &[
        "-Transform { translation: Vec3 { x: 3.0, z: -4.0 } } // placed by hand",
        "+Transform { translation: Vec3 { x: 5.0, z: -4.0 } } // placed by hand",
    ]);
    assert_eq!(w.lines, [(9, 9)]);
    assert_eq!(w.inverse, [set("EnemyB", "Transform", "translation.x", WValue::Float { v: 3.0, f32: false })]);
}

#[test]
fn a_new_lane_on_a_single_line_patch() {
    roundtrip(ARENA, &[set("EnemyA", "Transform", "translation.y", f(-2.0))], &[
        "-Transform { translation: Vec3 { x: -3.0, z: -4.0 } }",
        "+Transform { translation: Vec3 { x: -3.0, z: -4.0, y: -2.0 } }",
    ]);
}

#[test]
fn a_new_lane_on_a_multi_line_patch_keeps_its_comment() {
    let w = ok(ARENA, &[set("Shelf", "Transform", "translation.z", f(1.5))]);
    assert_eq!(diff(ARENA, &w.text), ["+        z: 1.5,"]);
    assert!(w.text.contains("        y: 0.0, /* inline */\n        z: 1.5,\n"));
}

#[test]
fn a_new_field_on_a_multi_line_patch() {
    let w = ok(ARENA, &[set("Shelf", "Transform", "scale", WValue::Vector { ty: "Vec3".into(), lanes: vec![("y".into(), 2.0)] })]);
    assert_eq!(diff(ARENA, &w.text), ["+    scale: Vec3 { y: 2.0 },"]);
}

#[test]
fn a_new_patch_goes_on_its_own_line_after_a_trailing_comment() {
    let w = roundtrip(ARENA, &[set("EnemyB", "game::Health", "max", f(400.0))], &["+game::Health { max: 400.0 }"]);
    assert!(w.text.contains("// placed by hand\ngame::Health { max: 400.0 }\n--"));
}

#[test]
fn an_override_inside_a_prefab_makes_its_children_list() {
    let w = ok(ARENA, &[set("EnemyB/eye", "Transform", "translation.y", f(0.8))]);
    assert_eq!(diff(ARENA, &w.text), ["+Children [", "+    #eye", "+    Transform { translation: Vec3 { y: 0.8 } }", "+]"]);
    assert_eq!(w.inverse, [Op::Revert { path: p("EnemyB/eye"), component: "Transform".into(), field: p("translation/y") }]);
    // A second override child goes into the same list.
    let w2 = ok(&w.text, &[set("EnemyB/hat", "Transform", "scale.x", f(2.0))]);
    assert_eq!(diff(&w.text, &w2.text), ["+    --", "+    #hat", "+    Transform { scale: Vec3 { x: 2.0 } }"]);
}

#[test]
fn an_unnamed_entity_is_named_when_first_written() {
    let src = "Crate Transform\n--\nCrate Transform\n--\nCrate Transform\n";
    let w = ok(src, &[set("#2", "Transform", "translation.x", f(1.0))]);
    assert_eq!(diff(src, &w.text), ["-Crate Transform", "+#Crate3 Crate Transform { translation: Vec3 { x: 1.0 } }"]);
}

#[test]
fn reverting_removes_the_override() {
    let w = ok(ARENA, &[Op::Revert { path: p("EnemyB"), component: "Transform".into(), field: p("translation/x") }]);
    assert_eq!(diff(ARENA, &w.text), [
        "-Transform { translation: Vec3 { x: 3.0, z: -4.0 } } // placed by hand",
        "+Transform { translation: Vec3 { z: -4.0 } } // placed by hand",
    ]);
    // The inverse sets the lane again (after the others: same value, its own place).
    assert!(ok(&w.text, &w.inverse).text.contains("Transform { translation: Vec3 { z: -4.0, x: 3.0 } } // placed by hand"));
    // The last field goes with its patch's line.
    let src = "#A\n@game::Enemy\nTransform { translation: Vec3 { x: 1.0 } }\n";
    let w = ok(src, &[Op::Revert { path: p("A"), component: "Transform".into(), field: p("translation/x") }]);
    assert_eq!(w.text, "#A\n@game::Enemy\n");
}

#[test]
fn deleting_takes_leading_comments_and_one_separator() {
    let src = "#A Marker\n--\n// B is the boss.\n#B Marker\n--\n#C Marker\n";
    let w = ok(src, &[Op::Delete { path: p("B") }]);
    assert_eq!(w.text, "#A Marker\n--\n#C Marker\n");
    let last = ok(src, &[Op::Delete { path: p("C") }]);
    assert_eq!(last.text, "#A Marker\n--\n// B is the boss.\n#B Marker\n");
    // The inverse puts the entity back (at the end of its list).
    let back = ok(&last.text, &last.inverse);
    assert_eq!(back.text, src);
}

#[test]
fn adding_an_instance_copies_the_style() {
    let w = roundtrip(ARENA, &[Op::Add {
        parent: Vec::new(),
        name: Some("EnemyC".into()),
        base: Base::Scene { component: "game::Enemy".into(), props: vec![("hp".into(), f(50.0))], fields: vec![] },
        patches: vec![NewPatch { component: "Transform".into(), fields: vec![("translation".into(), WValue::Vector { ty: "Vec3".into(), lanes: vec![("x".into(), 6.0)] })] }],
        asset: None,
    }], &["+--", "+#EnemyC", "+@game::Enemy { @hp: 50.0 }", "+Transform { translation: Vec3 { x: 6.0 } }"]);
    assert!(w.text.ends_with("}\n--\n#EnemyC\n@game::Enemy { @hp: 50.0 }\nTransform { translation: Vec3 { x: 6.0 } }\n"), "{}", w.text);
    let room = ok("#Room Transform\n", &[Op::Add { parent: p("Room"), name: Some("Door".into()), base: Base::Include("levels/door.bsn".into()), patches: vec![], asset: None }]);
    assert_eq!(room.text, "#Room Transform\nChildren [\n    #Door\n    :\"levels/door.bsn\"\n]\n");
}

#[test]
fn renaming_and_reparenting() {
    roundtrip(ARENA, &[Op::Rename { path: p("EnemyA"), to: "Grunt".into() }], &["-#EnemyA", "+#Grunt"]);
    let src = "#A Marker\n--\n#B\nMarker\nChildren [\n    #C Marker\n]\n";
    let w = ok(src, &[Op::Reparent { path: p("A"), parent: p("B"), index: None }]);
    assert_eq!(w.text, "#B\nMarker\nChildren [\n    #C Marker\n    --\n    #A Marker\n]\n");
    // The inverse puts it back where it was.
    assert_eq!(ok(&w.text, &w.inverse).text, src);
}

#[test]
fn setting_props() {
    roundtrip(ARENA, &[Op::SetProp { path: p("EnemyB"), prop: "hp".into(), value: f(400.0), field: false }], &["-@game::Enemy { @hp: 250.0 }", "+@game::Enemy { @hp: 400.0 }"]);
    let w = ok(ARENA, &[Op::SetProp { path: p("EnemyB"), prop: "team".into(), value: WValue::Int(2), field: true }]);
    assert_eq!(diff(ARENA, &w.text), ["-@game::Enemy { @hp: 250.0 }", "+@game::Enemy { @hp: 250.0, team: 2 }"]);
    let w = roundtrip(ARENA, &[Op::SetProp { path: p("EnemyA"), prop: "hp".into(), value: f(1.0), field: false }], &["-@game::Enemy", "+@game::Enemy { @hp: 1.0 }"]);
    assert_eq!(w.inverse, [Op::RevertProp { path: p("EnemyA"), prop: "hp".into(), field: false }]);
}

#[test]
fn reverting_props() {
    let w = roundtrip(ARENA, &[Op::RevertProp { path: p("EnemyB"), prop: "hp".into(), field: false }], &["-@game::Enemy { @hp: 250.0 }", "+@game::Enemy"]);
    assert_eq!(w.inverse, [Op::SetProp { path: p("EnemyB"), prop: "hp".into(), value: WValue::Float { v: 250.0, f32: false }, field: false }]);
    let src = ARENA.replace("{ @hp: 250.0 }", "{ @hp: 250.0, team: 2 }");
    let w = ok(&src, &[Op::RevertProp { path: p("EnemyB"), prop: "hp".into(), field: false }]);
    assert_eq!(diff(&src, &w.text), ["-@game::Enemy { @hp: 250.0, team: 2 }", "+@game::Enemy { team: 2 }"]);
    // A field and a prop are told apart by `@`.
    let e = apply(&src, &[Op::RevertProp { path: p("EnemyB"), prop: "hp".into(), field: true }]).unwrap_err();
    assert_eq!(e.reason, "not-found");
    let e = apply(ARENA, &[Op::RevertProp { path: p("EnemyA"), prop: "hp".into(), field: false }]).unwrap_err();
    assert_eq!(e.reason, "not-found");
}

#[test]
fn a_batch_applies_whole_or_not_at_all() {
    let e = apply(ARENA, &[set("EnemyB", "Transform", "translation.x", f(5.0)), Op::Delete { path: p("Nobody") }]).unwrap_err();
    assert_eq!(e.reason, "not-found");
    let w = ok(ARENA, &[set("EnemyB", "Transform", "translation.x", f(5.0)), set("EnemyA", "Transform", "translation.x", f(-5.0))]);
    assert_eq!(w.lines, [(5, 5), (9, 9)]);
    assert_eq!(ok(&w.text, &w.inverse).text, ARENA);
}

#[test]
fn floats_print_shortest_with_a_dot() {
    assert_eq!(float(3.0, true), "3.0");
    assert_eq!(float(0.1f32 as f64, true), "0.1");
    assert_eq!(float(-2.5, false), "-2.5");
    assert_eq!(float(1e21, false), "1000000000000000000000.0");
}

#[test]
fn existing_full_paths_are_kept() {
    let src = "#A bevy_transform::components::transform::Transform\n--\n#B Marker\n";
    let w = ok(src, &[set("B", "bevy_transform::components::transform::Transform", "scale.x", f(2.0))]);
    assert!(w.text.contains("#B Marker\nbevy_transform::components::transform::Transform { scale: Vec3 { x: 2.0 } }"), "{}", w.text);
}

#[test]
fn reverting_an_override_child_removes_its_entry() {
    let w = ok(ARENA, &[set("EnemyB/eye", "Transform", "translation.y", f(0.8))]);
    let back = ok(&w.text, &w.inverse);
    assert_eq!(back.text, ARENA);
}

#[test]
fn a_prefab_internal_entity_cannot_be_deleted() {
    let e = apply(ARENA, &[Op::Delete { path: p("EnemyB/eye") }]).unwrap_err();
    assert_eq!(e.reason, "prefab-internal");
}

#[test]
fn a_sidecar_creates_paths_from_the_top() {
    assert_eq!(apply("", &[set("PlayerUnit#0", "Transform", "translation", f(1.0))]).unwrap_err().reason, "not-found");
    let w = apply_sidecar("", &[set("PlayerUnit#0/gun", "Transform", "scale", WValue::Vector { ty: "Vec3".into(), lanes: vec![("x".into(), 2.0)] })]).unwrap();
    assert_eq!(w.text, "#\"PlayerUnit#0\"\nChildren [\n    #gun\n    Transform { scale: Vec3 { x: 2.0 } }\n]\n");
    let w2 = apply_sidecar(&w.text, &[set("Hud#0", "Transform", "scale", WValue::Vector { ty: "Vec3".into(), lanes: vec![("y".into(), 3.0)] })]).unwrap();
    assert!(w2.text.ends_with("]\n--\n#\"Hud#0\"\nTransform { scale: Vec3 { y: 3.0 } }\n"), "{}", w2.text);
}

fn add(parent: &str, name: Option<&str>, base: Base, patches: Vec<NewPatch>, asset: Option<&str>) -> Op {
    Op::Add { parent: p(parent), name: name.map(String::from), base, patches, asset: asset.map(String::from) }
}

fn scene(component: &str) -> Base {
    Base::Scene { component: component.into(), props: vec![], fields: vec![] }
}

fn patch(component: &str, fields: Vec<(&str, WValue)>) -> NewPatch {
    NewPatch { component: component.into(), fields: fields.into_iter().map(|(n, v)| (n.into(), v)).collect() }
}

#[test]
fn an_unnamed_add_takes_the_first_free_name() {
    let enemy = || add("", None, scene("enemy::Enemy"), vec![], None);
    let w = ok(ARENA, &[enemy(), enemy()]);
    assert_eq!(w.created, [p("Enemy"), p("Enemy2")]);
    assert!(w.text.ends_with("}\n--\n#Enemy\n@enemy::Enemy\n--\n#Enemy2\n@enemy::Enemy\n"), "{}", w.text);
    // The base loses its digits; a taken base takes the next number.
    assert_eq!(ok(ARENA, &[add("", None, scene("game::Enemy3"), vec![], None)]).created, [p("Enemy")]);
    assert_eq!(ok("#shelf Marker\n", &[add("", None, Base::None, vec![], Some("models/shelf.glb"))]).created, [p("shelf2")]);
    // An asset's stem, a first patch's short name, else Entity.
    assert_eq!(ok(ARENA, &[add("", None, Base::None, vec![], Some("models/crate.glb#Scene1"))]).created, [p("crate")]);
    assert_eq!(ok(ARENA, &[add("", None, Base::None, vec![patch("bevy_light::PointLight", vec![])], None)]).created, [p("PointLight")]);
    assert_eq!(ok(ARENA, &[add("", None, Base::None, vec![], None)]).created, [p("Entity")]);
    // A given name that is taken is refused.
    assert_eq!(apply(ARENA, &[add("", Some("EnemyA"), Base::None, vec![], None)]).unwrap_err().reason, "exists");
}

#[test]
fn a_reparent_and_its_inverse_give_the_file_back() {
    let src = "#A Marker\n--\n#B Marker\n--\n#C Marker\n";
    let w = ok(src, &[Op::Reparent { path: p("A"), parent: p("C"), index: None }]);
    assert_eq!(w.text, "#B Marker\n--\n#C Marker\nChildren [\n    #A Marker\n]\n");
    assert_eq!(w.renamed, [(p("A"), p("C/A"))]);
    assert_eq!(w.inverse, [Op::Reparent { path: p("C/A"), parent: p(""), index: Some(0) }]);
    assert_eq!(ok(&w.text, &w.inverse).text, src);
    // The same parent with an index reorders, and keeps the path.
    for (index, expect) in [(1, "#B Marker\n--\n#A Marker\n--\n#C Marker\n"), (2, "#B Marker\n--\n#C Marker\n--\n#A Marker\n")] {
        let w = ok(src, &[Op::Reparent { path: p("A"), parent: p(""), index: Some(index) }]);
        assert_eq!(w.text, expect);
        assert_eq!(w.renamed, [(p("A"), p("A"))], "a reorder is a move the engine follows");
        assert_eq!(ok(&w.text, &w.inverse).text, src);
    }
    // A child with its comment, out and back.
    let room = "#Room\nChildren [\n    // The lamp.\n    #Lamp Marker\n    --\n    #Door Marker\n]\n--\n#Hall Marker\n";
    let w = ok(room, &[Op::Reparent { path: p("Room/Lamp"), parent: p(""), index: Some(0) }]);
    assert_eq!(w.text, "// The lamp.\n#Lamp Marker\n--\n#Room\nChildren [\n    #Door Marker\n]\n--\n#Hall Marker\n");
    assert_eq!(ok(&w.text, &w.inverse).text, room);
    let w = ok(room, &[Op::Reparent { path: p("Room/Lamp"), parent: p("Hall"), index: None }]);
    assert_eq!(ok(&w.text, &w.inverse).text, room);
    assert_eq!(apply(room, &[Op::Reparent { path: p("Room"), parent: p("Room/Door"), index: None }]).unwrap_err().reason, "invalid");
}

#[test]
fn a_middle_delete_and_its_restore_give_the_file_back() {
    let src = "#A Marker\n--\n// B is the boss.\n#B Marker\n--\n#C Marker\n";
    let w = ok(src, &[Op::Delete { path: p("B") }]);
    assert_eq!(w.inverse, [Op::Restore { parent: p(""), index: 1, text: "// B is the boss.\n#B Marker".into() }]);
    let back = ok(&w.text, &w.inverse);
    assert_eq!(back.text, src);
    assert_eq!(back.created, [p("B")]);
    let room = "#Room\nChildren [\n    #Lamp Marker\n    --\n    #Door\n    Marker\n    --\n    #Hall Marker\n]\n";
    let w = ok(room, &[Op::Delete { path: p("Room/Door") }]);
    assert_eq!(ok(&w.text, &w.inverse).text, room);
    // An only child goes with its list, and comes back with it.
    let one = "#Room Marker\nChildren [\n    #Lamp Marker\n]\n";
    let w = ok(one, &[Op::Delete { path: p("Room/Lamp") }]);
    assert_eq!(w.text, "#Room Marker\n");
    assert_eq!(ok(&w.text, &w.inverse).text, one);
}

#[test]
fn components_are_added_and_removed_as_patches() {
    let src = "#Crate\nTransform\n--\n#PointLight\nPointLight { intensity: 1500.0 }\n";
    let add_c = |path: &str, c: &str, fields: Vec<(String, WValue)>| Op::AddComponent { path: p(path), component: c.into(), fields, text: None };
    let w = roundtrip(src, &[add_c("Crate", "NotShadowCaster", vec![])], &["+NotShadowCaster"]);
    assert_eq!(w.inverse, [Op::RemoveComponent { path: p("Crate"), component: "NotShadowCaster".into() }]);
    roundtrip(src, &[add_c("Crate", "game::Health", vec![("max".into(), f(4.0))])], &["+game::Health { max: 4.0 }"]);
    assert_eq!(apply(src, &[add_c("PointLight", "PointLight", vec![])]).unwrap_err().reason, "exists");
    let w = roundtrip(src, &[Op::RemoveComponent { path: p("PointLight"), component: "PointLight".into() }], &["-PointLight { intensity: 1500.0 }"]);
    assert_eq!(w.inverse, [Op::AddComponent { path: p("PointLight"), component: "PointLight".into(), fields: vec![], text: Some("PointLight { intensity: 1500.0 }".into()) }]);
    assert_eq!(apply(src, &[Op::RemoveComponent { path: p("Crate"), component: "PointLight".into() }]).unwrap_err().reason, "not-found");
}

#[test]
fn an_asset_is_placed_by_its_path() {
    let at = patch("Transform", vec![("translation", WValue::Vector { ty: "Vec3".into(), lanes: vec![("x".into(), 2.0), ("y".into(), 0.0), ("z".into(), 1.0)] })]);
    let w = roundtrip("#Floor Marker\n", &[add("", None, Base::None, vec![at], Some("models/crate.glb"))], &[
        "+--",
        "+#crate",
        "+SceneRoot(\"models/crate.glb#Scene0\")",
        "+Transform { translation: Vec3 { x: 2.0, y: 0.0, z: 1.0 } }",
    ]);
    assert_eq!(w.created, [p("crate")]);
    assert!(ok("", &[add("", None, Base::None, vec![], Some("models/tree.gltf#Scene2"))]).text.contains("SceneRoot(\"models/tree.gltf#Scene2\")"));
    assert_eq!(ok("", &[add("", None, Base::None, vec![], Some("levels/room.bsn"))]).text, "#room\n:\"levels/room.bsn\"\n");
    assert_eq!(apply("", &[add("", None, Base::None, vec![], Some("textures/wood.png"))]).unwrap_err().reason, "invalid");
}

#[test]
fn what_a_prefab_gives_cannot_be_changed_or_parented() {
    let with_eye = ok(ARENA, &[set("EnemyB/eye", "Transform", "translation.y", f(0.8))]).text;
    for op in [
        Op::Delete { path: p("EnemyB/eye") },
        Op::Rename { path: p("EnemyB/eye"), to: "eye2".into() },
        Op::Reparent { path: p("EnemyB/eye"), parent: p(""), index: None },
        Op::Reparent { path: p("EnemyA"), parent: p("EnemyB"), index: None },
        Op::RemoveComponent { path: p("EnemyB/eye"), component: "Transform".into() },
        add("EnemyB", None, Base::None, vec![], None),
        add("EnemyB/eye", None, Base::None, vec![], None),
    ] {
        assert_eq!(apply(&with_eye, std::slice::from_ref(&op)).unwrap_err().reason, "prefab-internal", "{op:?}");
    }
    assert!(written(&syntax::parse(&with_eye).unwrap(), &p("EnemyB")));
    assert!(!written(&syntax::parse(&with_eye).unwrap(), &p("EnemyB/eye")));
}

#[test]
fn a_patch_is_matched_by_the_type_it_resolves_to() {
    let src = "#A\ngame::Health { max: 3.0 }\nTransform\n";
    let resolve = |c: &str| matches!(c, "Health" | "game::Health").then(|| "game::Health".to_string());
    let remove = [Op::RemoveComponent { path: p("A"), component: "Health".into() }];
    assert_eq!(apply(src, &remove).unwrap_err().reason, "not-found", "by text alone, Health is not game::Health");
    let w = super::apply(src, &remove, false, Some(&resolve)).unwrap();
    assert_eq!(w.text, "#A\nTransform\n");
    let add = [Op::AddComponent { path: p("A"), component: "Health".into(), fields: vec![], text: None }];
    assert_eq!(super::apply(src, &add, false, Some(&resolve)).unwrap_err().reason, "exists");
}

#[test]
fn an_unnamed_entity_comes_back_by_its_place() {
    let src = "#A Marker\n--\nCrate\n--\nCrate\n";
    let w = ok(src, &[Op::Delete { path: p("#1") }]);
    let back = ok(&w.text, &w.inverse);
    assert_eq!(back.text, src);
    assert_eq!(back.created, [p("#1")]);
    // Redo: the restore's inverse deletes the same entity.
    assert_eq!(ok(&back.text, &back.inverse).text, w.text);
}

#[test]
fn what_a_batch_adds_then_deletes_is_not_created() {
    let w = ok(ARENA, &[add("Shelf", Some("Lamp"), Base::None, vec![], None), Op::Delete { path: p("Shelf") }]);
    assert!(w.created.is_empty(), "{:?}", w.created);
}

#[test]
fn a_batch_cannot_reuse_a_path_it_changed() {
    let comp = |path: &str| Op::AddComponent { path: p(path), component: "Marker".into(), fields: vec![], text: None };
    let src = "#A Transform\n--\n#B Transform\n--\nCrate\n--\nCrate\n";
    for ops in [
        vec![Op::Delete { path: p("A") }, Op::Rename { path: p("B"), to: "A".into() }, comp("A")],
        vec![Op::Rename { path: p("A"), to: "C".into() }, add("", Some("A"), Base::None, vec![], None), comp("A")],
        vec![Op::Reparent { path: p("A"), parent: p("B"), index: None }, comp("B/A")],
        vec![add("A", Some("Lamp"), Base::None, vec![], None), comp("A/Lamp")],
        // Positional ids shift when their list changes.
        vec![Op::Delete { path: p("#0") }, comp("#0")],
        vec![add("", None, Base::None, vec![], None), comp("#1")],
    ] {
        assert_eq!(apply(src, &ops).map(|w| w.text).unwrap_err().reason, "invalid", "{ops:?}");
    }
    // Ops on paths nothing earlier changed are fine, a component removed then added too.
    ok(src, &[Op::Delete { path: p("A") }, comp("B")]);
    ok(src, &[Op::RemoveComponent { path: p("A"), component: "Transform".into() }, Op::AddComponent { path: p("A"), component: "Transform".into(), fields: vec![], text: None }]);
}

#[test]
fn a_resolved_type_never_falls_back_to_text() {
    let src = "#A\nHealth\nTransform\n";
    let resolve = |c: &str| match c {
        "Health" | "b::Health" => Some("b::Health".to_string()),
        "a::Health" => Some("a::Health".to_string()),
        _ => None,
    };
    let e = super::apply(src, &[Op::RemoveComponent { path: p("A"), component: "a::Health".into() }], false, Some(&resolve)).unwrap_err();
    assert_eq!(e.reason, "not-found", "a::Health is not the file's b::Health");
}

#[test]
fn a_whitespace_line_ends_what_an_entity_owns() {
    let src = "#A Marker\n--\n// Section two.\n    \n#B Marker\n--\n#C Marker\n";
    let w = ok(src, &[Op::Delete { path: p("B") }]);
    assert_eq!(w.text, "#A Marker\n--\n// Section two.\n    \n#C Marker\n");
    assert_eq!(ok(&w.text, &w.inverse).text, src);
}
