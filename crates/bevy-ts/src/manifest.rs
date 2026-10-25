//! The generation manifest JS hands Rust once per generation, and the resolution Rust hands
//! back. JSON both ways; neither runs per frame.

use crate::backend::SourceLocation;
use crate::layout::{ComponentLayout, FieldType};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Editor hints for one field.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct FieldHints {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub step: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FieldDecl {
    pub name: String,
    #[serde(rename = "type")]
    pub ty: FieldType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<serde_json::Value>,
    #[serde(default)]
    pub hints: FieldHints,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ComponentDecl {
    /// The Rust-style path (`enemies::Grunt`), or the class's `static path`.
    pub name: String,
    pub fields: Vec<FieldDecl>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub location: Option<SourceLocation>,
    /// `module#export` for an exported class.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// A scene component's scene: its props, read from `static scene(props)`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scene: Option<SceneDecl>,
}

/// A scene component's `static scene(props)`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SceneDecl {
    pub props: Vec<PropDecl>,
}

/// One scene prop: its type from the props type, its default from the destructuring.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PropDecl {
    pub name: String,
    #[serde(rename = "type")]
    pub ty: PropType,
    /// A literal default the compile step read.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<serde_json::Value>,
    /// A default the compile step could not read as a literal; the scene function computes it.
    #[serde(default)]
    pub has_default: bool,
    /// `?` in the props type.
    #[serde(default)]
    pub optional: bool,
    /// The prop in the scene's destructuring (`hp = 100`), where Apply writes its default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub location: Option<SourceLocation>,
}

/// A prop's type: a component field type, or a string (props only, never a column).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum PropType {
    String { kind: StringKind },
    Field(FieldType),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StringKind {
    String,
}

impl ComponentDecl {
    pub fn layout(&self) -> ComponentLayout {
        ComponentLayout::c_layout(self.fields.iter().map(|f| (f.name.as_str(), &f.ty)))
    }

    pub fn field(&self, name: &str) -> Option<&FieldDecl> {
        self.fields.iter().find(|f| f.name == name)
    }
}

/// A bevy-ts stage: one exclusive system in a Bevy main or fixed schedule, running that
/// schedule's TS systems.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Stage {
    First,
    PreUpdate,
    RunFixedMainLoop,
    Update,
    SpawnScene,
    PostUpdate,
    Last,
    FixedFirst,
    FixedPreUpdate,
    FixedUpdate,
    FixedPostUpdate,
    FixedLast,
}

impl Stage {
    pub const ALL: [Stage; 12] = [
        Stage::First,
        Stage::PreUpdate,
        Stage::RunFixedMainLoop,
        Stage::Update,
        Stage::SpawnScene,
        Stage::PostUpdate,
        Stage::Last,
        Stage::FixedFirst,
        Stage::FixedPreUpdate,
        Stage::FixedUpdate,
        Stage::FixedPostUpdate,
        Stage::FixedLast,
    ];

    /// The stages that exist from startup; the others are made on first use.
    pub const EAGER: [Stage; 4] = [Stage::PreUpdate, Stage::Update, Stage::PostUpdate, Stage::FixedUpdate];

    pub fn as_str(self) -> &'static str {
        match self {
            Stage::First => "first",
            Stage::PreUpdate => "preUpdate",
            Stage::RunFixedMainLoop => "runFixedMainLoop",
            Stage::Update => "update",
            Stage::SpawnScene => "spawnScene",
            Stage::PostUpdate => "postUpdate",
            Stage::Last => "last",
            Stage::FixedFirst => "fixedFirst",
            Stage::FixedPreUpdate => "fixedPreUpdate",
            Stage::FixedUpdate => "fixedUpdate",
            Stage::FixedPostUpdate => "fixedPostUpdate",
            Stage::FixedLast => "fixedLast",
        }
    }

    /// The stage's anchor name, `bevy-ts:<stage>` in kebab case.
    pub fn anchor(self) -> String {
        let mut out = String::from("bevy-ts:");
        for c in self.as_str().chars() {
            if c.is_ascii_uppercase() {
                out.push('-');
                out.push(c.to_ascii_lowercase());
            } else {
                out.push(c);
            }
        }
        out
    }

    pub fn eager(self) -> bool {
        Stage::EAGER.contains(&self)
    }
}

/// A component a system names: a TS component by declared name, or a Rust component by the
/// name `bevy-ts/bevy` exports (`Transform`) or by type path.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ComponentRef {
    pub name: String,
    /// `ts` for TS components, `rust` for Rust ones.
    pub source: ComponentSource,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ComponentSource {
    Ts,
    Rust,
}

/// What a query param fetches per row.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum DataDecl {
    Entity { entity: bool },
    Component {
        component: ComponentRef,
        #[serde(default)]
        mutable: bool,
        #[serde(default)]
        optional: bool,
    },
}

/// A query filter.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FilterDecl {
    With(ComponentRef),
    Without(ComponentRef),
    Changed(ComponentRef),
    Added(ComponentRef),
}

/// A lifecycle event an observer can take.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Lifecycle {
    Add,
    Insert,
    Remove,
}

/// One system parameter, as the compile step (or a setup closure) declared it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ParamDecl {
    Query {
        data: Vec<DataDecl>,
        #[serde(default)]
        filter: Vec<FilterDecl>,
    },
    Res {
        resource: String,
        /// `Option<Res<R>>`: absent gives `undefined` instead of failing the run.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        optional: bool,
    },
    ResMut {
        resource: String,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        optional: bool,
    },
    Local {
        fields: Vec<FieldDecl>,
    },
    Commands,
    MessageReader {
        message: String,
    },
    MessageWriter {
        message: String,
    },
    State {
        state: String,
    },
    NextState {
        state: String,
    },
    /// The value before the machine's last transition.
    PreviousState {
        state: String,
    },
    /// A one-shot system's input, `In<E>`: an event by name, first param only.
    In {
        event: String,
    },
    /// An observer's trigger: an event by name, or a lifecycle event of a component.
    On {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        event: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        lifecycle: Option<Lifecycle>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        component: Option<ComponentRef>,
    },
    /// A system param exported from Rust (`ReadRapierContext`), by its TS name.
    Port {
        name: String,
    },
    /// A param bundle: an object whose properties are params.
    Bundle {
        fields: Vec<BundleField>,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BundleField {
    pub name: String,
    pub param: ParamDecl,
}

/// Where a system runs.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ScheduleDecl {
    PreStartup,
    Startup,
    PostStartup,
    Stage { stage: Stage },
    /// A custom schedule: a TS `ScheduleLabel` by path (`game::Waves`) or one a crate exposed
    /// with `ts_schedule` (`net:tick`).
    Schedule { label: String },
    OnEnter { state: String, value: String },
    OnExit { state: String, value: String },
    /// Bevy's `OnTransition { exited, entered }`.
    OnTransition { state: String, exited: String, entered: String },
    Observer,
    /// A one-shot system, run by `commands.runSystem` or `RunTsSystem`.
    OneShot,
}

/// A component filter in `anyMatchFilter`, as a query filter.
pub type MatchFilter = FilterDecl;

/// A run condition, evaluated in Rust.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ConditionDecl {
    InState { state: String, value: String },
    /// Bevy's `state_changed`: the machine transitioned since this condition last ran.
    StateChanged { state: String },
    /// Bevy's `state_exists`: the machine has a value.
    StateExists { state: String },
    OnMessage { message: String },
    RunOnce,
    ResourceExists { resource: String },
    ResourceAdded { resource: String },
    ResourceChanged { resource: String },
    ResourceExistsAndChanged { resource: String },
    ResourceRemoved { resource: String },
    ResourceChangedOrRemoved { resource: String },
    /// Field values, encoded with the resource's layout and compared byte for byte.
    ResourceEquals { resource: String, value: serde_json::Map<String, serde_json::Value> },
    ResourceExistsAndEquals { resource: String, value: serde_json::Map<String, serde_json::Value> },
    AnyWithComponent { component: ComponentRef },
    AnyComponentRemoved { component: ComponentRef },
    AnyMatchFilter { filter: Vec<MatchFilter> },
    ConditionChanged { condition: Box<ConditionDecl> },
    ConditionChangedTo { condition: Box<ConditionDecl>, to: bool },
    Not { condition: Box<ConditionDecl> },
    All { conditions: Vec<ConditionDecl> },
    /// A combinator: `a.andThen(b)` and the others; `eager` evaluates `b` even when `a` decides.
    Combine { op: CombineOp, a: Box<ConditionDecl>, b: Box<ConditionDecl>, #[serde(default)] eager: bool },
    /// A condition a crate exposed with `ts_condition`, with its JSON arguments.
    Native {
        name: String,
        #[serde(default)]
        args: serde_json::Value,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        location: Option<SourceLocation>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CombineOp {
    And,
    Or,
    Nand,
    Nor,
    Xor,
    Xnor,
}

/// A group's collective `runIf`: evaluated once per run of a stage, before its first member.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GroupDecl {
    /// Stable across reloads: the first member's identity and an index.
    pub id: String,
    pub condition: ConditionDecl,
}

/// Where a custom schedule goes in Bevy's schedule orders.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScheduleOrderDecl {
    pub label: String,
    /// `main` (`MainScheduleOrder`), `fixed` (`FixedMainScheduleOrder`) or `startup`.
    pub order: ScheduleOrder,
    /// The schedule it goes after, or before.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub before: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub location: Option<SourceLocation>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ScheduleOrder {
    Main,
    Fixed,
    Startup,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SystemDecl {
    /// The system's identity, `<module path>#<export>`.
    pub name: String,
    pub schedule: ScheduleDecl,
    #[serde(default)]
    pub params: Vec<ParamDecl>,
    /// Systems (by identity) or sets this one runs after.
    #[serde(default)]
    pub after: Vec<String>,
    #[serde(default)]
    pub before: Vec<String>,
    #[serde(default)]
    pub sets: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub condition: Option<ConditionDecl>,
    /// The groups whose collective `runIf` gates it, outermost first.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub groups: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub location: Option<SourceLocation>,
}

impl SystemDecl {
    /// Every query param, bundles included, in order.
    pub fn queries(&self) -> Vec<(&Vec<DataDecl>, &Vec<FilterDecl>)> {
        fn walk<'a>(p: &'a ParamDecl, out: &mut Vec<(&'a Vec<DataDecl>, &'a Vec<FilterDecl>)>) {
            match p {
                ParamDecl::Query { data, filter } => out.push((data, filter)),
                ParamDecl::Bundle { fields } => fields.iter().for_each(|f| walk(&f.param, out)),
                _ => {}
            }
        }
        let mut out = Vec::new();
        self.params.iter().for_each(|p| walk(p, &mut out));
        out
    }

    /// Components the system reads and writes, as the union over its queries: a component is
    /// written when any query names it `Mut`.
    pub fn access(&self) -> (Vec<String>, Vec<String>) {
        let mut read = Vec::new();
        let mut write = Vec::new();
        for (data, _) in self.queries() {
            for d in data {
                if let DataDecl::Component { component, mutable, .. } = d {
                    let list = if *mutable { &mut write } else { &mut read };
                    if !list.contains(&component.name) {
                        list.push(component.name.clone());
                    }
                }
            }
        }
        read.retain(|r| !write.contains(r));
        (read, write)
    }
}

/// A TS resource: a layout, and the value `insertResource` gave, if any.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ResourceDecl {
    pub name: String,
    pub fields: Vec<FieldDecl>,
    /// Field values to insert with when the resource does not exist yet.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub insert: Option<serde_json::Map<String, serde_json::Value>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub location: Option<SourceLocation>,
}

/// A message or event type: a layout.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PayloadDecl {
    pub name: String,
    pub fields: Vec<FieldDecl>,
    /// Events only: an entity event, and whether it bubbles up `ChildOf`.
    #[serde(default)]
    pub entity: bool,
    #[serde(default)]
    pub propagate: bool,
    /// Messages only: `app.addMessage` was called for it.
    #[serde(default)]
    pub added: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub location: Option<SourceLocation>,
}

impl PayloadDecl {
    pub fn layout(&self) -> ComponentLayout {
        ComponentLayout::c_layout(self.fields.iter().map(|f| (f.name.as_str(), &f.ty)))
    }
}

/// A TS state machine.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StateDecl {
    pub name: String,
    pub values: Vec<String>,
    /// The value `initState` gave.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub initial: Option<String>,
    /// A sub state's source value (`SubStates(Game.Playing, ...)`): it exists only there.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<StateValueDecl>,
    /// A computed state's sources (`ComputedStates(Game, compute, ...)`), by name; JS computes it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub computed: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub location: Option<SourceLocation>,
}

/// A state value: its machine and value by name.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StateValueDecl {
    pub state: String,
    pub value: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EntryDecl {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub location: Option<SourceLocation>,
}

/// Declarations collected while a generation's modules evaluated.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Manifest {
    #[serde(default)]
    pub components: Vec<ComponentDecl>,
    #[serde(default)]
    pub systems: Vec<SystemDecl>,
    /// Observers attached to one entity by `commands.entity(e).observe(fn)` in an earlier
    /// generation, rebuilt from this generation's functions. They follow `systems` in key order.
    #[serde(default)]
    pub targeted: Vec<SystemDecl>,
    #[serde(default)]
    pub resources: Vec<ResourceDecl>,
    #[serde(default)]
    pub messages: Vec<PayloadDecl>,
    #[serde(default)]
    pub events: Vec<PayloadDecl>,
    #[serde(default)]
    pub states: Vec<StateDecl>,
    #[serde(default)]
    pub entries: Vec<EntryDecl>,
    /// Groups with a collective `runIf`.
    #[serde(default)]
    pub groups: Vec<GroupDecl>,
    /// Custom schedules `app.initSchedule` (or `addSystems`) creates.
    #[serde(default, rename = "initSchedules")]
    pub init_schedules: Vec<String>,
    /// `app.insertScheduleAfter` and its kin.
    #[serde(default, rename = "scheduleOrder")]
    pub schedule_order: Vec<ScheduleOrderDecl>,
    /// Command kinds the generation's code appends, by name, so Rust can check them up front.
    #[serde(default, rename = "commandKinds")]
    pub command_kinds: Vec<String>,
}

/// A plugin's manifest section: opaque bytes, and where in TS it was written.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Section {
    pub name: String,
    pub bytes: Vec<u8>,
    pub location: Option<SourceLocation>,
}

/// What [`crate::backend::ScriptBackend::load`] returns.
#[derive(Clone, Debug, Default)]
pub struct LoadedGeneration {
    pub manifest: Manifest,
    pub sections: Vec<Section>,
    /// Network requests the modules made while they evaluated, applied only once the
    /// generation commits.
    pub requests: Vec<crate::net::NetRequest>,
}

/// A component as resolved for JS: its id and layout.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ResolvedComponent {
    pub id: u32,
    pub layout: ComponentLayout,
    /// False for a Rust component whose bytes TS may not write as raw arrays (`q.tables`): it has
    /// holes, or `bool`, enum or `Entity` fields only accessors keep valid.
    #[serde(default = "yes", rename = "rawWrites", skip_serializing_if = "is_true")]
    pub raw_writes: bool,
    /// A Rust component's whole default value, which a script spawn or insert starts from, so
    /// fields it leaves out and holes keep their `Default`. Empty for TS components.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub defaults: Vec<u8>,
}

fn yes() -> bool {
    true
}

fn is_true(b: &bool) -> bool {
    *b
}

/// Rust's answer to a manifest: ids and layouts for every component the generation names, and
/// the opcode of every command kind.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Resolution {
    pub generation: u64,
    /// TS components by declared name, native ones by their `bevy-ts/bevy` name.
    pub components: BTreeMap<String, ResolvedComponent>,
    #[serde(rename = "commandKinds")]
    pub command_kinds: BTreeMap<String, u16>,
    /// Message and event types by name: their index in ops, and their layout.
    #[serde(default)]
    pub messages: BTreeMap<String, ResolvedPayload>,
    #[serde(default)]
    pub events: BTreeMap<String, ResolvedPayload>,
    /// State machines by name: their index in ops.
    #[serde(default)]
    pub states: BTreeMap<String, u32>,
    /// Exported Rust system params by TS name: the JS glue building each param's class.
    #[serde(default)]
    pub ports: BTreeMap<String, String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ResolvedPayload {
    pub index: u32,
    pub layout: ComponentLayout,
}
