//! `MaterialAlpha` (openspec changes `material-alpha`, `material-fade`): fades what an entity
//! draws. Its targets are the entity's own `MeshMaterial3d<StandardMaterial>` and, on a glTF scene
//! root, the meshes its scene spawned. Originals are shared (every red cube, every instance of a
//! model), so each target draws with a [`FadeMaterial`] copy owned by the entity, one per distinct
//! original, edited in place as `alpha` changes. Removing it puts the originals back and drops the
//! copies. This is the one place that knows a faded mesh's original ([`standard_of`]).

use bevy::app::AnimationSystems;
use bevy::asset::{AssetEventSystems, uuid_handle};
use bevy::ecs::entity::EntityHashSet;
use bevy::mesh::MeshVertexBufferLayoutRef;
use bevy::pbr::{ExtendedMaterial, MaterialExtension, MaterialExtensionKey, MaterialExtensionPipeline};
use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, FragmentState, RenderPipelineDescriptor, SpecializedMeshPipelineError};
use bevy::shader::ShaderRef;
use bevy::world_serialization::WorldInstanceReady;

use crate::scene::{SceneMaterialSet, scene_entities};

/// Fades the entity's materials: each target draws with a [`FadeMaterial`] copy that keeps the
/// original's alpha mode and draws about `alpha` (clamped to 0..1) of its pixels, picked by a 4×4
/// ordered dither, so alpha steps in sixteenths and the pattern shows. Depth stays written, so a
/// model fades as one solid; its shadow loses the same pixels. A `Blend`, `Premultiplied`, `Add`
/// or `Multiply` original is faded through its `base_color` alpha instead, and only its shadow
/// dithers. Remove it when not fading: while it is on, the entity's meshes no longer batch with
/// others sharing their material.
#[derive(Component, Reflect, Clone, Copy, Debug, PartialEq)]
#[reflect(Component, Default)]
pub struct MaterialAlpha {
    pub alpha: f32,
}

impl Default for MaterialAlpha {
    fn default() -> Self {
        Self { alpha: 1.0 }
    }
}

/// What a faded target draws with: its original as `base`, and the dither.
pub type FadeMaterial = ExtendedMaterial<StandardMaterial, Fade>;

/// The dither of a [`FadeMaterial`]: about `alpha` of the pixels draw, in the main pass (unless the
/// base is a blend mode) and in the prepass the shadow pass runs. `src/fade.wesl`.
#[derive(Asset, AsBindGroup, Reflect, Debug, Clone, Copy, PartialEq, Default)]
pub struct Fade {
    #[uniform(100)]
    pub alpha: f32,
}

const SHADER: Handle<Shader> = uuid_handle!("245ee06b-5b02-400a-9e77-28ea1d135a80");

impl MaterialExtension for Fade {
    fn fragment_shader() -> ShaderRef {
        SHADER.into()
    }

    fn prepass_fragment_shader() -> ShaderRef {
        SHADER.into()
    }

    /// Bevy gives an opaque base's depth-only prepass and shadow pipelines no fragment stage, so
    /// nothing would discard there: give them the dither's.
    fn specialize(
        _: &MaterialExtensionPipeline,
        descriptor: &mut RenderPipelineDescriptor,
        _: &MeshVertexBufferLayoutRef,
        _: MaterialExtensionKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        if descriptor.fragment.is_none() {
            descriptor.fragment = Some(FragmentState { shader: SHADER, shader_defs: descriptor.vertex.shader_defs.clone(), ..default() });
        }
        Ok(())
    }
}

type Id = AssetId<StandardMaterial>;
type CopyId = AssetId<FadeMaterial>;

/// What a faded entity owns: its copies, and the meshes it faded at the last sync.
#[derive(Component, Debug, Default)]
pub struct MaterialAlphaTargets {
    /// The meshes drawing one of the copies at the last sync.
    pub targets: Vec<Entity>,
    /// By original: the original and its copy.
    copies: HashMap<Id, (Handle<StandardMaterial>, Handle<FadeMaterial>)>,
    /// By copy: its original.
    originals: HashMap<CopyId, Id>,
}

impl MaterialAlphaTargets {
    /// The copies the entity owns.
    pub fn copies(&self) -> impl Iterator<Item = &Handle<FadeMaterial>> {
        self.copies.values().map(|(_, c)| c)
    }

    /// The original a copy was made from; `None` for a handle that is not one of its copies.
    pub fn original(&self, copy: CopyId) -> Option<&Handle<StandardMaterial>> {
        self.originals.get(&copy).map(|o| &self.copies[o].0)
    }
}

/// The `StandardMaterial` `mesh` draws, or the original of the fade copy it draws instead; `None`
/// for neither (a script material, no material). Its owner is `mesh` or an ancestor.
pub fn standard_of(world: &World, mesh: Entity) -> Option<Handle<StandardMaterial>> {
    let e = world.get_entity(mesh).ok()?;
    if let Some(m) = e.get::<MeshMaterial3d<StandardMaterial>>() {
        return Some(m.0.clone());
    }
    let copy = e.get::<MeshMaterial3d<FadeMaterial>>()?.0.id();
    let mut at = Some(mesh);
    while let Some(a) = at {
        if let Some(o) = world.get::<MaterialAlphaTargets>(a).and_then(|t| t.original(copy)) {
            return Some(o.clone());
        }
        at = world.get::<ChildOf>(a).map(ChildOf::parent);
    }
    None
}

/// Faded roots whose scene became ready since the last `PostUpdate`.
#[derive(Resource, Default)]
struct ReadyRoots(EntityHashSet);

pub(crate) fn plugin(app: &mut App) {
    let shader = Shader::from_wesl(include_str!("fade.wesl"), "embedded://bevy_ts_material/fade.wesl");
    app.world_mut().resource_mut::<Assets<Shader>>().insert(SHADER.id(), shader).expect("a uuid handle");
    app.add_plugins(MaterialPlugin::<FadeMaterial>::default());
    app.register_type::<MaterialAlpha>().init_resource::<ReadyRoots>().add_observer(on_ready).add_systems(
        PostUpdate,
        // After scene-material, so its swaps and restores are applied before this reads the meshes;
        // after animation, so an animated `alpha` reaches the copies the frame it is written.
        changed.after(AssetEventSystems).after(SceneMaterialSet).after(AnimationSystems).run_if(|q: Query<(), Faded>| !q.is_empty()),
    );
}

/// A faded root's scene is ready (spawned, or spawned again after a reload): fade its meshes.
fn on_ready(ready: On<WorldInstanceReady>, faded: Query<(), With<MaterialAlpha>>, mut roots: ResMut<ReadyRoots>) {
    if faded.contains(ready.entity) {
        roots.0.insert(ready.entity);
    }
}

/// Faded, or faded until `MaterialAlpha` was removed: the system has work only with one.
type Faded = Or<(With<MaterialAlpha>, With<MaterialAlphaTargets>)>;

/// `MaterialAlpha` added or changed, or removed from an entity that still holds copies.
type Asks = Or<(Changed<MaterialAlpha>, (With<MaterialAlphaTargets>, Without<MaterialAlpha>))>;

/// The entities `entity` fades: itself, and the meshes of its scene instance (none before it is
/// ready), those that hold a `StandardMaterial` or a fade copy. TSX children are not in it; a
/// child takes its own `MaterialAlpha`. A mesh scene-material gave a script material holds neither.
fn targets(world: &World, entity: Entity) -> Vec<Entity> {
    let mut out = vec![entity];
    out.extend(scene_entities(world, entity));
    out.retain(|e| world.get::<MeshMaterial3d<StandardMaterial>>(*e).is_some() || world.get::<MeshMaterial3d<FadeMaterial>>(*e).is_some());
    out
}

/// What a copy of `original` draws with at `alpha`: its base colour, and its dither. A blend-mode
/// original fades through its colour's alpha; the dither then only reaches its shadow.
fn faded(original: &StandardMaterial, alpha: f32) -> (Color, Fade) {
    let color = match original.alpha_mode {
        AlphaMode::Blend | AlphaMode::Premultiplied | AlphaMode::Add | AlphaMode::Multiply => original.base_color.with_alpha(original.base_color.alpha() * alpha),
        _ => original.base_color,
    };
    (color, Fade { alpha })
}

fn copy_of(original: &StandardMaterial, alpha: f32) -> FadeMaterial {
    let (base_color, extension) = faded(original, alpha);
    FadeMaterial { base: StandardMaterial { base_color, ..original.clone() }, extension }
}

/// What asks for a sync: `MaterialAlpha` added, changed or removed, a scene ready, a mesh given a
/// material (its owner is the nearest ancestor with `MaterialAlpha`), an original reloaded.
fn changed(
    asks: Query<Entity, Asks>,
    mut ready: ResMut<ReadyRoots>,
    given: Query<Entity, Changed<MeshMaterial3d<StandardMaterial>>>,
    owners: Query<(Entity, Option<&MaterialAlphaTargets>), With<MaterialAlpha>>,
    parents: Query<&ChildOf>,
    mut events: MessageReader<AssetEvent<StandardMaterial>>,
    mut commands: Commands,
) {
    let mut reloaded: Vec<(Entity, Id)> = Vec::new();
    for ev in events.read() {
        if let AssetEvent::Modified { id } = ev {
            reloaded.extend(owners.iter().filter(|(_, t)| t.is_some_and(|t| t.copies.contains_key(id))).map(|(e, _)| (e, *id)));
        }
    }
    for (e, id) in reloaded {
        commands.queue(move |world: &mut World| recopy(world, e, id));
    }
    let given = given.iter().filter_map(|e| std::iter::once(e).chain(parents.iter_ancestors(e)).find(|a| owners.contains(*a)));
    let mut seen = EntityHashSet::default();
    for e in asks.iter().chain(ready.0.drain()).chain(given) {
        if seen.insert(e) {
            commands.queue(move |world: &mut World| sync(world, e));
        }
    }
}

/// An original changed (a glTF saved again): its copy is made again from it.
fn recopy(world: &mut World, entity: Entity, original: Id) {
    let (Some(alpha), Some(targets)) = (world.get::<MaterialAlpha>(entity), world.get::<MaterialAlphaTargets>(entity)) else { return };
    let alpha = alpha.alpha.clamp(0.0, 1.0);
    let Some((o, c)) = targets.copies.get(&original).cloned() else { return };
    let Some(new) = world.resource::<Assets<StandardMaterial>>().get(&o).map(|o| copy_of(o, alpha)) else { return };
    if let Some(mut c) = world.resource_mut::<Assets<FadeMaterial>>().get_mut(&c) {
        *c = new;
    }
}

/// Brings `entity`'s targets to what it asks: each draws with the entity's copy of its original,
/// at the entity's alpha; with no `MaterialAlpha`, each gets its original back and the copies go.
pub fn sync(world: &mut World, entity: Entity) {
    let Ok(e) = world.get_entity(entity) else { return };
    let Some(alpha) = e.get::<MaterialAlpha>().map(|m| m.alpha.clamp(0.0, 1.0)) else {
        restore(world, entity);
        return;
    };
    // Edited in place, not taken and inserted: a bound alpha syncs every frame.
    let mut state = match world.get_mut::<MaterialAlphaTargets>(entity) {
        Some(mut t) => std::mem::take(&mut *t),
        None => default(),
    };
    let targets = targets(world, entity);
    for &t in &targets {
        // A fade copy already: this entity's, or another's to leave alone.
        let Some(h) = world.get::<MeshMaterial3d<StandardMaterial>>(t).map(|m| m.0.clone()) else { continue };
        let copy = match state.copies.get(&h.id()) {
            Some((_, c)) => c.clone(),
            None => {
                let Some(m) = world.resource::<Assets<StandardMaterial>>().get(&h).map(|o| copy_of(o, alpha)) else { continue };
                let c = world.resource_mut::<Assets<FadeMaterial>>().add(m);
                state.originals.insert(c.id(), h.id());
                state.copies.insert(h.id(), (h, c.clone()));
                c
            }
        };
        world.entity_mut(t).remove::<MeshMaterial3d<StandardMaterial>>().insert(MeshMaterial3d(copy));
    }
    // A changed alpha: rewrite only the copies whose values differ, so an unchanged one is not
    // prepared again.
    let wants: Vec<_> = {
        let originals = world.resource::<Assets<StandardMaterial>>();
        state.copies.values().filter_map(|(o, c)| Some((c.id(), faded(originals.get(o)?, alpha)))).collect()
    };
    let mut copies = world.resource_mut::<Assets<FadeMaterial>>();
    for (c, want) in wants {
        if copies.get(c).is_some_and(|c| (c.base.base_color, c.extension) != want)
            && let Some(mut c) = copies.get_mut(c)
        {
            (c.base.base_color, c.extension) = want;
        }
    }
    state.targets = targets.into_iter().filter(|t| world.get::<MeshMaterial3d<FadeMaterial>>(*t).is_some_and(|m| state.originals.contains_key(&m.0.id()))).collect();
    match world.get_mut::<MaterialAlphaTargets>(entity) {
        Some(mut t) => *t = state,
        None => {
            world.entity_mut(entity).insert(state);
        }
    }
}

/// `MaterialAlpha` removed: every mesh still drawing one of the entity's copies draws its original
/// again, and dropping `MaterialAlphaTargets` frees the copies.
fn restore(world: &mut World, entity: Entity) {
    let Some(state) = world.entity_mut(entity).take::<MaterialAlphaTargets>() else { return };
    let mut candidates = targets(world, entity);
    candidates.extend(state.targets.iter().copied());
    for t in candidates {
        let Some(h) = world.get_entity(t).ok().and_then(|e| e.get::<MeshMaterial3d<FadeMaterial>>()) else { continue };
        if let Some(o) = state.original(h.0.id()).cloned() {
            world.entity_mut(t).remove::<MeshMaterial3d<FadeMaterial>>().insert(MeshMaterial3d(o));
        }
    }
}
