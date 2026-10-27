//! Stage 2 of bevy-ui-coverage: `<text>`'s font, layout and decoration props, and `<span>` rich
//! text. Each prop writes its own component; a bound span writes only its own `TextSpan`.

use crate::common;

use bevy::prelude::*;
use bevy::text::{FontStyle, FontWeight, LetterSpacing, LineBreak, LineHeight, Strikethrough, StrikethroughColor, TextBackgroundColor, Underline, UnderlineColor};
use bevy::ui::widget::TextShadow;
use bevy_ts::generation::ReloadStatus;
use bevy_ts_tsx::{TsxNode, TsxTree};
use common::*;

const GAME: &str = r##"import { app, Component, u8, signal, px, Update, type Query } from "bevy-ts";

export class Hit extends Component { static path = "game::Hit"; declare on: u8 }
export const [hp, setHp] = signal(100);

export function hit(q: Query<[Hit]>) {
  q.each((h) => { if (h.on === 1) setHp(hp() - 10); });
}
app.addSystems(Update, hit);

app.scene("ui", () => (
  <>
    <text key="styled" size={18} weight={700} italic justify="center" linebreak="word-or-char" lineHeight={1.5} letterSpacing={px(2)}
      textShadow={{ x: 1, y: 2, color: "#000000" }} textBackground="#202020" underline="#ff0000" strikethrough>
      Styled
    </text>
    <text key="hp" size={20} color="#ffffff">HP <span key="num" color="#ff0000">{() => `${hp()}`}</span> / <span key="max" weight={300}>100</span>!</text>
  </>
));
"##;

fn setup(name: &str, src: &str) -> (crate::common::TestDir, App) {
    let dir = project(name, &[("game.tsx", src)]);
    let mut app = app(&dir.join("game.tsx"));
    let r = reload(&mut app, false);
    assert_eq!(r.status, ReloadStatus::Applied, "{:?}", r.errors);
    app.update();
    let errors: Vec<_> = logs(&app).into_iter().filter(|e| e.level == bevy_ts::log::LogLevel::Error).map(|e| e.message).collect();
    assert!(errors.is_empty(), "{errors:?}");
    app.insert_resource(dir.clone());
    (dir, app)
}

fn node(app: &mut App, path: &[&str]) -> Entity {
    let world = app.world_mut();
    world.query::<(Entity, &TsxNode)>().iter(world).find(|(_, n)| n.key_path == path).map(|(e, _)| e).unwrap_or_else(|| panic!("no node {path:?}"))
}

fn rgb(hex: &str) -> Color {
    Color::from(Srgba::hex(hex).unwrap())
}

#[test]
fn each_text_prop_writes_its_component() {
    let (_dir, mut app) = setup("ui-text-props", GAME);
    let e = node(&mut app, &["styled"]);
    let w = app.world();
    let f = w.get::<TextFont>(e).unwrap();
    assert_eq!((f.weight, f.style), (FontWeight(700), FontStyle::Italic));
    let l = w.get::<TextLayout>(e).unwrap();
    assert_eq!((l.justify, l.linebreak), (Justify::Center, LineBreak::WordOrCharacter));
    assert_eq!(*w.get::<LineHeight>(e).unwrap(), LineHeight::RelativeToFont(1.5));
    assert_eq!(*w.get::<LetterSpacing>(e).unwrap(), LetterSpacing::Px(2.0));
    let s = w.get::<TextShadow>(e).unwrap();
    assert_eq!((s.offset, s.color), (Vec2::new(1.0, 2.0), rgb("#000000")));
    assert_eq!(w.get::<TextBackgroundColor>(e).unwrap().0, rgb("#202020"));
    assert!(w.get::<Underline>(e).is_some());
    assert_eq!(w.get::<UnderlineColor>(e).unwrap().0, rgb("#ff0000"));
    assert!(w.get::<Strikethrough>(e).is_some());
    assert!(w.get::<StrikethroughColor>(e).is_none(), "strikethrough={{true}} keeps Bevy's colour");
}

#[test]
fn spans_are_child_entities_and_a_bound_span_writes_only_itself() {
    let (_dir, mut app) = setup("ui-text-spans", GAME);
    let root = node(&mut app, &["hp"]);
    assert_eq!(app.world().get::<Text>(root).unwrap().0, "HP ");
    let num = node(&mut app, &["hp", "num"]);
    let max = node(&mut app, &["hp", "max"]);
    let w = app.world();
    assert_eq!(w.get::<TextSpan>(num).unwrap().0, "100");
    assert_eq!(w.get::<TextColor>(num).unwrap().0, rgb("#ff0000"));
    assert_eq!(w.get::<ChildOf>(num).unwrap().parent(), root);
    assert!(w.get::<Node>(num).is_none(), "a span has no Node");
    // A span takes the font and colour props it does not set from its parent.
    assert_eq!(w.get::<TextFont>(num).unwrap().font_size, bevy::text::FontSize::Px(20.0));
    assert_eq!(w.get::<TextColor>(max).unwrap().0, rgb("#ffffff"));
    assert_eq!(w.get::<TextFont>(max).unwrap().weight, FontWeight(300));
    // Bevy reads spans in child order: HP, the number, " / ", 100, "!".
    let order: Vec<String> = w
        .get::<Children>(root)
        .unwrap()
        .iter()
        .map(|c| w.get::<TextSpan>(c).map(|s| s.0.clone()).unwrap_or_default())
        .collect();
    assert_eq!(order, ["100", " / ", "100", "!"]);

    let ticks = |app: &App, e: Entity| (app.world().entity(e).get_ref::<TextSpan>().unwrap().last_changed(), app.world().entity(e).get_ref::<TextFont>().unwrap().last_changed());
    let (max_before, root_before) = (ticks(&app, max), app.world().entity(root).get_ref::<Text>().unwrap().last_changed());
    let (_, num_font) = ticks(&app, num);
    let calls = app.world().resource::<TsxTree>().reactive.calls;
    let (id, _) = bevy_ts::plugin_api::component_by_name(app.world(), "game::Hit").unwrap();
    let e = app.world_mut().spawn_empty().id();
    // SAFETY: Hit is one u8.
    unsafe { bevy_ts::commands::insert_bytes(&mut app.world_mut().entity_mut(e), id, &[1]) };
    updates(&mut app, 2);
    app.world_mut().entity_mut(e).despawn();
    updates(&mut app, 1);
    assert_eq!(app.world().get::<TextSpan>(num).unwrap().0, "80");
    assert_eq!(ticks(&app, max), max_before, "a sibling span is not written");
    assert_eq!(app.world().entity(root).get_ref::<Text>().unwrap().last_changed(), root_before, "the root text is not written");
    assert_eq!(ticks(&app, num).1, num_font, "only the span's TextSpan is written");
    assert!(app.world().resource::<TsxTree>().reactive.calls > calls);
}

#[test]
fn a_span_outside_text_fails_naming_its_location() {
    let dir = project("ui-text-outside", &[("game.tsx", "import { app } from \"bevy-ts\";\napp.scene(\"ui\", () => (\n  <node>\n    <span>x</span>\n  </node>\n));\n")]);
    let mut app = app(&dir.join("game.tsx"));
    let r = reload(&mut app, false);
    assert_eq!(r.status, ReloadStatus::Failed);
    let e = r.errors.iter().find(|e| e.message.contains("<span>")).unwrap_or_else(|| panic!("{:?}", r.errors));
    assert_eq!(e.location.as_ref().map(|l| l.line), Some(4), "{e:?}");
}

#[test]
fn a_misspelt_text_word_names_the_accepted_ones() {
    let dir = project("ui-text-word", &[("game.tsx", "import { app } from \"bevy-ts\";\napp.scene(\"ui\", () => <text justify={\"middle\" as never}>x</text>);\n")]);
    let mut app = app(&dir.join("game.tsx"));
    let r = reload(&mut app, false);
    assert_eq!(r.status, ReloadStatus::Failed);
    assert!(r.errors.iter().any(|e| e.message.contains("middle") && e.message.contains("justified")), "{:?}", r.errors);
}

#[test]
fn font_is_an_asset_path() {
    let dir = project("ui-text-font", &[("game.tsx", "import { app } from \"bevy-ts\";\napp.scene(\"ui\", () => <text key=\"t\" font=\"fonts/Inter.ttf\">x</text>);\n")]);
    let mut app = app(&dir.join("game.tsx"));
    app.add_plugins(AssetPlugin::default()).init_asset::<bevy::text::Font>();
    assert_eq!(reload(&mut app, false).status, ReloadStatus::Applied);
    app.update();
    let e = node(&mut app, &["t"]);
    let bevy::text::FontSource::Handle(h) = &app.world().get::<TextFont>(e).unwrap().font else { panic!("not a handle") };
    assert_eq!(h.path().map(|p| p.to_string()), Some("fonts/Inter.ttf".into()));
}
