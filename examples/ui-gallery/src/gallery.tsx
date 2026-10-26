// Every bevy_ui element bevy-ts reaches from TSX, one panel per stage of the bevy-ui-coverage change.
// Not shipped (no example.json): `bevy-ts dev src/gallery.tsx`. A test (crates/bevy-ts-cli/tests/it/ui_gallery.rs)
// loads it headless and fails when an intrinsic in jsx-runtime.d.ts is not mounted here.
import { app, Show, Update, conicGradient, deg, fr, linearGradient, percent, px, radialGradient, ref, repeat, signal, sliced, span, tiled } from "bevy-ts";
import { AutoDirectionalNavigation, Camera3d, Checked, GlobalZIndex, Pressed, ScrollPosition, TextureAtlasLayout, Transform, ZIndex } from "bevy-ts/bevy";
import { CompassOctant, KeyCode, type DirectionalNavigation, type InputFocusVisible, type Keys } from "bevy-ts/input";
import type { Child } from "bevy-ts/jsx-runtime";

/** A titled panel in the gallery's grid of panels. */
const Panel = ({ title, children }: { title: string; children?: Child }) => (
  <node style={{ flexDirection: "column", rowGap: px(6), padding: px(10), minWidth: px(260) }} background="#1b1f27ee" border="#3a4150" radius={px(6)}>
    <text key="title" size={16} color="#9fb3d1">{title}</text>
    <node key="body" style={{ flexDirection: "column", rowGap: px(6) }}>
      {children}
    </node>
  </node>
);

const [clicks, setClicks] = signal(0);

/** Stage 0: the four intrinsics bevy-ts had before the change. */
const Basics = () => (
  <Panel title="Basics">
    <node key="box" style={{ width: px(120), height: px(24) }} background="#3366cc" />
    <text key="text" size={14}>{() => `Plain text, clicked ${clicks()} times`}</text>
    <image key="image" src="ui/checker.png" style={{ width: px(48), height: px(48) }} />
    <button key="button" style={{ padding: px(6) }} background="#2a7a4b" radius={px(4)} onPointerClick={() => setClicks(clicks() + 1)}>
      <text>Click me</text>
    </button>
  </Panel>
);

const [tilted, setTilted] = signal(false);

/** Stage 1: grid, the remaining `Node` fields, corners, sides, outline, shadow, transform, gradients. */
const Style = () => (
  <Panel title="Style">
    <node key="grid" style={{ display: "grid", gridTemplateColumns: [fr(1), px(60), repeat(2, fr(1))], gridAutoRows: px(18), columnGap: px(4), rowGap: px(4), width: px(240) }}>
      <node key="wide" style={{ gridColumn: span(2) }} background="#4a6fa5" />
      <node key="a" background="#6b8fc7" />
      <node key="b" background="#6b8fc7" />
      <node key="tall" style={{ gridRow: span(2) }} background="#a5784a" />
      <node key="c" style={{ gridColumn: { start: 2, span: 3 } }} background="#4aa57a" />
    </node>
    <node key="row" style={{ columnGap: px(10), alignItems: "center" }}>
      <node key="corners" style={{ width: px(40), height: px(40), border: px(3) }} radius={[px(14), 0, px(14), 0]} border={{ top: "#ff6060", bottom: "#60ff60" }} background="#333a48" />
      <node key="outline" style={{ width: px(40), height: px(40) }} radius={percent(50)} outline={{ width: px(2), offset: px(2), color: "#ffd060" }} background="#333a48" />
      <node key="shadow" style={{ width: px(40), height: px(40) }} radius={px(6)} shadow={{ color: "#000000c0", x: px(3), y: px(4), blur: px(6) }} background="#566078" />
      <button key="tilt" style={{ width: px(40), height: px(40) }} transform={() => ({ rotation: tilted() ? deg(15) : 0, scale: tilted() ? 1.1 : 1 })} background="#7a4ab0" onPointerClick={() => setTilted(!tilted())}>
        <ZIndex value={1} />
      </button>
    </node>
    <node key="gradients" style={{ columnGap: px(10) }}>
      <node key="linear" style={{ width: px(60), height: px(30) }} background={linearGradient(90, ["#ff5f6d", "#ffc371"])} />
      <node key="radial" style={{ width: px(60), height: px(30) }} background={radialGradient(["#ffffff", "#2a5298"])} />
      <node key="conic" style={{ width: px(30), height: px(30) }} radius={percent(50)} background={conicGradient(["#f00", "#ff0", "#0f0", "#0ff", "#00f", "#f0f", "#f00"])} />
      <node key="frame" style={{ width: px(60), height: px(30), border: px(3) }} border={linearGradient(0, ["#00c6ff", "#0072ff"])} />
    </node>
  </Panel>
);

/** Stages 2 and 3a: rich text with `<span>`, font, layout, decorations and a halo. */
const Text = () => (
  <Panel title="Text">
    <text key="rich" size={16}>
      HP <span key="hp" color="#ff6060" weight={700}>{() => `${100 - clicks()}`}</span> / 100 <span key="note" italic color="#9fb3d1">(click Basics to hurt)</span>
    </text>
    <text key="layout" size={13} justify="center" lineHeight={1.4} letterSpacing={px(1)} style={{ width: px(300) }}>
      Centred text that wraps over two lines, with wider letters.
    </text>
    <text key="decorated" size={16} underline="#ffd060" textShadow={{ x: 1, y: 1, color: "#000000" }}>
      Underlined, with a shadow, and <span key="struck" strikethrough textBackground="#553333">struck through</span>
    </text>
    <node key="pale" style={{ padding: px(6) }} background="#e8e4d8">
      <text key="haloed" size={18} color="#ffffff" textHalo={{ width: 1.5, color: "#000000" }}>
        White on pale, with a halo
      </text>
    </node>
  </Panel>
);

const mini = ref();

/** Stage 7: a viewport on a second camera, and `<image>`'s flip, rect, atlas and modes. */
const Media = () => (
  <Panel title="Cameras and images">
    <node key="row" style={{ columnGap: px(10), alignItems: "center" }}>
      <viewport key="minimap" camera={mini} style={{ width: px(96), height: px(72), border: px(2) }} border="#9fb3d1" radius={px(4)} />
      <image key="flip" src="ui/checker.png" flipX rect={[0, 0, 32, 64]} style={{ width: px(24), height: px(48) }} color="#ffd060" />
      <image key="atlas" src="ui/checker.png" atlas={{ layout: TextureAtlasLayout.fromGrid({ tileSize: [16, 16], columns: 4, rows: 4 }), index: 5 }} style={{ width: px(32), height: px(32) }} />
    </node>
    <node key="modes" style={{ columnGap: px(10) }}>
      <image key="sliced" src="ui/checker.png" mode={sliced({ border: 16 })} style={{ width: px(120), height: px(40) }} />
      <image key="tiled" src="ui/checker.png" mode={tiled({ stretch: 1 })} style={{ width: px(120), height: px(40) }} />
    </node>
  </Panel>
);

const [submitted, setSubmitted] = signal("");

/** `<input>`: Bevy's one-line `TextInput`; Enter submits, Escape drops focus. */
const TextInput = () => (
  <Panel title="Text input">
    <input
      key="input"
      style={{ width: px(240), padding: px(4) }}
      background="#0d1017"
      border="#3a4150"
      size={14}
      color="#e6edf3"
      initialValue="Type, then Enter"
      maxCharacters={40}
      onSubmit={({ value }) => setSubmitted(value)}
    />
    <text key="submitted" size={12} color="#9fb3d1">{() => `Submitted: ${submitted()}`}</text>
  </Panel>
);

const [events, setEvents] = signal<string[]>([]);
const note = (s: string) => setEvents((l) => [s, ...l].slice(0, 4));
const [locked, setLocked] = signal(false);
const held = ref();

/** Stage 3: the picking events with Bevy's fields, `onActivate`, `disabled` and `has(Pressed)`. */
const Events = () => (
  <Panel title="Events">
    <node key="row" style={{ columnGap: px(10), alignItems: "center" }}>
      <node key="pad" style={{ width: px(90), height: px(48), justifyContent: "center", alignItems: "center" }} background="#2c3444" radius={px(4)}
        onPointerEnter={() => note("enter")}
        onPointerLeave={() => note("leave")}
        onPointerDrag={({ distance }) => note(`drag ${Math.round(distance[0])},${Math.round(distance[1])}`)}
        onPointerScroll={({ unit, y }) => note(`scroll ${y} ${unit}`)}>
        <text size={12} color="#9fb3d1">hover, drag, scroll</text>
      </node>
      <button key="press" ref={held} style={{ padding: px(6) }} radius={px(4)}
        background={() => (held.has(Pressed) ? "#1d5c36" : "#2a7a4b")}
        disabled={() => locked()}
        onActivate={() => note("activate")}>
        <text size={14}>{() => (locked() ? "Locked" : "Press")}</text>
      </button>
      <button key="lock" style={{ padding: px(6) }} radius={px(4)} background="#555d6e" onActivate={() => setLocked(!locked())}>
        <text size={14}>{() => (locked() ? "Unlock" : "Lock")}</text>
      </button>
    </node>
    <text key="log" size={13} color="#d0d6e0">{() => events().join(" · ") || "no events yet"}</text>
  </Panel>
);

const [focused, setFocused] = signal("");
const [ring, setRing] = signal(false);
const [lastCause, setLastCause] = signal("");

/** The arrows move focus between the Focus panel's buttons; Tab is Bevy's own. */
export function focusKeys(keys: Keys, nav: DirectionalNavigation, visible: InputFocusVisible) {
  if (keys.justPressed(KeyCode.ArrowRight)) nav.navigate(CompassOctant.East);
  if (keys.justPressed(KeyCode.ArrowLeft)) nav.navigate(CompassOctant.West);
  if (visible.get() !== ring()) setRing(visible.get());
}
app.addSystems(Update, focusKeys);

/** A focusable button: blue while focused, outlined only when focus came from the keyboard. */
const Focusable = ({ name }: { name: string }) => (
  <button key={name} tabIndex={0} style={{ padding: px(6), border: px(2) }} radius={px(4)}
    background={() => (focused() === name ? "#2f5fa8" : "#3a4150")}
    border={() => (focused() === name && ring() ? "#ffd060" : "#00000000")}
    onFocusGained={({ cause }) => { setFocused(name); setLastCause(cause); }}
    onFocusLost={() => setFocused((f) => (f === name ? "" : f))}>
    <AutoDirectionalNavigation />
    <text size={14}>{name}</text>
  </button>
);
/** Stage 4: Tab and Shift-Tab (tabNavigation), the arrows (directionalNavigation), focus events. */
const Focus = () => (
  <Panel title="Focus">
    <node key="row" tabGroup={{}} style={{ columnGap: px(8) }}>
      <Focusable name="one" />
      <Focusable name="two" />
      <Focusable name="three" />
    </node>
    <text key="state" size={13} color="#d0d6e0">{() => (focused() ? `${focused()} (${lastCause()})` : "Tab or click a button")}</text>
  </Panel>
);

const [sound, setSound] = signal(true);
const [volume, setVolume] = signal(40);
const [page, setPage] = signal<string | null>("audio");
const [pick, setPick] = signal<string | null>("b");
const longList = ref();
const box = { width: px(16), height: px(16), border: px(2) };
const radios = { easy: ref(), hard: ref() };

/** Stage 5: checkbox, radio group, slider, tabs, list box, and a scroll area with its scrollbar. */
const Widgets = () => (
  <Panel title="Widgets">
    <node key="row" style={{ columnGap: px(10), alignItems: "center" }}>
      <checkbox key="sound" style={box} border="#9fb3d1" background={() => (sound() ? "#2f5fa8" : "#3a4150")} checked={() => sound()} onValueChange={({ value }) => setSound(value)} />
      <text key="label" size={13}>{() => (sound() ? "Sound on" : "Sound off")}</text>
      <radioGroup key="level" style={{ columnGap: px(4) }}>
        {/* Uncontrolled: Bevy moves `Checked` itself; the look reads it. */}
        {(["easy", "hard"] as const).map((k) => (
          <radio key={k} ref={radios[k]} style={{ columnGap: px(4), alignItems: "center" }} initialChecked={k === "easy"}>
            <node key="dot" style={box} border="#9fb3d1" radius={percent(50)} background={() => (radios[k].has(Checked) ? "#2f5fa8" : "#3a4150")} />
            <text key="label" size={13}>{k}</text>
          </radio>
        ))}
      </radioGroup>
    </node>
    <slider key="volume" style={{ width: px(200), height: px(12) }} background="#2c3444" radius={px(6)} value={() => volume()} min={0} max={100} onValueChange={({ value }) => setVolume(Math.round(value))}>
      {/* Bevy leaves the thumb to the game: moved by transform (stage 3b), 188 px of track. */}
      <sliderThumb key="thumb" style={{ position: "absolute", width: px(12), height: px(12) }} background="#ffd060" radius={percent(50)}
        transform={() => ({ translation: [px(volume() * 1.88), 0] })} />
    </slider>
    <tabList key="tabs" style={{ columnGap: px(4) }} selected={() => page()} onValueChange={({ key }) => setPage(key)}>
      {["audio", "video"].map((k) => (
        <tab key={k} tabIndex={0} style={{ padding: px(4) }} background={() => (page() === k ? "#2f5fa8" : "#3a4150")}>
          <text size={12}>{k}</text>
        </tab>
      ))}
    </tabList>
    <node key="lists" style={{ columnGap: px(8) }}>
      <listBox key="list" style={{ flexDirection: "column", width: px(80) }} onValueChange={({ key }) => setPick(key)}>
        {["a", "b", "c"].map((k) => (
          <listItem key={k} selected={() => pick() === k} style={{ padding: px(2) }} background={() => (pick() === k ? "#2f5fa8" : "#00000000")}>
            <text size={12}>{`Item ${k}`}</text>
          </listItem>
        ))}
      </listBox>
      <node key="scroller" ref={longList} scrollArea style={{ width: px(100), height: px(60), flexDirection: "column", overflow: { y: "scroll" } }} background="#11151c">
        {Array.from({ length: 8 }, (_, i) => <text key={`line${i}`} size={12} style={{ flexShrink: 0 }}>{`Line ${i + 1}`}</text>)}
      </node>
      <scrollbar key="bar" target={longList} style={{ width: px(8), height: px(60) }} background="#2c3444">
        <scrollbarThumb key="thumb" background="#9fb3d1" radius={px(4)} />
      </scrollbar>
    </node>
  </Panel>
);

const [menuOpen, setMenuOpen] = signal(true);
const [picked, setPicked] = signal("normal");
const [modalShown, setModalShown] = signal(false);

/** The menu's owner: Bevy asks, by `MenuEvent`, and the game opens or closes the popup. */
function onMenu({ action }: { action: string }) {
  if (action === "toggle") setMenuOpen(!menuOpen());
  else if (action.startsWith("open")) setMenuOpen(true);
  else if (action === "close-all") setMenuOpen(false);
}

const itemLook = { padding: px(4), minWidth: px(90) };

/** Stage 6: a menu whose popup is a popover under its button, a floating dialog dragged by its title, and a modal. */
const Overlays = () => (
  <Panel title="Overlays">
    {/* Open at load, so the gallery mounts every intrinsic; focus leaving it closes it, as in Bevy. */}
    <node key="menu" onMenuEvent={onMenu}>
      <menuButton key="button" style={{ padding: px(4) }} background="#3a4150" radius={px(4)}>
        <text key="label" size={13}>{() => `Difficulty: ${picked()}`}</text>
      </menuButton>
      <Show when={menuOpen}>
        <popover key="pop" positions={[{ side: "bottom", align: "start", gap: 2 }, { side: "top", align: "start", gap: 2 }]}>
          {/* Over floating dialogs, which Bevy stacks at 50 to 89. */}
          <GlobalZIndex value={100} />
          <menuPopup key="popup" style={{ flexDirection: "column", padding: px(2), border: px(1) }} background="#11151c" border="#3a4150">
            {["easy", "normal", "hard"].map((k) => (
              <menuItem key={k} style={itemLook} background={() => (picked() === k ? "#2f5fa8" : "#00000000")} onActivate={() => setPicked(k)}>
                <text key="label" size={12}>{k}</text>
              </menuItem>
            ))}
          </menuPopup>
        </popover>
      </Show>
    </node>
    <dialog key="note" style={{ flexDirection: "column", width: px(180), border: px(1) }} background="#1b2433" border="#3a4150" radius={px(4)}>
      <dialogDragHandle key="title" style={{ padding: px(4) }} background="#2f5fa8"><text key="t" size={12}>Drag me</text></dialogDragHandle>
      <text key="body" size={12} style={{ padding: px(4) }}>A dialog that is not modal</text>
    </dialog>
    <button key="open" style={{ padding: px(4) }} background="#3a4150" radius={px(4)} onActivate={() => setModalShown(true)}>
      <text key="t" size={13}>Open the modal</text>
    </button>
  </Panel>
);

/** Over everything while shown; the barrier stays mounted, hidden, and the dialog mounts modal when it opens. Escape or a click outside closes it. */
const Modal = () => (
  <modalDialogBarrier key="modal" style={() => ({ display: modalShown() ? "flex" : "none", position: "absolute", width: percent(100), height: percent(100), justifyContent: "center", alignItems: "center" })}
    background="#000000aa" onRequestClose={() => setModalShown(false)}>
    {/* Over floating dialogs (Bevy's 50 to 89), as feathers' barrier is; menus' popovers sit at 100. */}
    <GlobalZIndex value={99} />
    <Show when={modalShown}>
      <dialog key="d" modal style={{ flexDirection: "column", padding: px(12), rowGap: px(8), border: px(1) }} background="#1b1f27" border="#3a4150" radius={px(6)}>
        <dialogDragHandle key="title"><text key="t" size={16}>A modal dialog</text></dialogDragHandle>
        <text key="body" size={13}>Tab stays inside. Escape or a click outside closes it.</text>
        <button key="ok" tabIndex={0} style={{ padding: px(4) }} background="#2f5fa8" radius={px(4)} onActivate={() => setModalShown(false)}>
          <text key="t" size={13}>OK</text>
        </button>
      </dialog>
    </Show>
  </modalDialogBarrier>
);

const [tick, setTick] = signal(0);
export function drift() {
  setTick(tick() + 1);
}
app.addSystems(Update, drift);

/** Stage 3b: 40 dots moved every frame by `transform`; their `style` is static, so nothing lays out again. */
const Placement = () => (
  <Panel title="Placement">
    <node key="field" style={{ width: px(240), height: px(60), overflow: "clip" }} background="#11151c">
      {Array.from({ length: 40 }, (_, i) => (
        <node key={`dot${i}`} style={{ position: "absolute", left: 0, top: 0, width: px(4), height: px(4) }} radius={percent(50)} background="#9fb3d1"
          transform={() => ({ translation: [px((i * 37 + tick()) % 236), px((i * 13) % 56)] })} />
      ))}
    </node>
  </Panel>
);

/** The panels scroll by the wheel: there are more than a 960×540 window shows. */
const [scrollY, setScrollY] = signal(0);

app.scene("gallery", () => (
  <>
    {/* Not UI: a cube behind the panels, so <entity>, <mesh> and <material> appear too. */}
    {/* The game's camera, in the scene so edit mode's Game tab shows the gallery. At (-3, 3, 6), looking at the origin. */}
    <entity key="camera">
      <Camera3d />
      <Transform translation={[-3, 3, 6]} rotation={[-0.2031, -0.2247, -0.048, 0.9518]} />
    </entity>
    <entity key="cube">
      <mesh shape="cuboid" size={[1, 1, 1]} />
      <material color="#c33" roughness={0.6} />
    </entity>
    {/* Not UI: a label above the cube, so <worldText> appears (its own entity; it cannot share one with a mesh). */}
    <entity key="label">
      <Transform translation={[0, 1, 0]} />
      <worldText size={0.4} color="#ffd060" outline={{ width: 0.08, color: "#000" }}>{() => `Clicked ${clicks()} times`}</worldText>
    </entity>
    {/* Not UI either: a flat square, so <mesh2d> and <material2d> appear (it draws under a Camera2d only). */}
    <entity key="square">
      <mesh2d shape="rectangle" size={[1, 1]} />
      <material2d color="#3c6" />
    </entity>
    {/* Not UI: 2D text, so <text2d> appears (it draws under a Camera2d only). */}
    <entity key="caption">
      <text2d size={16} color="#fff">{() => `Clicked ${clicks()} times`}</text2d>
    </entity>
    {/* The minimap's camera, above the cube looking down; its viewport gives it an image of its own. */}
    <entity key="mini" ref={mini}>
      <Camera3d />
      <Transform translation={[0, 4, 0]} rotation={[-0.7071, 0, 0, 0.7071]} />
    </entity>
    <node key="gallery" style={{ position: "absolute", top: px(12), left: px(12), right: px(12), bottom: px(12), flexWrap: "wrap", alignContent: "flex-start", columnGap: px(12), rowGap: px(12), overflow: { x: "clip", y: "scroll" } }}
      onPointerScroll={({ unit, y }) => setScrollY(Math.max(0, scrollY() - y * (unit === "line" ? 24 : 1)))}>
      <ScrollPosition value={() => [0, scrollY()] as const} />
      <Basics />
      <Style />
      <Text />
      <Events />
      <Focus />
      <Placement />
      <Widgets />
      <Overlays />
      <Media />
      <TextInput />
    </node>
    <Modal />
  </>
));
