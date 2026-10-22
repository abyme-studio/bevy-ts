use super::*;
use std::collections::HashMap;

struct TestSchema;

fn f(name: &str, ty: FieldType, default: Value) -> FieldSchema {
    FieldSchema { name: name.into(), ty: ValueType::Field(ty), default }
}

impl Schema for TestSchema {
    fn component(&self, path: &str) -> Result<ComponentSchema, String> {
        let rust = |name: &str, fields| Ok(ComponentSchema { name: name.into(), kind: ComponentKind::Rust, fields, is_name: false });
        match path {
            "Transform" | "bevy_transform::components::transform::Transform" => rust(
                "Transform",
                vec![
                    f("translation", FieldType::Vec3, json!([0.0, 0.0, 0.0])),
                    f("rotation", FieldType::Quat, json!([0.0, 0.0, 0.0, 1.0])),
                    f("scale", FieldType::Vec3, json!([1.0, 1.0, 1.0])),
                ],
            ),
            "Name" => Ok(ComponentSchema { name: "Name".into(), kind: ComponentKind::Rust, fields: vec![], is_name: true }),
            "Health" | "enemies::Health" => Ok(ComponentSchema {
                name: "enemies::Health".into(),
                kind: ComponentKind::Ts,
                fields: vec![f("hp", FieldType::F32, json!(0.0)), f("max", FieldType::F32, json!(0.0))],
                is_name: false,
            }),
            "Light" => Ok(ComponentSchema { name: "Light".into(), kind: ComponentKind::Ts, fields: vec![f("intensity", FieldType::F32, json!(1.0))], is_name: false }),
            "Visibility" => rust("Visibility", vec![f("0", FieldType::Enum { variants: vec!["Inherited".into(), "Hidden".into(), "Visible".into()], repr: None }, json!("Inherited"))]),
            "Marker" => Err("Marker is ambiguous: a::Marker, b::Marker".into()),
            _ => Err(format!("unknown component {path}")),
        }
    }

    fn scene(&self, path: &str) -> Result<SceneSchema, String> {
        match path {
            "enemies::Grunt" | "Grunt" => Ok(SceneSchema {
                component: ComponentSchema { name: "enemies::Grunt".into(), kind: ComponentKind::Ts, fields: vec![f("team", FieldType::U8, json!(0))], is_name: false },
                props: vec![f("hp", FieldType::F32, json!(100.0))],
                required: vec![],
            }),
            "Lamp" | "Hat" => Ok(SceneSchema {
                component: ComponentSchema { name: path.into(), kind: ComponentKind::Ts, fields: vec![], is_name: false },
                props: vec![],
                required: vec![],
            }),
            "Drone" => Ok(SceneSchema {
                component: ComponentSchema { name: path.into(), kind: ComponentKind::Ts, fields: vec![], is_name: false },
                props: vec![f("throw", FieldType::Bool, json!(false))],
                required: vec![],
            }),
            "Turret" => Ok(SceneSchema {
                component: ComponentSchema { name: path.into(), kind: ComponentKind::Ts, fields: vec![], is_name: false },
                props: vec![f("range", FieldType::F32, json!(0.0))],
                required: vec!["range".into()],
            }),
            _ => Err(format!("unknown scene component {path}")),
        }
    }
}

#[derive(Default)]
struct TestExpander {
    calls: Vec<Vec<ExpandRequest>>,
}

fn comp(ty: &str, kind: ComponentKind, props: Value) -> ComponentDesc {
    let at = SourceLocation { file: "src/enemies.tsx".into(), line: 3, column: 5 };
    ComponentDesc { ty: ty.into(), kind, props: props.as_object().unwrap().clone(), source: Some(at), overrides: BTreeMap::new() }
}

fn entity(key: &str, components: Vec<ComponentDesc>, children: Vec<NodeDesc>) -> NodeDesc {
    NodeDesc {
        key: key.into(),
        ty: "entity".into(),
        name: None,
        ref_id: None,
        slot: None,
        on: BTreeMap::new(),
        source: vec![SourceLocation { file: "src/enemies.tsx".into(), line: 2, column: 3 }],
        components,
        children,
    }
}

impl Expander for TestExpander {
    fn expand(&mut self, requests: &[ExpandRequest]) -> Result<Vec<Expansion>, Vec<ScriptError>> {
        self.calls.push(requests.to_vec());
        Ok(requests
            .iter()
            .map(|r| match r.component.as_str() {
                "Drone" if r.props.get("throw").is_some() => Err(vec![ScriptError::at("Drone exploded", Some(SourceLocation { file: "src/drone.tsx".into(), line: 9, column: 7 }))]),
                "enemies::Grunt" => {
                    let hp = r.props.get("hp").cloned().unwrap_or(json!(100.0));
                    let mut root = entity(
                        "enemy",
                        vec![
                            comp("enemies::Health", ComponentKind::Ts, json!({ "initialHp": hp, "max": hp })),
                            comp("Transform", ComponentKind::Rust, json!({ "translation": [0.0, 0.0, -4.0] })),
                        ],
                        vec![entity("eye", vec![comp("Transform", ComponentKind::Rust, json!({ "translation": [0.0, 0.6, 0.5] }))], vec![])],
                    );
                    root.ty = "enemies::Grunt".into();
                    Ok(root)
                }
                "Lamp" => Ok(entity("lamp", vec![comp("Light", ComponentKind::Ts, json!({ "intensity": { "$b": 7 } }))], vec![])),
                _ => Ok(entity("drone", vec![comp("Transform", ComponentKind::Rust, json!({ "translation": { "$b": 8, "$s": 1 } }))], vec![])),
            })
            .collect())
    }
}

#[derive(Default)]
struct TestFiles(HashMap<String, String>);

impl Files for TestFiles {
    fn parsed(&mut self, path: &str) -> Result<Arc<ParsedFile>, String> {
        let src = self.0.get(path).ok_or("no such file")?;
        syntax::parse(src).map(Arc::new).map_err(|d| d.render(path, src))
    }
}

fn files(list: &[(&str, &str)]) -> TestFiles {
    TestFiles(list.iter().map(|(p, s)| (p.to_string(), s.to_string())).collect())
}

fn run(list: &[(&str, &str)]) -> (Result<Composed, Vec<ScriptError>>, TestExpander) {
    let mut fs = files(list);
    let mut ex = TestExpander::default();
    let r = compose(list[0].0, &mut fs, &TestSchema, &mut ex, false);
    (r, ex)
}

/// Composed under `bevy-ts dev`: missing code makes placeholders.
fn dev(list: &[(&str, &str)]) -> Composed {
    let mut fs = files(list);
    compose(list[0].0, &mut fs, &TestSchema, &mut TestExpander::default(), true).unwrap_or_else(|e| panic!("{e:?}"))
}

fn dev_err(list: &[(&str, &str)]) -> String {
    let mut fs = files(list);
    let e = compose(list[0].0, &mut fs, &TestSchema, &mut TestExpander::default(), true).unwrap_err().remove(0);
    format!("{} {}", e.location.map(|l| l.to_string()).unwrap_or_default(), e.message)
}

fn key(p: &[&str]) -> Vec<String> {
    p.iter().map(|s| s.to_string()).collect()
}

fn ok(list: &[(&str, &str)]) -> Composed {
    run(list).0.unwrap_or_else(|e| panic!("{e:?}"))
}

fn err(list: &[(&str, &str)]) -> String {
    let e = run(list).0.unwrap_err().remove(0);
    format!("{} {}", e.location.map(|l| l.to_string()).unwrap_or_default(), e.message)
}

fn props<'a>(n: &'a NodeDesc, ty: &str) -> &'a Map<String, Value> {
    &n.components.iter().find(|c| c.ty == ty).unwrap_or_else(|| panic!("{ty} on {}", n.key)).props
}

#[test]
fn layered_patches_over_an_instance() {
    let (r, ex) = run(&[(
        "levels/arena.bsn",
        "#EnemyB\n@enemies::Grunt { @hp: 250.0, team: 1 }\nTransform { translation: Vec3 { x: 3.0 } }\n",
    )]);
    let c = r.unwrap();
    assert_eq!(ex.calls.len(), 1);
    assert_eq!(ex.calls[0][0].props, json!({ "hp": 250.0, "team": 1 }).as_object().unwrap().clone());
    let b = &c.nodes[0];
    assert_eq!((b.key.as_str(), b.name.as_deref(), b.ty.as_str()), ("EnemyB", Some("EnemyB"), "enemies::Grunt"));
    let t = b.components.iter().find(|c| c.ty == "Transform").unwrap();
    assert_eq!(t.props["translation"], json!([3.0, 0.0, -4.0]));
    assert_eq!(t.overrides["translation"].to_string(), "levels/arena.bsn:3:13");
    // The level is the outermost call site.
    assert_eq!(b.source.last().unwrap().to_string(), "levels/arena.bsn:1:1");
}

#[test]
fn child_overrides_merge_by_name() {
    let c = ok(&[(
        "levels/arena.bsn",
        "#EnemyB @Grunt Children [\n  #eye Transform { translation: Vec3 { y: 0.8 } }\n  --\n  #hat Transform\n]",
    )]);
    let b = &c.nodes[0];
    assert_eq!(b.children.len(), 2);
    assert_eq!(props(&b.children[0], "Transform")["translation"], json!([0.0, 0.8, 0.5]));
    assert_eq!(b.children[1].key, "hat");
    assert!(c.owners.contains_key(&vec!["EnemyB".to_string(), "eye".to_string()]));
}

#[test]
fn overriding_a_prefab_initial_field_makes_it_controlled() {
    let c = ok(&[("a.bsn", "#E @Grunt Health { hp: 5.0 }")]);
    let h = props(&c.nodes[0], "enemies::Health");
    assert_eq!(h["hp"], json!(5.0));
    assert!(!h.contains_key("initialHp"));
}

#[test]
fn includes_resolve_and_own_their_frames() {
    let c = ok(&[
        ("levels/arena.bsn", "#RoomA :\"./rooms/room.bsn\"\n--\n#RoomB :\"levels/rooms/room.bsn\" Transform { translation: Vec3 { x: 20.0 } }"),
        ("levels/rooms/room.bsn", "#Room Transform Children [ #Floor Transform { scale: Vec3(10.0, 1.0, 10.0) } ]"),
    ]);
    assert_eq!(c.nodes.len(), 2);
    let a = &c.nodes[0];
    assert_eq!(a.name.as_deref(), Some("RoomA"));
    assert_eq!(props(&a.children[0], "Transform")["scale"], json!([10.0, 1.0, 10.0]));
    assert_eq!(props(&c.nodes[1], "Transform")["translation"], json!([20.0, 0.0, 0.0]));
    let floor = &c.owners[&vec!["RoomA".to_string(), "Floor".to_string()]];
    let files: Vec<&str> = floor.iter().map(|f| f.file.as_str()).collect();
    assert_eq!(files, ["levels/arena.bsn", "levels/rooms/room.bsn"]);
    // Each frame holds the node's path in its own file.
    assert_eq!(floor[0].path, ["RoomA", "Floor"]);
    assert_eq!(floor[1].path, ["Room", "Floor"]);
    assert_eq!(c.files.len(), 2);
}

#[test]
fn a_multi_entity_include_is_a_group() {
    let c = ok(&[("a.bsn", "#G :\"b.bsn\""), ("b.bsn", "#One Transform -- #Two Transform")]);
    let keys: Vec<&str> = c.nodes[0].children.iter().map(|n| n.key.as_str()).collect();
    assert_eq!(keys, ["One", "Two"]);
}

#[test]
fn inclusion_cycles_fail() {
    assert!(err(&[("a.bsn", "#X :\"b.bsn\""), ("b.bsn", "#Y :\"a.bsn\"")]).contains("cycle: a.bsn -> b.bsn -> a.bsn"));
}

#[test]
fn identity_rules() {
    assert!(err(&[("a.bsn", "#A Transform -- #A Transform")]).contains("two siblings are named \"A\""));
    let c = ok(&[("a.bsn", "Transform -- #B Transform -- Transform")]);
    let keys: Vec<&str> = c.nodes.iter().map(|n| n.key.as_str()).collect();
    assert_eq!(keys, ["#0", "B", "#1"]);
    assert_eq!(c.warnings.len(), 2);
    assert!(c.warnings[0].message.contains("unnamed entity"));
}

#[test]
fn a_bound_field_cannot_be_overridden() {
    let e = err(&[("levels/a.bsn", "#L @Lamp\nLight { intensity: 3.0 }")]);
    assert!(e.starts_with("levels/a.bsn:2:9 Light.intensity is bound in TSX at src/enemies.tsx:3:5"), "{e}");
}

#[test]
fn typing_errors_name_their_place() {
    assert_eq!(err(&[("a.bsn", "#A\nTransform { position: Vec3::ZERO }")]), "a.bsn:2:13 Transform has no field position (fields: translation, rotation, scale)");
    assert!(err(&[("a.bsn", "#A Transform { translation: true }")]).contains("expected a Vec3"));
    assert!(err(&[("a.bsn", "#A Marker")]).contains("a::Marker, b::Marker"));
    assert!(err(&[("a.bsn", "#A Visibility(Shown)")]).contains("unknown variant Shown (variants: Inherited, Hidden, Visible)"));
    assert!(err(&[("a.bsn", "#A @Grunt { @speed: 1.0 }")]).contains("has no scene prop speed"));
    assert!(err(&[("a.bsn", "#A @Grunt { team: 300 }")]).contains("team"));
}

#[test]
fn constants_tuples_and_names() {
    let c = ok(&[("a.bsn", "#A Transform { rotation: Quat::IDENTITY, scale: Vec3::ONE } Visibility(Hidden) Name(\"Alpha\")")]);
    let t = props(&c.nodes[0], "Transform");
    assert_eq!(t["rotation"], json!([0.0, 0.0, 0.0, 1.0]));
    assert_eq!(props(&c.nodes[0], "Visibility")["0"], json!("Hidden"));
    assert_eq!(c.nodes[0].name.as_deref(), Some("Alpha"));
}

#[test]
fn every_instance_expands_in_one_call() {
    let src: String = (0..50).map(|i| format!("#E{i} @Grunt\n")).collect::<Vec<_>>().join("--\n");
    let (r, ex) = run(&[("a.bsn", &src)]);
    assert_eq!(r.unwrap().nodes.len(), 50);
    assert_eq!(ex.calls.len(), 1);
    assert_eq!(ex.calls[0].len(), 50);
}

#[test]
fn relative_paths() {
    assert_eq!(resolve_path("levels/rooms/a.bsn", "./crate.glb#Mesh0"), "levels/rooms/crate.glb#Mesh0");
    assert_eq!(resolve_path("levels/rooms/a.bsn", "../b.bsn"), "levels/b.bsn");
    assert_eq!(resolve_path("levels/rooms/a.bsn", "models/x.glb"), "models/x.glb");
}

#[test]
fn a_bound_root_transform_cannot_be_placed() {
    let e = err(&[("a.bsn", "#D @Drone\nTransform { translation: Vec3 { x: 1.0 } }")]);
    assert!(e.starts_with("a.bsn:2:13 Transform.translation is bound in TSX"), "{e}");
}

// level-placeholders: missing code under `bevy-ts dev`.

#[test]
fn an_unknown_scene_component_is_a_placeholder_with_its_saved_text() {
    let c = dev(&[("levels/arena.bsn", "#Boss\n@enemies::Boss { @hp: 900.0 }\nTransform { translation: Vec3 { x: 2.0 } }\n--\n#E @Grunt")]);
    assert_eq!(c.nodes.len(), 2, "the other entities load as before");
    let boss = &c.nodes[0];
    assert_eq!(boss.ty, "entity");
    assert_eq!(props(boss, "Transform")["translation"], json!([2.0, 0.0, 0.0]));
    let m = &c.missing[&key(&["Boss"])];
    assert_eq!(m.len(), 1);
    assert_eq!((m[0].component.as_str(), m[0].reason), ("enemies::Boss", MissingReason::Unregistered));
    assert_eq!(m[0].fields, BTreeMap::from([("@hp".to_string(), "900.0".to_string())]));
    assert_eq!(m[0].location.location().to_string(), "levels/arena.bsn:2:1");
    assert!(m[0].scene);
    assert!(!c.missing.contains_key(&key(&["E"])));
}

#[test]
fn an_unknown_patch_is_a_placeholder_and_the_rest_applies() {
    let c = dev(&[("a.bsn", "#Crate\nTransform { translation: Vec3 { y: 1.0 } }\nLootz { table: \"chest_rare\" }")]);
    assert_eq!(props(&c.nodes[0], "Transform")["translation"], json!([0.0, 1.0, 0.0]));
    assert!(c.nodes[0].components.iter().all(|c| c.ty != "Lootz"));
    let m = &c.missing[&key(&["Crate"])][0];
    assert_eq!((m.component.as_str(), m.reason, m.message.as_str()), ("Lootz", MissingReason::Unregistered, "unknown component Lootz"));
    assert_eq!(m.fields["table"], "\"chest_rare\"");
    assert!(!m.scene);
}

#[test]
fn a_mismatch_applies_what_fits() {
    let c = dev(&[("a.bsn", "#E\nHealth { max: 400.0, hp: 400.0, cap: 3.0, }\nTransform { translation: true }")]);
    let h = props(&c.nodes[0], "enemies::Health");
    assert_eq!((h["hp"].clone(), h["max"].clone()), (json!(400.0), json!(400.0)));
    let m = &c.missing[&key(&["E"])];
    assert_eq!(m.len(), 2);
    assert_eq!((m[0].component.as_str(), m[0].reason), ("Health", MissingReason::Mismatch));
    assert_eq!(m[0].fields, BTreeMap::from([("cap".to_string(), "3.0".to_string())]));
    assert!(m[0].misfit["cap"].starts_with("enemies::Health has no field cap"), "{:?}", m[0].misfit);
    // A wrongly typed field: the component stays, with that field at its default.
    assert!(c.nodes[0].components.iter().any(|c| c.ty == "Transform"));
    assert_eq!(m[1].misfit["translation"], "expected a Vec3");
}

#[test]
fn a_misfit_prop_expands_without_it() {
    let mut fs = files(&[("a.bsn", "#E @Grunt { @hp: \"lots\", @speed: 2.0 }")]);
    let mut ex = TestExpander::default();
    let c = compose("a.bsn", &mut fs, &TestSchema, &mut ex, true).unwrap();
    assert!(ex.calls[0][0].props.is_empty(), "the misfits are left out");
    assert_eq!(c.nodes[0].ty, "enemies::Grunt");
    let m = &c.missing[&key(&["E"])][0];
    assert_eq!(m.reason, MissingReason::Mismatch);
    assert_eq!(m.fields, BTreeMap::from([("@hp".to_string(), "\"lots\"".to_string()), ("@speed".to_string(), "2.0".to_string())]));
    assert!(m.misfit["@speed"].contains("has no scene prop speed"));
    assert!(!m.scene);
}

#[test]
fn an_unset_required_prop_makes_the_instance_a_placeholder() {
    let c = dev(&[("a.bsn", "#T @Turret { @rnage: 3.0 }")]);
    assert_eq!(c.nodes[0].ty, "entity");
    let m = &c.missing[&key(&["T"])];
    assert_eq!(m.len(), 1, "one entry: the instance");
    assert_eq!((m[0].reason, m[0].message.as_str()), (MissingReason::Mismatch, "Turret needs @range"));
    assert_eq!(m[0].fields["@rnage"], "3.0");
    assert!(m[0].scene);
}

#[test]
fn a_missing_scene_instance_keeps_its_inner_overrides() {
    let c = dev(&[("a.bsn", "#Boss @enemies::Boss Children [\n  #eye Transform { translation: Vec3 { y: 0.8 } }\n  --\n  #hat @Hat\n]")]);
    let boss = &c.nodes[0];
    let keys: Vec<&str> = boss.children.iter().map(|n| n.key.as_str()).collect();
    assert_eq!(keys, ["hat"], "the eye is kept, the hat spawns");
    let m = &c.missing[&key(&["Boss"])][0];
    assert_eq!(m.overrides, vec![KeptOverride { path: key(&["eye"]), fields: BTreeMap::from([("Transform.translation".to_string(), "Vec3 { y: 0.8 }".to_string())]) }]);
}

#[test]
fn a_scene_that_throws_fails_its_instance_only() {
    let c = dev(&[("a.bsn", "#A @Drone\n--\n#B @Drone { @throw: true } Transform { translation: Vec3 { x: 1.0 } }")]);
    assert_eq!(c.nodes[0].key, "A");
    assert!(c.nodes[0].components.iter().any(|c| c.ty == "Transform"), "A expands");
    let m = &c.missing[&key(&["B"])][0];
    assert_eq!((m.component.as_str(), m.reason, m.message.as_str()), ("Drone", MissingReason::Failed, "Drone exploded"));
    assert_eq!(m.expected.as_ref().map(|e| e.location().to_string()).as_deref(), Some("src/drone.tsx:9:7"));
    assert_eq!(m.fields["@throw"], "true");
    assert_eq!(props(&c.nodes[1], "Transform")["translation"], json!([1.0, 0.0, 0.0]), "B spawns with what resolved");
}

#[test]
fn broken_files_stay_errors() {
    assert!(dev_err(&[("a.bsn", "#X :\"b.bsn\" Lootz"), ("b.bsn", "#Y :\"a.bsn\"")]).contains("cycle"));
    assert!(dev_err(&[("a.bsn", "#A Lootz -- #A Transform")]).contains("two siblings are named \"A\""));
    assert!(dev_err(&[("a.bsn", "#A :\"nope.bsn\"")]).contains("nope.bsn"));
    assert!(dev_err(&[("levels/a.bsn", "#L @Lamp\nLight { intensity: 3.0 }")]).contains("is bound in TSX"));
}

#[test]
fn strict_mode_fails_as_before() {
    assert!(err(&[("a.bsn", "#A Lootz")]).contains("unknown component Lootz"));
    assert!(err(&[("a.bsn", "#T @Turret")]).contains("Turret needs @range"));
    let mut fs = files(&[("a.bsn", "#B @Drone { @throw: true }")]);
    let e = compose("a.bsn", &mut fs, &TestSchema, &mut TestExpander::default(), false).unwrap_err();
    assert_eq!(e[0].message, "Drone exploded");
}
