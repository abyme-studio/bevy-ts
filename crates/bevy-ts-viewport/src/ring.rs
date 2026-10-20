//! The render world's half: keep a ring per viewport window for its size, pick a free slot, feed it
//! to Bevy as the window's swap chain, and announce it once the GPU has finished it.

use crate::shared::{Shared, SharedRes, Target};
use crate::surface::{self, FORMAT, SLOTS, Slot, Surface};
use bevy::prelude::*;
use bevy::render::camera::extract_cameras;
use bevy::render::render_resource::TextureView;
use bevy::render::renderer::{RenderDevice, RenderQueue};
use bevy::render::sync_world::RenderEntity;
use bevy::render::view::ExtractedWindow;
use bevy::render::{ExtractSchedule, MainWorld, Render, RenderApp, RenderSystems};
use bevy::window::PresentMode;
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::mpsc::{Sender, channel};

/// Counts main-world updates, so a frame knows which update it shows.
#[derive(Resource, Default, Clone, Copy)]
pub struct UpdateCount(pub u64);

/// Marks the primary window that stands for the viewport: no native handle, its swap chain the
/// ring.
#[derive(Component, Debug, Default, Clone, Copy)]
pub struct VirtualWindow;

/// A window that stands for another viewport (a preview's): its swap chain that viewport's ring.
/// It is never primary, and nothing writes window input to it.
#[derive(Component, Clone)]
pub struct ViewportWindow(pub Arc<Shared>);

/// A ring: three surfaces of one allocation, and textures over their top-left `w` by `h`.
struct Ring {
    number: u32,
    surfaces: Vec<Surface>,
    /// (w, h) the slots were wrapped at, and the slots.
    slots: Option<((u32, u32), Vec<Slot>)>,
}

impl Ring {
    fn alloc(&self) -> (u32, u32) {
        (self.surfaces[0].width(), self.surfaces[0].height())
    }
}

/// The render world's ring state, per viewport window (by main-world entity).
#[derive(Resource, Default)]
struct Rings(HashMap<Entity, RingState>);

#[derive(Default)]
struct RingState {
    current: Option<Ring>,
    /// The previous ring, released once the current ring's first frame is announced.
    old: Option<Ring>,
    next_number: u32,
    /// The slot rendered this frame: (ring, slot, w, h, seq, main-world update).
    this_frame: Option<(u32, usize, u32, u32, u64, u64)>,
    seq: u64,
    /// The viewport the ring belongs to.
    shared: Option<Arc<Shared>>,
}

/// Wakes the thread that polls the device, so `on_submitted_work_done` fires as soon as the GPU
/// finishes rather than at the next submit.
#[derive(Resource)]
struct Poller(Sender<()>);

pub(crate) fn build_render(app: &mut App, shared: Arc<Shared>) {
    let Some(render_app) = app.get_sub_app_mut(RenderApp) else { return };
    render_app
        .insert_resource(SharedRes(shared))
        .init_resource::<Rings>()
        .add_systems(ExtractSchedule, prepare_slot.before(extract_cameras))
        .add_systems(Render, announce.in_set(RenderSystems::Cleanup))
        .add_systems(Render, register_render_thread);
}

pub(crate) fn finish_render(app: &mut App) {
    let Some(render_app) = app.get_sub_app_mut(RenderApp) else { return };
    let device = render_app.world().resource::<RenderDevice>().wgpu_device().clone();
    let (tx, rx) = channel::<()>();
    std::thread::Builder::new()
        .name("bevy-ts-viewport-gpu".into())
        .spawn(move || {
            while rx.recv().is_ok() {
                while rx.try_recv().is_ok() {}
                let _ = device.poll(wgpu::PollType::wait_indefinitely());
            }
        })
        .expect("spawn the viewport GPU thread");
    render_app.insert_resource(Poller(tx));
}

/// Before cameras are extracted: give each viewport window an `ExtractedWindow` whose swap chain
/// is a free slot of a ring of the window's size, or none when nothing should render.
type ViewportWindowQuery = QueryState<(Entity, RenderEntity, &'static Window, Option<&'static ViewportWindow>), Or<(With<VirtualWindow>, With<ViewportWindow>)>>;

fn prepare_slot(world: &mut World, mut query: Local<Option<ViewportWindowQuery>>) {
    let game = world.resource::<SharedRes>().0.clone();
    let (found, update) = world.resource_scope(|_, mut main: Mut<MainWorld>| {
        let q = query.get_or_insert_with(|| main.query_filtered());
        let found: Vec<_> = q
            .iter(&main)
            .map(|(e, re, w, v)| (e, re, w.resolution.physical_width().max(1), w.resolution.physical_height().max(1), v.map_or_else(|| game.clone(), |v| v.0.clone())))
            .collect();
        (found, main.get_resource::<UpdateCount>().map_or(0, |u| u.0))
    });
    // A viewport window that is gone takes its ring with it.
    world.resource_mut::<Rings>().0.retain(|e, _| found.iter().any(|f| f.0 == *e));
    for (main, entity, w, h, shared) in found {
        let mut rings = world.resource_mut::<Rings>();
        let mut ring = std::mem::take(rings.0.entry(main).or_default());
        let device = world.resource::<RenderDevice>().clone();
        let slot_view = prepare_one(&mut ring, &shared, &device, w, h, update);
        ring.shared = Some(shared.clone());
        world.resource_mut::<Rings>().0.insert(main, ring);
        extract_window(world, entity, w, h, slot_view, &shared);
    }
}

fn prepare_one(ring: &mut RingState, shared: &Shared, device: &RenderDevice, w: u32, h: u32, update: u64) -> Option<TextureView> {
    ring.this_frame = None;
    let mut slot_view = None;
    if shared.should_render() {
        // A ring of the rounded size.
        let want = surface::round_up(w, h);
        if ring.current.as_ref().is_none_or(|r| r.alloc() != want) {
            let surfaces: Option<Vec<_>> = (0..SLOTS).map(|_| Surface::new(want.0, want.1)).collect();
            if let Some(surfaces) = surfaces {
                let number = ring.next_number;
                ring.next_number += 1;
                if let Some(prev) = ring.current.take() {
                    // A ring that never showed a frame is not worth keeping.
                    if ring.old.is_none() {
                        ring.old = Some(prev);
                    }
                }
                ring.current = Some(Ring { number, surfaces, slots: None });
                shared.set_ring_info(Some((number, want.0, want.1)));
                shared.set_ring(number);
                shared.resend_surfaces();
            }
        }
        if let Some(current) = ring.current.as_mut() {
            if shared.take_resend() {
                for (i, s) in current.surfaces.iter().enumerate() {
                    shared.send_surface(current.number, i, s);
                }
            }
            if current.slots.as_ref().is_none_or(|(size, _)| *size != (w, h)) {
                let slots: Option<Vec<_>> = current.surfaces.iter().map(|s| surface::make_slot(device, s, w, h)).collect();
                current.slots = slots.map(|s| ((w, h), s));
            }
            if let Some((_, slots)) = &current.slots {
                let free = shared.pick_slot(current.number, |i| current.surfaces[i].in_use());
                match free {
                    Some(i) => {
                        ring.seq += 1;
                        ring.this_frame = Some((current.number, i, w, h, ring.seq, update));
                        slot_view = Some(slots[i].view.clone());
                        crate::trace::mark("slot", i as u64, update);
                    }
                    None => {
                        crate::trace::mark("skip", update, 0);
                        shared.stats.skipped()
                    }
                }
            }
        }
        // The old ring goes once a frame of a newer ring has been announced.
        if ring.old.is_some() && shared.announced_ring().is_some_and(|n| ring.current.as_ref().is_some_and(|c| c.number == n)) {
            ring.old = None;
        }
    } else if !shared.attached() && ring.current.is_some() {
        // Detached: let the surfaces go.
        ring.current = None;
        ring.old = None;
        shared.set_ring_info(None);
    }
    slot_view
}

fn extract_window(world: &mut World, entity: Entity, w: u32, h: u32, slot_view: Option<TextureView>, shared: &Shared) {
    let has_view = slot_view.is_some();
    let mut e = world.entity_mut(entity);
    match e.get_mut::<ExtractedWindow>() {
        Some(mut ew) => {
            ew.size_changed = ew.physical_width != w || ew.physical_height != h;
            ew.physical_width = w;
            ew.physical_height = h;
            ew.swap_chain_texture_view = slot_view;
        }
        None => {
            e.insert(ExtractedWindow {
                physical_width: w,
                physical_height: h,
                present_mode: PresentMode::Fifo,
                desired_maximum_frame_latency: None,
                swap_chain_texture_view: slot_view,
                swap_chain_texture: None,
                swap_chain_texture_format: Some(FORMAT),
                swap_chain_texture_view_format: Some(FORMAT),
                size_changed: false,
                present_mode_changed: false,
                alpha_mode: bevy::window::CompositeAlphaMode::Opaque,
                needs_initial_present: false,
            });
        }
    }
    shared.set_target(if has_view { Target::Rendering } else { Target::Idle });
}

/// Exclusive, so it runs on the render thread: adds that thread to the frame's threads.
fn register_render_thread(_: &mut World, mut done: Local<bool>) {
    if !std::mem::replace(&mut *done, true) {
        crate::qos::register_this_thread();
    }
}

/// After the frame's submit: announce each slot rendered once the GPU has completed it.
fn announce(rings: Res<Rings>, queue: Res<RenderQueue>, poller: Res<Poller>, device: Res<RenderDevice>, game: Res<SharedRes>, mut n: Local<u32>) {
    // The device's allocation, every 30 frames, for viewport_stats.
    *n += 1;
    if *n % 30 == 1
        && let Some(hal) = unsafe { device.wgpu_device().as_hal::<wgpu_hal::api::Metal>() }
    {
        use objc2_metal::MTLDevice;
        game.0.hub.gpu_allocated.store(hal.raw_device().currentAllocatedSize() as u64, std::sync::atomic::Ordering::Relaxed);
    }
    let mut any = false;
    for ring in rings.0.values() {
        let (Some((number, slot, w, h, seq, update)), Some(shared)) = (ring.this_frame, ring.shared.clone()) else { continue };
        shared.mark_pending(slot);
        crate::trace::mark("submit", seq, update);
        let submitted = std::time::Instant::now();
        queue.on_submitted_work_done(move || {
            shared.announce(number, slot, submitted, update, json!({ "ring": number, "slot": slot, "seq": seq, "w": w, "h": h }));
        });
        any = true;
    }
    if any {
        let _ = poller.0.send(());
    }
}
