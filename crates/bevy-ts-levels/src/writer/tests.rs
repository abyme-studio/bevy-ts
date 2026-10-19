use super::*;

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
        name: "EnemyC".into(),
        base: Base::Scene { component: "game::Enemy".into(), props: vec![("hp".into(), f(50.0))], fields: vec![] },
        patches: vec![NewPatch { component: "Transform".into(), fields: vec![("translation".into(), WValue::Vector { ty: "Vec3".into(), lanes: vec![("x".into(), 6.0)] })] }],
    }], &["+--", "+#EnemyC", "+@game::Enemy { @hp: 50.0 }", "+Transform { translation: Vec3 { x: 6.0 } }"]);
    assert!(w.text.ends_with("}\n--\n#EnemyC\n@game::Enemy { @hp: 50.0 }\nTransform { translation: Vec3 { x: 6.0 } }\n"), "{}", w.text);
    let room = ok("#Room Transform\n", &[Op::Add { parent: p("Room"), name: "Door".into(), base: Base::Include("levels/door.bsn".into()), patches: vec![] }]);
    assert_eq!(room.text, "#Room Transform\nChildren [\n    #Door\n    :\"levels/door.bsn\"\n]\n");
}

#[test]
fn renaming_and_reparenting() {
    roundtrip(ARENA, &[Op::Rename { path: p("EnemyA"), to: "Grunt".into() }], &["-#EnemyA", "+#Grunt"]);
    let src = "#A Marker\n--\n#B\nMarker\nChildren [\n    #C Marker\n]\n";
    let w = ok(src, &[Op::Reparent { path: p("A"), parent: p("B") }]);
    assert_eq!(w.text, "#B\nMarker\nChildren [\n    #C Marker\n    --\n    #A Marker\n]\n");
    let back = ok(&w.text, &w.inverse);
    assert_eq!(back.text, "#B\nMarker\nChildren [\n    #C Marker\n]\n--\n#A Marker\n");
}

#[test]
fn setting_props() {
    roundtrip(ARENA, &[Op::SetProp { path: p("EnemyB"), prop: "hp".into(), value: f(400.0), field: false }], &["-@game::Enemy { @hp: 250.0 }", "+@game::Enemy { @hp: 400.0 }"]);
    let w = ok(ARENA, &[Op::SetProp { path: p("EnemyB"), prop: "team".into(), value: WValue::Int(2), field: true }]);
    assert_eq!(diff(ARENA, &w.text), ["-@game::Enemy { @hp: 250.0 }", "+@game::Enemy { @hp: 250.0, team: 2 }"]);
    let w = ok(ARENA, &[Op::SetProp { path: p("EnemyA"), prop: "hp".into(), value: f(1.0), field: false }]);
    assert_eq!(diff(ARENA, &w.text), ["-@game::Enemy", "+@game::Enemy { @hp: 1.0 }"]);
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
fn a_prefab_internal_entity_cannot_be_deleted() {
    let e = apply(ARENA, &[Op::Delete { path: p("EnemyB/eye") }]).unwrap_err();
    assert_eq!(e.reason, "prefab-internal");
}
