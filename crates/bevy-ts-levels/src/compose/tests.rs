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
            }),
            "Lamp" | "Drone" => Ok(SceneSchema {
                component: ComponentSchema { name: path.into(), kind: ComponentKind::Ts, fields: vec![], is_name: false },
                props: vec![],
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
    fn expand(&mut self, requests: &[ExpandRequest]) -> Result<Vec<NodeDesc>, Vec<ScriptError>> {
        self.calls.push(requests.to_vec());
        Ok(requests
            .iter()
            .map(|r| match r.component.as_str() {
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
                    root
                }
                "Lamp" => entity("lamp", vec![comp("Light", ComponentKind::Ts, json!({ "intensity": { "$b": 7 } }))], vec![]),
                _ => entity("drone", vec![comp("Transform", ComponentKind::Rust, json!({ "translation": { "$b": 8, "$s": 1 } }))], vec![]),
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
    let r = compose(list[0].0, &mut fs, &TestSchema, &mut ex);
    (r, ex)
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
    assert_eq!(files, ["levels/arena.bsn", "levels/rooms/room.bsn", "levels/rooms/room.bsn"]);
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
