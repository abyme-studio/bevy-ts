//! The render world's half: keep a ring for the virtual window's size, pick a free slot, feed it
//! to Bevy as the window's swap chain, and announce it once the GPU has finished it.

use crate::shared::{Shared, SharedRes, Target};
use crate::surface::{self, FORMAT, SLOTS, Slot, Surface};
use bevy::prelude::*;
use bevy::render::camera::extract_cameras;
use bevy::render::renderer::{RenderDevice, RenderQueue};
use bevy::render::sync_world::RenderEntity;
use bevy::render::view::ExtractedWindow;
use bevy::render::{ExtractSchedule, MainWorld, Render, RenderApp, RenderSystems};
use bevy::window::PresentMode;
use serde_json::json;
use std::sync::Arc;
use std::sync::mpsc::{Sender, channel};

/// Marks the primary window that stands for the viewport: no native handle, its swap chain the
/// ring.
#[derive(Component, Debug, Default, Clone, Copy)]
pub struct VirtualWindow;

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

/// The render world's ring state.
#[derive(Resource, Default)]
struct RingState {
    current: Option<Ring>,
    /// The previous ring, released once the current ring's first frame is announced.
    old: Option<Ring>,
    next_number: u32,
    /// The slot rendered this frame: (ring, slot, w, h, seq).
    this_frame: Option<(u32, usize, u32, u32, u64)>,
    seq: u64,
}

/// Wakes the thread that polls the device, so `on_submitted_work_done` fires as soon as the GPU
/// finishes rather than at the next submit.
#[derive(Resource)]
struct Poller(Sender<()>);

pub(crate) fn build_render(app: &mut App, shared: Arc<Shared>) {
    let Some(render_app) = app.get_sub_app_mut(RenderApp) else { return };
    render_app
        .insert_resource(SharedRes(shared))
        .init_resource::<RingState>()
        .add_systems(ExtractSchedule, prepare_slot.before(extract_cameras))
        .add_systems(Render, announce.in_set(RenderSystems::Cleanup));
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

/// Before cameras are extracted: give the virtual window an `ExtractedWindow` whose swap chain is
/// a free slot of a ring of the window's size, or none when nothing should render.
fn prepare_slot(world: &mut World, mut query: Local<Option<QueryState<(RenderEntity, &'static Window), With<VirtualWindow>>>>) {
    let shared = world.resource::<SharedRes>().0.clone();
    let found = world.resource_scope(|_, mut main: Mut<MainWorld>| {
        let q = query.get_or_insert_with(|| main.query_filtered());
        q.single(&main).ok().map(|(e, w)| (e, w.resolution.physical_width().max(1), w.resolution.physical_height().max(1)))
    });
    let Some((entity, w, h)) = found else { return };
    world.resource_mut::<RingState>().this_frame = None;

    let mut slot_view = None;
    if shared.should_render() {
        let device = world.resource::<RenderDevice>().clone();
        let mut ring = world.resource_mut::<RingState>();
        let ring = &mut *ring;
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
                let slots: Option<Vec<_>> = current.surfaces.iter().map(|s| surface::make_slot(&device, s, w, h)).collect();
                current.slots = slots.map(|s| ((w, h), s));
            }
            if let Some((_, slots)) = &current.slots {
                let free = shared.pick_slot(current.number, |i| current.surfaces[i].in_use());
                match free {
                    Some(i) => {
                        ring.seq += 1;
                        ring.this_frame = Some((current.number, i, w, h, ring.seq));
                        slot_view = Some(slots[i].view.clone());
                    }
                    None => shared.stats.skipped(),
                }
            }
        }
        // The old ring goes once a frame of a newer ring has been announced.
        if ring.old.is_some() && shared.announced_ring().is_some_and(|n| ring.current.as_ref().is_some_and(|c| c.number == n)) {
            ring.old = None;
        }
    } else {
        let mut ring = world.resource_mut::<RingState>();
        // Detached: let the surfaces go.
        if !shared.attached() {
            ring.current = None;
            ring.old = None;
        }
    }

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

/// After the frame's submit: announce the slot once the GPU has completed it.
fn announce(ring: Res<RingState>, queue: Res<RenderQueue>, shared: Res<SharedRes>, poller: Res<Poller>) {
    let Some((number, slot, w, h, seq)) = ring.this_frame else { return };
    let shared = shared.0.clone();
    shared.mark_pending(slot);
    queue.on_submitted_work_done(move || {
        shared.announce(number, slot, json!({ "ring": number, "slot": slot, "seq": seq, "w": w, "h": h }));
    });
    let _ = poller.0.send(());
}
