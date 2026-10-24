//! Every `GizmoBuffer` drawing method TS calls, written once (design 1): the [`entries!`] table
//! lists each Bevy method with its typed arguments and expands to its TS name, argument specs,
//! builder and the call itself, so the list cannot drift from the call. The same entries drive
//! `Gizmos<G>` for every group and, later, the retained replay.
//!
//! A method that returns a builder in Bevy draws at once with Bevy's defaults and returns the
//! param object, typed as the builder (design 5). A setting truncates the buffer back to where
//! the builder began and redraws with every setting so far; [`OpenBuilder`] remembers where.

use bevy::gizmos::arcs::{Arc2dBuilder, Arc3dBuilder};
use bevy::gizmos::arrows::ArrowBuilder;
use bevy::gizmos::circles::{Ellipse2dBuilder, EllipseBuilder, SphereBuilder};
use bevy::gizmos::config::GizmoConfigGroup;
use bevy::gizmos::gizmos::GizmoBuffer;
use bevy::gizmos::grid::{GridBuilder2d, GridBuilder3d};
use bevy::gizmos::primitives::dim2::{Annulus2dBuilder, Line2dBuilder, Segment2dBuilder};
use bevy::gizmos::primitives::dim3::{Capsule3dBuilder, Cone3dBuilder, ConicalFrustum3dBuilder, Cylinder3dBuilder, Plane3dBuilder, Torus3dBuilder};
use bevy::gizmos::rounded_box::{RoundedCuboidBuilder, RoundedRectBuilder};
use bevy::ecs::change_detection::Tick;
use bevy::prelude::*;
use bevy_ts::host::{self, ArgSpec, HostArg, HostList, HostPoints, HostTransform, HostValue, Kind, MethodSpec, ParamState, RunTicks, SlotReader, SlotWriter};
use bevy::ecs::world::unsafe_world_cell::UnsafeWorldCell;

use super::TsGizmos;
use super::primitives::{self, Curve2d, Curve3d, Primitive2d, Primitive3d, Registry};

/// Bevy's `Aabb3d` (`min`, `max`) as TS passes it to `aabb3d`.
#[derive(crate::HostRecord, Clone, Copy, Debug, Default)]
pub struct Aabb3d {
    pub min: Vec3,
    pub max: Vec3,
}

impl From<Aabb3d> for bevy::shape::Aabb3d {
    fn from(a: Aabb3d) -> Self {
        bevy::shape::Aabb3d { min: a.min.into(), max: a.max.into() }
    }
}

/// A `Vec2` setting that crosses as a `Vec3`'s three slots: `GridBuilder2d.skew` shares one JS
/// method with `GridBuilder3d.skew`, so both take the same slots; TS types it as a `Vec2`.
pub struct Xy(Vec2);

impl HostValue for Xy {
    fn kind() -> Kind {
        Kind::Vec2
    }
    fn read(r: &mut SlotReader) -> Result<Self, String> {
        Ok(Xy(Vec3::read(r)?.truncate()))
    }
    fn write(&self, w: &mut SlotWriter) {
        self.0.extend(0.0).write(w);
    }
}

impl From<Xy> for Vec2 {
    fn from(v: Xy) -> Self {
        v.0
    }
}

/// The four lengths of a buffer: list positions and colours, strip positions and colours.
pub fn lengths<C: GizmoConfigGroup>(b: &GizmoBuffer<C, ()>) -> [usize; 4] {
    [b.list_positions.len(), b.list_colors.len(), b.strip_positions.len(), b.strip_colors.len()]
}

fn truncate<C: GizmoConfigGroup>(b: &mut GizmoBuffer<C, ()>, to: [usize; 4]) {
    b.list_positions.truncate(to[0]);
    b.list_colors.truncate(to[1]);
    b.strip_positions.truncate(to[2]);
    b.strip_colors.truncate(to[3]);
}

/// The settings so far of the builder just drawn: each setting and its argument slots.
pub type Settings = [(Setting, Vec<u8>)];

/// A Bevy builder, given its settings in call order, then dropped, which draws it.
pub trait Settle: Sized {
    fn settle(self, settings: &Settings) -> Result<(), String>;

    /// As `settle`, for a primitive named `name` whose Bevy call may return no builder.
    fn settle_primitive(self, _name: &str, settings: &Settings) -> Result<(), String> {
        self.settle(settings)
    }
}

impl Settle for () {
    fn settle(self, _: &Settings) -> Result<(), String> {
        Ok(())
    }

    fn settle_primitive(self, name: &str, settings: &Settings) -> Result<(), String> {
        match settings.first() {
            Some((s, _)) => Err(format!("{name} has no setting {}", s.ts())),
            None => Ok(()),
        }
    }
}

/// A call that took its settings itself (a primitive or a curve).
impl Settle for Result<(), String> {
    fn settle(self, _: &Settings) -> Result<(), String> {
        self
    }
}

/// The builder just drawn, if a setting may still redraw it (design 5).
#[derive(Default)]
pub struct OpenBuilder {
    /// The entry that drew it.
    entry: Option<Entry>,
    /// Whether nothing was drawn since.
    live: bool,
    /// The run that drew it.
    run: Option<Tick>,
    start: [usize; 4],
    end: [usize; 4],
    /// The entry's argument slots, to redraw it.
    args: Vec<u8>,
    settings: Vec<(Setting, Vec<u8>)>,
}

macro_rules! settings {
    (@slots) => { 0 };
    (@slots $wire:ty) => { <$wire as HostValue>::kind().slots() };
    ($($id:ident($($arg:ident: $wire:ty)?)),* $(,)?) => {
        /// Every builder setting by its TS name. A setting's slots (its wire type) are the same on
        /// every builder that has it.
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
        #[allow(non_camel_case_types)]
        pub enum Setting { $($id),* }

        impl Setting {
            pub const ALL: &[Setting] = &[$(Setting::$id),*];

            pub fn ts(self) -> &'static str {
                match self { $(Setting::$id => stringify!($id)),* }
            }

            /// Slots its argument takes.
            fn slots(self) -> usize {
                match self { $(Setting::$id => settings!(@slots $($wire)?)),* }
            }
        }
    };
}

settings! {
    resolution(resolution: u32),
    withTipLength(length: f32),
    withDoubleEnd(),
    skewX(skew: f32),
    skewY(skew: f32),
    skewZ(skew: f32),
    skew(skew: Vec3),
    outerEdgesX(),
    outerEdgesY(),
    outerEdgesZ(),
    outerEdges(),
    cornerRadius(cornerRadius: f32),
    arcResolution(arcResolution: u32),
    edgeRadius(edgeRadius: f32),
    outerResolution(resolution: u32),
    innerResolution(resolution: u32),
    drawArrow(isEnabled: bool),
    cellCount(cellCount: UVec2),
    spacing(spacing: Vec2),
    baseResolution(resolution: u32),
    heightResolution(resolution: u32),
    minorResolution(minorResolution: u32),
    majorResolution(majorResolution: u32),
}

macro_rules! builders {
    ($($ty:ident $(as $ts:ident)? { $($s:ident => $m:ident($($arg:ident: $t:ty)?)),* $(,)? })*) => {
        $(impl<C: GizmoConfigGroup> Settle for $ty<'_, C, ()> {
            fn settle(mut self, settings: &Settings) -> Result<(), String> {
                for (s, bytes) in settings {
                    #[allow(unused_mut, unused_variables)]
                    let mut r = SlotReader::new(bytes);
                    self = match s {
                        $(Setting::$s => self.$m($(<$t as HostValue>::read(&mut r).map_err(|e| format!("{}: {e}", stringify!($arg)))?.into())?),)*
                        #[allow(unreachable_patterns)]
                        other => return Err(format!("{} has no setting {}", stringify!($ty), other.ts())),
                    };
                }
                Ok(())
            }
        })*

        /// Each builder: Bevy's name, the TS interface that declares its settings (its own name,
        /// or one for every primitive builder of a dimension), and its settings with their TS
        /// arguments, in declaration order. `GridBuilder3d` comes before `GridBuilder2d`: the
        /// first `skew` is the JS one.
        pub fn builders() -> Vec<(&'static str, &'static str, Vec<(Setting, Vec<ArgSpec>)>)> {
            vec![$((stringify!($ty), None$(.or(Some(stringify!($ts))))?.unwrap_or(stringify!($ty)), vec![$((Setting::$s, vec![$(<$t as HostArg>::spec(stringify!($arg)))?])),*])),*]
        }
    };
}

builders! {
    EllipseBuilder { resolution => resolution(resolution: u32) }
    Ellipse2dBuilder { resolution => resolution(resolution: u32) }
    SphereBuilder { resolution => resolution(resolution: u32) }
    Arc2dBuilder { resolution => resolution(resolution: u32) }
    Arc3dBuilder { resolution => resolution(resolution: u32) }
    ArrowBuilder { withTipLength => with_tip_length(length: f32), withDoubleEnd => with_double_end() }
    GridBuilder3d {
        skewX => skew_x(skew: f32),
        skewY => skew_y(skew: f32),
        skewZ => skew_z(skew: f32),
        skew => skew(skew: Vec3),
        outerEdgesX => outer_edges_x(),
        outerEdgesY => outer_edges_y(),
        outerEdgesZ => outer_edges_z(),
        outerEdges => outer_edges(),
    }
    GridBuilder2d {
        skewX => skew_x(skew: f32),
        skewY => skew_y(skew: f32),
        skew => skew(skew: Xy),
        outerEdgesX => outer_edges_x(),
        outerEdgesY => outer_edges_y(),
        outerEdges => outer_edges(),
    }
    RoundedRectBuilder { cornerRadius => corner_radius(cornerRadius: f32), arcResolution => arc_resolution(arcResolution: u32) }
    RoundedCuboidBuilder { edgeRadius => edge_radius(edgeRadius: f32), arcResolution => arc_resolution(arcResolution: u32) }
    Annulus2dBuilder as Primitive2dBuilder {
        resolution => resolution(resolution: u32),
        outerResolution => outer_resolution(resolution: u32),
        innerResolution => inner_resolution(resolution: u32),
    }
    Line2dBuilder as Primitive2dBuilder { drawArrow => draw_arrow(isEnabled: bool) }
    Segment2dBuilder as Primitive2dBuilder { drawArrow => draw_arrow(isEnabled: bool) }
    Plane3dBuilder as Primitive3dBuilder { cellCount => cell_count(cellCount: UVec2), spacing => spacing(spacing: Vec2) }
    Cylinder3dBuilder as Primitive3dBuilder { resolution => resolution(resolution: u32) }
    Capsule3dBuilder as Primitive3dBuilder { resolution => resolution(resolution: u32) }
    Cone3dBuilder as Primitive3dBuilder {
        resolution => resolution(resolution: u32),
        baseResolution => base_resolution(resolution: u32),
        heightResolution => height_resolution(resolution: u32),
    }
    ConicalFrustum3dBuilder as Primitive3dBuilder { resolution => resolution(resolution: u32) }
    Torus3dBuilder as Primitive3dBuilder { minorResolution => minor_resolution(minorResolution: u32), majorResolution => major_resolution(majorResolution: u32) }
}

macro_rules! entries {
    ($($id:ident $(as $builder:ident)? => |$g:ident $(@ $reg:ident, $set:ident)? $(, $arg:ident: $ty:ty)*| $call:expr;)*) => {
        /// Every drawing method by its TS name, in declaration order.
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        #[allow(non_camel_case_types)]
        pub enum Entry { $($id),* }

        impl Entry {
            pub const ALL: &[Entry] = &[$(Entry::$id),*];

            pub fn ts(self) -> &'static str {
                match self { $(Entry::$id => stringify!($id)),* }
            }

            /// The builder it returns, by its TS name.
            pub fn builder(self) -> Option<&'static str> {
                match self { $(Entry::$id => None $(.or(Some(stringify!($builder))))?),* }
            }

            pub fn args(self) -> Vec<ArgSpec> {
                match self { $(Entry::$id => vec![$(<$ty as HostArg>::spec(stringify!($arg))),*]),* }
            }

            /// Reads the arguments and makes the Bevy call, giving a builder its settings.
            /// `registry` reads primitives and curves.
            #[allow(non_snake_case)]
            pub fn draw<C: GizmoConfigGroup>(self, buf: &mut GizmoBuffer<C, ()>, registry: Registry, r: &mut SlotReader, slices: &[&[u8]], settings: &Settings) -> Result<(), String> {
                #[allow(unused_mut, unused_variables)]
                let mut slices = slices.iter().copied();
                match self {
                    $(Entry::$id => {
                        $(let $arg = <$ty as HostArg>::take(r, &mut slices, stringify!($arg))?;)*
                        let $g = &mut *buf;
                        $(let $reg = registry; let $set = settings;)?
                        Settle::settle($call, settings)
                    })*
                }
            }

            /// The host function of each entry, for a `Gizmos<C>`.
            fn call<C: GizmoConfigGroup>(self) -> host::HostFn {
                match self { $(Entry::$id => call_entry::<C, { Entry::$id as usize }>),* }
            }
        }
    };
}

entries! {
    line => |g, start: Vec3, end: Vec3, color: Color| g.line(start, end, color);
    lineGradient => |g, start: Vec3, end: Vec3, startColor: Color, endColor: Color| g.line_gradient(start, end, startColor, endColor);
    ray => |g, start: Vec3, vector: Vec3, color: Color| g.ray(start, vector, color);
    rayGradient => |g, start: Vec3, vector: Vec3, startColor: Color, endColor: Color| g.ray_gradient(start, vector, startColor, endColor);
    linestrip => |g, positions: HostPoints<Vec3>, color: Color| g.linestrip(positions.iter(), color);
    lineloop => |g, positions: HostPoints<Vec3>, color: Color| g.lineloop(positions.iter(), color);
    linestripGradient => |g, points: HostList<(Vec3, Color)>| g.linestrip_gradient(points.0);
    line2d => |g, start: Vec2, end: Vec2, color: Color| g.line_2d(start, end, color);
    lineGradient2d => |g, start: Vec2, end: Vec2, startColor: Color, endColor: Color| g.line_gradient_2d(start, end, startColor, endColor);
    linestrip2d => |g, positions: HostPoints<Vec2>, color: Color| g.linestrip_2d(positions.iter(), color);
    lineloop2d => |g, positions: HostPoints<Vec2>, color: Color| g.lineloop_2d(positions.iter(), color);
    linestripGradient2d => |g, positions: HostList<(Vec2, Color)>| g.linestrip_gradient_2d(positions.0);
    ray2d => |g, start: Vec2, vector: Vec2, color: Color| g.ray_2d(start, vector, color);
    rayGradient2d => |g, start: Vec2, vector: Vec2, startColor: Color, endColor: Color| g.ray_gradient_2d(start, vector, startColor, endColor);
    rect => |g, isometry: Isometry3d, size: Vec2, color: Color| g.rect(isometry, size, color);
    rect2d => |g, isometry: Isometry2d, size: Vec2, color: Color| g.rect_2d(isometry, size, color);
    cube => |g, transform: HostTransform, color: Color| g.cube(transform, color);
    aabb3d => |g, aabb: Aabb3d, transform: HostTransform, color: Color| g.aabb_3d(aabb, transform, color);
    cross => |g, isometry: Isometry3d, halfSize: f32, color: Color| g.cross(isometry, halfSize, color);
    cross2d => |g, isometry: Isometry2d, halfSize: f32, color: Color| g.cross_2d(isometry, halfSize, color);
    circle as EllipseBuilder => |g, isometry: Isometry3d, radius: f32, color: Color| g.circle(isometry, radius, color);
    circle2d as Ellipse2dBuilder => |g, isometry: Isometry2d, radius: f32, color: Color| g.circle_2d(isometry, radius, color);
    ellipse as EllipseBuilder => |g, isometry: Isometry3d, halfSize: Vec2, color: Color| g.ellipse(isometry, halfSize, color);
    ellipse2d as Ellipse2dBuilder => |g, isometry: Isometry2d, halfSize: Vec2, color: Color| g.ellipse_2d(isometry, halfSize, color);
    sphere as SphereBuilder => |g, isometry: Isometry3d, radius: f32, color: Color| g.sphere(isometry, radius, color);
    arc2d as Arc2dBuilder => |g, isometry: Isometry2d, arcAngle: f32, radius: f32, color: Color| g.arc_2d(isometry, arcAngle, radius, color);
    arc3d as Arc3dBuilder => |g, angle: f32, radius: f32, isometry: Isometry3d, color: Color| g.arc_3d(angle, radius, isometry, color);
    shortArc3dBetween as Arc3dBuilder => |g, center: Vec3, from: Vec3, to: Vec3, color: Color| g.short_arc_3d_between(center, from, to, color);
    longArc3dBetween as Arc3dBuilder => |g, center: Vec3, from: Vec3, to: Vec3, color: Color| g.long_arc_3d_between(center, from, to, color);
    shortArc2dBetween as Arc2dBuilder => |g, center: Vec2, from: Vec2, to: Vec2, color: Color| g.short_arc_2d_between(center, from, to, color);
    longArc2dBetween as Arc2dBuilder => |g, center: Vec2, from: Vec2, to: Vec2, color: Color| g.long_arc_2d_between(center, from, to, color);
    arrow as ArrowBuilder => |g, start: Vec3, end: Vec3, color: Color| g.arrow(start, end, color);
    arrow2d as ArrowBuilder => |g, start: Vec2, end: Vec2, color: Color| g.arrow_2d(start, end, color);
    axes => |g, transform: HostTransform, baseLength: f32| g.axes(transform, baseLength);
    axes2d => |g, transform: HostTransform, baseLength: f32| g.axes_2d(transform, baseLength);
    grid as GridBuilder2d => |g, isometry: Isometry3d, cellCount: UVec2, spacing: Vec2, color: Color| g.grid(isometry, cellCount, spacing, color);
    grid3d as GridBuilder3d => |g, isometry: Isometry3d, cellCount: UVec3, spacing: Vec3, color: Color| g.grid_3d(isometry, cellCount, spacing, color);
    grid2d as GridBuilder2d => |g, isometry: Isometry2d, cellCount: UVec2, spacing: Vec2, color: Color| g.grid_2d(isometry, cellCount, spacing, color);
    roundedRect as RoundedRectBuilder => |g, isometry: Isometry3d, size: Vec2, color: Color| g.rounded_rect(isometry, size, color);
    roundedRect2d as RoundedRectBuilder => |g, isometry: Isometry2d, size: Vec2, color: Color| g.rounded_rect_2d(isometry, size, color);
    roundedCuboid as RoundedCuboidBuilder => |g, isometry: Isometry3d, size: Vec3, color: Color| g.rounded_cuboid(isometry, size, color);
    text => |g, isometry: Isometry3d, text: String, fontSize: f32, anchor: Vec2, color: Color| g.text(isometry, &text, fontSize, anchor, color);
    textSections => |g, isometry: Isometry3d, sections: HostList<(String, Color)>, fontSize: f32, anchor: Vec2| {
        let sections: Vec<(&str, Color)> = sections.0.iter().map(|(s, c)| (s.as_str(), *c)).collect();
        g.text_sections(isometry, &sections, fontSize, anchor)
    };
    text2d => |g, isometry: Isometry2d, text: String, fontSize: f32, anchor: Vec2, color: Color| g.text_2d(isometry, &text, fontSize, anchor, color);
    textSections2d => |g, isometry: Isometry2d, sections: HostList<(String, Color)>, fontSize: f32, anchor: Vec2| {
        let sections: Vec<(&str, Color)> = sections.0.iter().map(|(s, c)| (s.as_str(), *c)).collect();
        g.text_sections_2d(isometry, &sections, fontSize, anchor)
    };
    primitive2d as Primitive2dBuilder => |g @ registry, settings, primitive: Primitive2d, isometry: Isometry2d, color: Color| primitives::draw_2d(g, registry, &primitive.0, isometry, color, settings);
    primitive3d as Primitive3dBuilder => |g @ registry, settings, primitive: Primitive3d, isometry: Isometry3d, color: Color| primitives::draw_3d(g, registry, &primitive.0, isometry, color, settings);
    curve2d => |g @ registry, settings, curve: Curve2d, times: HostPoints<f32>, color: Color| {
        let _ = settings;
        primitives::curve_2d(g, registry, &curve.0, times.iter(), color)
    };
    curve3d => |g @ registry, settings, curve: Curve3d, times: HostPoints<f32>, color: Color| {
        let _ = settings;
        primitives::curve_3d(g, registry, &curve.0, times.iter(), color)
    };
    curveGradient2d => |g @ registry, settings, curve: Curve2d, timesWithColors: HostList<(f32, Color)>| {
        let _ = settings;
        primitives::curve_gradient_2d(g, registry, &curve.0, timesWithColors.0)
    };
    curveGradient3d => |g @ registry, settings, curve: Curve3d, timesWithColors: HostList<(f32, Color)>| {
        let _ = settings;
        primitives::curve_gradient_3d(g, registry, &curve.0, timesWithColors.0)
    };
}

/// A slice argument's bytes, by position.
fn views<'a>(slices: &'a [&mut [u8]]) -> [&'a [u8]; host::MAX_SLICES] {
    std::array::from_fn(|k| slices.get(k).map_or(&[][..], |s| &**s))
}

/// The host function of entry `E` on a `Gizmos<C>`: draws, and opens the builder if it returns one.
fn call_entry<C: GizmoConfigGroup, const E: usize>(
    state: &mut ParamState,
    world: UnsafeWorldCell<'_>,
    ticks: RunTicks,
    r: &mut SlotReader,
    _: &mut SlotWriter,
    slices: &mut [&mut [u8]],
) -> Result<(), String> {
    // SAFETY: as `#[export]`'s calls: the host calls this only from a TS run that holds the world
    // exclusively, with JS suspended; the state came from `host::init`; slices that could alias
    // were refused before the call.
    let mut p = unsafe { host::fetch::<TsGizmos<'static, 'static, C>>(state, world, ticks) }?;
    let p = &mut p;
    let entry = Entry::ALL[E];
    let buf: &mut GizmoBuffer<C, ()> = &mut p.gizmos;
    let start = lengths(buf);
    entry.draw(buf, Registry::App(&p.registry), r, &views(slices), &[])?;
    let open = &mut *p.open;
    open.live = entry.builder().is_some();
    if open.live {
        open.entry = Some(entry);
        open.run = Some(ticks.this_run);
        open.start = start;
        open.end = lengths(buf);
        open.args.clear();
        open.args.extend_from_slice(r.taken());
        open.settings.clear();
    }
    Ok(())
}

/// The host function of setting `S`: redraws the open builder with it (design 5).
fn call_setting<C: GizmoConfigGroup, const S: usize>(
    state: &mut ParamState,
    world: UnsafeWorldCell<'_>,
    ticks: RunTicks,
    r: &mut SlotReader,
    _: &mut SlotWriter,
    _: &mut [&mut [u8]],
) -> Result<(), String> {
    // SAFETY: as `call_entry`.
    let mut p = unsafe { host::fetch::<TsGizmos<'static, 'static, C>>(state, world, ticks) }?;
    let p = &mut p;
    let setting = Setting::ALL[S];
    let TsGizmos { gizmos, open, registry } = p;
    let buf: &mut GizmoBuffer<C, ()> = gizmos;
    let open = &mut **open;
    let entry = open.entry.filter(|_| open.live && open.run == Some(ticks.this_run) && open.end == lengths(buf));
    let Some(entry) = entry else {
        let builder = open.entry.and_then(Entry::builder).unwrap_or("a builder");
        return Err(format!("{builder}.{}: call it right after the call that returned the builder", setting.ts()));
    };
    let arg = r.bytes(setting.slots() * 8)?.to_vec();
    open.settings.push((setting, arg));
    truncate(buf, open.start);
    let redraw = |buf: &mut GizmoBuffer<C, ()>, settings: &Settings| entry.draw(buf, Registry::App(registry), &mut SlotReader::new(&open.args), &[], settings);
    if let Err(e) = redraw(buf, &open.settings) {
        // Not a setting of this builder: draw it as it was.
        truncate(buf, open.start);
        redraw(buf, &open.settings[..open.settings.len() - 1])?;
        open.settings.pop();
        return Err(e);
    }
    open.end = lengths(buf);
    Ok(())
}

/// Every method of `Gizmos<C>`: the entries, then the settings of each builder.
pub fn methods<C: GizmoConfigGroup>() -> Vec<MethodSpec> {
    let mut out: Vec<MethodSpec> = Entry::ALL
        .iter()
        .map(|e| MethodSpec { name: e.ts().into(), args: e.args(), ret: Kind::Unit, call: e.call::<C>(), chain: e.builder(), of: None, getter: false })
        .collect();
    let mut seen = std::collections::HashSet::new();
    for (_, builder, settings) in builders() {
        for (s, args) in settings {
            // A setting of several primitive builders is declared once on their TS interface.
            if seen.insert((builder, s)) {
                out.push(MethodSpec { name: s.ts().into(), args, ret: Kind::Unit, call: setting_call::<C>(s), chain: None, of: Some(builder), getter: false });
            }
        }
    }
    out
}

fn setting_call<C: GizmoConfigGroup>(s: Setting) -> host::HostFn {
    macro_rules! each {
        ($($id:ident),*) => {
            match s { $(Setting::$id => call_setting::<C, { Setting::$id as usize }>),* }
        };
    }
    each!(
        resolution,
        withTipLength,
        withDoubleEnd,
        skewX,
        skewY,
        skewZ,
        skew,
        outerEdgesX,
        outerEdgesY,
        outerEdgesZ,
        outerEdges,
        cornerRadius,
        arcResolution,
        edgeRadius,
        outerResolution,
        innerResolution,
        drawArrow,
        cellCount,
        spacing,
        baseResolution,
        heightResolution,
        minorResolution,
        majorResolution
    )
}
