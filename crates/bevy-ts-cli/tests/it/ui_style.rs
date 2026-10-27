//! Stage 1 of bevy-ui-coverage: every `Node` field through `style`, grid, per-corner radius,
//! per-side border, `outline`, `shadow`, `transform`, gradients in `background` and `border`, and
//! a newtype's field named `value`. Each prop writes its own component and no other.

use crate::common;

use bevy::prelude::*;
use bevy::ui::{BackgroundGradient, BorderGradient, BoxShadow, Gradient, OverflowAxis, UiTransform, Val2};
use bevy_ts::generation::ReloadStatus;
use bevy_ts_tsx::{TsxNode, TsxTree};
use common::*;

const GAME: &str = r##"import { app, Component, u8, ref, signal, px, percent, fr, minmax, repeat, span, deg, linearGradient, radialGradient, conicGradient, Update, type Query, type Mut } from "bevy-ts";
import { ZIndex, TabIndex } from "bevy-ts/bevy";

export class Flip extends Component { static path = "game::Flip"; declare on: u8 }
export class Seen extends Component { static path = "game::Seen"; declare z: u8; declare tab: u8 }

export const [hot, setHot] = signal(false);
const panel = ref();

export function flip(q: Query<[Flip]>) {
  q.each((f) => { if (f.on === 1 && !hot()) setHot(true); if (f.on === 2 && hot()) setHot(false); });
}

/** A TS system reads a newtype's field as `value`. */
export function seeTabs(tabs: Query<[TabIndex]>, seen: Query<[Mut<Seen>]>) {
  let t = 0;
  tabs.each((i) => { t = i.value; });
  seen.each((s) => { s.tab = t; });
}

app.addSystems(Update, flip, seeTabs);

app.scene("ui", () => (
  <>
    <entity key="seen"><Seen /></entity>
    <node key="grid" style={{ display: "grid", gridTemplateColumns: [fr(1), px(200), repeat(2, [minmax(px(40), fr(2))])], gridTemplateRows: repeat("auto-fill", px(30)), gridAutoRows: ["auto", "min-content"], gridAutoFlow: "column-dense" }}>
      <node key="cell" style={{ gridColumn: span(2), gridRow: { start: 1, end: 3 } }} />
      <node key="line" style={{ gridColumn: 2 }} />
    </node>
    <node key="keys" style={{ boxSizing: "content-box", justifyItems: "center", justifySelf: "end", alignContent: "space-between", direction: "rtl", scrollbarWidth: 6, overflowClipMargin: { visualBox: "padding-box", margin: 3 }, overflow: { x: "clip", y: "scroll" } }} />
    <node key="corners" radius={[px(4), 0, px(4), 0]} border={{ top: "#ff0000", left: "#00ff00" }} />
    <node key="named" radius={{ bottomLeft: percent(50) }} />
    <node key="outlined" outline={{ color: "#ff0000" }} />
    <node key="looks" ref={panel}
      shadow={{ color: "#00000088", x: 0, y: px(4), blur: px(8) }}
      outline={{ width: px(2), color: "#ffffff" }}
      transform={{ translation: [px(10), percent(5)], scale: 2, rotation: deg(90) }}
      background={() => (hot() ? linearGradient(90, ["#ff0000", ["#0000ff", percent(80)]]) : "#222222")}
      border={() => (hot() ? "#ffffff" : radialGradient(["#ff0000", "#0000ff"], { at: [px(10), px(20)], shape: { circle: px(30) } }))}
    >
      <ZIndex value={2} />
      <TabIndex value={3} />
    </node>
    <node key="shadows" shadow={[{ color: "#ff0000" }, { color: "#0000ff", spread: px(2) }]} background={[conicGradient(["#ff0000", ["#00ff00", 180]], { from: 90 }), linearGradient(0, ["#000000", "#ffffff"])]} />
    <text key="z">{() => `z ${panel.get(ZIndex).value}`}</text>
  </>
));
"##;

fn setup(name: &str, src: &str) -> (crate::common::TestDir, App) {
    let dir = project(name, &[("game.tsx", src)]);
    let mut app = app(&dir.join("game.tsx"));
    let r = reload(&mut app, false);
    assert_eq!(r.status, ReloadStatus::Applied, "{:?}", r.errors);
    app.update();
    let errors = errors(&app);
    assert!(errors.is_empty(), "{errors:?}");
    app.insert_resource(dir.clone());
    (dir, app)
}

fn errors(app: &App) -> Vec<String> {
    logs(app).into_iter().filter(|e| e.level == bevy_ts::log::LogLevel::Error).map(|e| e.message).collect()
}

fn node(app: &mut App, path: &[&str]) -> Entity {
    let world = app.world_mut();
    world.query::<(Entity, &TsxNode)>().iter(world).find(|(_, n)| n.key_path == path).map(|(e, _)| e).unwrap_or_else(|| panic!("no node {path:?}"))
}

fn ui<T: Component + Clone>(app: &mut App, path: &[&str]) -> T {
    let e = node(app, path);
    app.world().get::<T>(e).unwrap_or_else(|| panic!("{path:?} has no {}", std::any::type_name::<T>())).clone()
}

fn set_flip(app: &mut App, v: u8) {
    let (id, _) = bevy_ts::plugin_api::component_by_name(app.world(), "game::Flip").unwrap();
    let e = app.world_mut().spawn_empty().id();
    // SAFETY: Flip is one u8.
    unsafe { bevy_ts::commands::insert_bytes(&mut app.world_mut().entity_mut(e), id, &[v]) };
}

fn rgb(hex: &str) -> Color {
    Color::from(Srgba::hex(hex).unwrap())
}

#[test]
fn grid_tracks_and_placements() {
    let (_dir, mut app) = setup("ui-grid", GAME);
    let n: Node = ui(&mut app, &["grid"]);
    assert_eq!(n.display, Display::Grid);
    let cols: Vec<RepeatedGridTrack> = vec![GridTrack::fr(1.0), GridTrack::px(200.0), RepeatedGridTrack::repeat_many(2, vec![GridTrack::minmax(MinTrackSizingFunction::Px(40.0), MaxTrackSizingFunction::Fraction(2.0))])];
    assert_eq!(n.grid_template_columns, cols);
    assert_eq!(n.grid_template_rows, vec![RepeatedGridTrack::px(GridTrackRepetition::AutoFill, 30.0)]);
    assert_eq!(n.grid_auto_rows, vec![GridTrack::auto(), GridTrack::min_content()]);
    assert_eq!(n.grid_auto_flow, GridAutoFlow::ColumnDense);
    let cell: Node = ui(&mut app, &["grid", "cell"]);
    assert_eq!(cell.grid_column, GridPlacement::span(2));
    assert_eq!(cell.grid_row, GridPlacement::start_end(1, 3));
    assert_eq!(ui::<Node>(&mut app, &["grid", "line"]).grid_column, GridPlacement::start(2));
}

#[test]
fn the_remaining_node_fields() {
    let (_dir, mut app) = setup("ui-keys", GAME);
    let n: Node = ui(&mut app, &["keys"]);
    assert_eq!(n.box_sizing, BoxSizing::ContentBox);
    assert_eq!(n.justify_items, JustifyItems::Center);
    assert_eq!(n.justify_self, JustifySelf::End);
    assert_eq!(n.align_content, AlignContent::SpaceBetween);
    assert_eq!(n.direction, bevy::ui::InlineDirection::Rtl);
    assert_eq!(n.scrollbar_width, 6.0);
    assert_eq!(n.overflow_clip_margin, OverflowClipMargin { visual_box: bevy::ui::VisualBox::PaddingBox, margin: 3.0 });
    assert_eq!((n.overflow.x, n.overflow.y), (OverflowAxis::Clip, OverflowAxis::Scroll));
}

#[test]
fn corners_and_sides() {
    let (_dir, mut app) = setup("ui-corners", GAME);
    let r = ui::<Node>(&mut app, &["corners"]).border_radius;
    assert_eq!(r, BorderRadius::new(Val::Px(4.0), Val::Px(0.0), Val::Px(4.0), Val::Px(0.0)));
    let b: BorderColor = ui(&mut app, &["corners"]);
    assert_eq!((b.top, b.left, b.right), (rgb("#ff0000"), rgb("#00ff00"), BorderColor::DEFAULT.right));
    let r = ui::<Node>(&mut app, &["named"]).border_radius;
    assert_eq!(r.bottom_left, Val::Percent(50.0).into());
    assert_eq!(r.top_left, BorderRadius::ZERO.top_left);
}

#[test]
fn shadow_outline_and_transform() {
    let (_dir, mut app) = setup("ui-looks", GAME);
    let s: BoxShadow = ui(&mut app, &["looks"]);
    assert_eq!(s.0.len(), 1);
    assert_eq!((s.0[0].color, s.0[0].x_offset, s.0[0].y_offset, s.0[0].blur_radius), (Color::srgba(0.0, 0.0, 0.0, 0x88 as f32 / 255.0), Val::Px(0.0), Val::Px(4.0), Val::Px(8.0)));
    let o: Outline = ui(&mut app, &["looks"]);
    assert_eq!((o.width, o.color), (Val::Px(2.0), rgb("#ffffff")));
    // A missing field is Bevy's default: an outline given only a colour is 1 px wide.
    let o: Outline = ui(&mut app, &["outlined"]);
    assert_eq!((o.width, o.offset, o.color), (Val::Px(1.0), Val::Px(0.0), rgb("#ff0000")));
    let t: UiTransform = ui(&mut app, &["looks"]);
    assert_eq!(t.translation, Val2::new(Val::Px(10.0), Val::Percent(5.0)));
    assert_eq!(t.scale, Vec2::splat(2.0));
    assert!((t.rotation.as_radians() - std::f32::consts::FRAC_PI_2).abs() < 1e-5);
    let many: BoxShadow = ui(&mut app, &["shadows"]);
    assert_eq!(many.0.len(), 2);
    assert_eq!(many.0[1].spread_radius, Val::Px(2.0));
}

#[test]
fn background_and_border_own_colour_and_gradient() {
    let (_dir, mut app) = setup("ui-gradients", GAME);
    let e = node(&mut app, &["looks"]);
    assert_eq!(app.world().get::<BackgroundColor>(e).unwrap().0, rgb("#222222"));
    assert!(app.world().get::<BackgroundGradient>(e).is_none());
    let Gradient::Radial(r) = &app.world().get::<BorderGradient>(e).unwrap().0[0] else { panic!("not radial") };
    assert_eq!(r.shape, bevy::ui::RadialGradientShape::Circle(Val::Px(30.0)));
    assert!(app.world().get::<BorderColor>(e).is_none());
    let outline_tick = app.world().entity(e).get_ref::<Outline>().unwrap().last_changed();
    let node_tick = app.world().entity(e).get_ref::<Node>().unwrap().last_changed();

    set_flip(&mut app, 1);
    updates(&mut app, 2);
    let Gradient::Linear(l) = &app.world().get::<BackgroundGradient>(e).expect("gradient after the flip").0[0] else { panic!("not linear") };
    assert!((l.angle - std::f32::consts::FRAC_PI_2).abs() < 1e-5);
    assert_eq!(l.stops[1].point, Val::Percent(80.0));
    assert!(app.world().get::<BackgroundColor>(e).is_none(), "the colour goes when the gradient comes");
    assert_eq!(app.world().get::<BorderColor>(e).unwrap().top, rgb("#ffffff"));
    assert!(app.world().get::<BorderGradient>(e).is_none());
    // Only background and border were written.
    assert_eq!(app.world().entity(e).get_ref::<Outline>().unwrap().last_changed(), outline_tick);
    assert_eq!(app.world().entity(e).get_ref::<Node>().unwrap().last_changed(), node_tick);

    set_flip(&mut app, 2);
    updates(&mut app, 2);
    assert!(app.world().get::<BackgroundColor>(e).is_some());
    assert!(app.world().get::<BackgroundGradient>(e).is_none());

    let shadows = node(&mut app, &["shadows"]);
    let layers = app.world().get::<BackgroundGradient>(shadows).unwrap().0.clone();
    assert!(matches!(&layers[..], [Gradient::Conic(c), Gradient::Linear(_)] if (c.start - std::f32::consts::FRAC_PI_2).abs() < 1e-5));
}

#[test]
fn a_newtypes_field_is_value() {
    let (_dir, mut app) = setup("ui-value", GAME);
    let e = node(&mut app, &["looks"]);
    assert_eq!(app.world().get::<ZIndex>(e).unwrap().0, 2);
    assert_eq!(app.world().get::<bevy::input_focus::tab_navigation::TabIndex>(e).unwrap().0, 3);
    let z = node(&mut app, &["z"]);
    assert_eq!(app.world().get::<Text>(z).unwrap().0, "z 2", "ref.get(ZIndex).value");
    updates(&mut app, 2);
    let world = app.world_mut();
    let seen = world.query::<(Entity, &TsxNode)>().iter(world).find(|(_, n)| n.key_path == ["seen"]).unwrap().0;
    assert_eq!(read_ts(&app, seen, "game::Seen").unwrap()["tab"], 3, "a TS system reads TabIndex.value");
    assert!(app.world().resource::<TsxTree>().reactive.calls > 0);
}

#[test]
fn a_misspelt_key_names_itself() {
    let dir = project("ui-typo", &[("game.tsx", r#"import { app, fr } from "bevy-ts";
app.scene("ui", () => <node style={{ gridTemplateColums: [fr(1)] } as never} />);
"#)]);
    let mut app = app(&dir.join("game.tsx"));
    let r = reload(&mut app, false);
    assert_eq!(r.status, ReloadStatus::Failed);
    assert!(r.errors.iter().any(|e| e.message.contains("gridTemplateColums")), "{:?}", r.errors);
}
