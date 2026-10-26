// Type tests for the UI intrinsics' props: `tsc -p test` must pass, and every `@ts-expect-error`
// must be an error.
import { app, conicGradient, deg, fr, linearGradient, minmax, percent, px, radialGradient, ref, repeat, signal, sliced, span, tiled } from "bevy-ts";
import { TextureAtlasLayout, ZIndex } from "bevy-ts/bevy";

app.scene("ui", () => (
  <>
    <node
      style={{ display: "grid", gridTemplateColumns: [fr(1), px(200), repeat(2, minmax(px(40), fr(2)))], gridRow: span(2), gridColumn: { start: 1, end: 3 }, overflow: { x: "clip" }, boxSizing: "content-box" }}
      radius={[px(4), 0, px(4), 0]}
      border={{ top: "#f00" }}
      outline={{ width: px(2), color: "#fff" }}
      shadow={[{ color: "#0008", y: px(4) }, { blur: px(8) }]}
      transform={{ rotation: deg(45), scale: [1, 2], translation: [px(1), percent(2)] }}
      background={() => linearGradient(90, ["#f00", ["#00f", percent(80)]])}
    >
      <ZIndex value={2} />
    </node>
    <node background={[radialGradient(["#fff", "#000"], { shape: { circle: px(4) } }), conicGradient(["#f00", ["#0f0", 90]], { from: 45 })]} />
    {/* @ts-expect-error  a misspelt style key */}
    <node style={{ gridTemplateColums: [fr(1)] }} />
    {/* @ts-expect-error  not a word Bevy has */}
    <node style={{ boxSizing: "padding-box" }} />
    {/* @ts-expect-error  shadow has no offset */}
    <node shadow={{ offset: 3 }} />
  </>
));

/** A newtype's field is `value`, never `"0"`. */
export function readZ(z: ZIndex): number {
  // @ts-expect-error  no field "0"
  z["0"];
  return z.value;
}

export const RichText = () => (
  <text size={20} weight={700} italic justify="center" linebreak="none" lineHeight={1.2} letterSpacing={px(1)} textShadow={{ x: 1 }} underline strikethrough="#f00">
    HP <span color="#f00" textBackground="#000">{() => 3}</span>
    {/* @ts-expect-error  justify is <text>'s, not a span's */}
    <span justify="center">x</span>
  </text>
);
export const Haloed = () => (
  <text color="#fff" textHalo={{ width: 1.5, color: "#000" }}>
    Harbour <span color="#ff0">quay</span>
    {/* @ts-expect-error  textHalo is <text>'s: it outlines every span already */}
    <span textHalo={{ width: 1 }}>x</span>
  </text>
);
// @ts-expect-error  not a justify word
export const BadJustify = () => <text justify="middle">x</text>;

const mapCam = ref();
export const Media = () => (
  <node camera={mapCam}>
    <viewport camera={mapCam} />
    <image src="a.png" flipX rect={[0, 0, 8, 8]} atlas={{ layout: TextureAtlasLayout.fromGrid({ tileSize: [8, 8], columns: 2, rows: 2 }), index: 1 }} mode={sliced({ border: 4, center: { tile: 1 } })} />
    <image src="a.png" mode={tiled()} />
    {/* @ts-expect-error  a viewport needs a camera */}
    <viewport />
    {/* @ts-expect-error  not a mode */}
    <image src="a.png" mode="cover" />
  </node>
);

const [vol, setVol] = signal(0);
const scrolled = ref();
export const Widgets = () => (
  <node scrollArea ref={scrolled} style={{ overflow: { y: "scroll" } }}>
    <checkbox checked={() => vol() > 0} onValueChange={({ value, isFinal }) => setVol(value && isFinal ? 1 : 0)} />
    <checkbox initialChecked />
    <radioGroup onValueChange={({ key, value }) => key === null || value}>
      <radio key="a" />
    </radioGroup>
    <slider value={() => vol()} min={0} max={10} step={1} precision={0} orientation="horizontal" trackClick="snap" onValueChange={({ value }) => setVol(value)}>
      <sliderThumb />
    </slider>
    <scrollbar target={scrolled} orientation="vertical" minThumbLength={8}>
      <scrollbarThumb background="#888" radius={4} style={{ border: 1 }} />
    </scrollbar>
    <tabList initialSelected="a" activation="automatic" onValueChange={({ key }) => key}>
      <tab key="a" />
    </tabList>
    <listBox multiSelect onValueChange={({ key }) => key}>
      <listItem selected={() => true} />
    </listBox>
    {/* @ts-expect-error  controlled and initial: give one */}
    <slider value={() => vol()} initialValue={0} />
    {/* @ts-expect-error  initial props are written at mount: not bound */}
    <checkbox initialChecked={() => true} />
    {/* @ts-expect-error  a thumb has no width: its scrollbar sizes it */}
    <scrollbarThumb style={{ width: 4 }} />
    {/* @ts-expect-error  a scrollbar needs its target */}
    <scrollbar />
  </node>
);

export const Overlays = () => (
  <node onMenuEvent={({ action, source, entity }) => action === "close-all" && source !== entity}>
    <menuButton />
    <popover positions={[{ side: "bottom", align: "center", gap: 2 }, {}]} windowMargin={8}>
      <menuPopup layout="row">
        <menuItem onActivate={({ entity }) => entity} />
      </menuPopup>
    </popover>
    <modalDialogBarrier onRequestClose={({ source }) => source}>
      <dialog modal={() => vol() > 0}>
        <dialogDragHandle />
      </dialog>
    </modalDialogBarrier>
    {/* @ts-expect-error  Bevy's sides are top, bottom, left and right */}
    <popover positions={[{ side: "above" }]} />
    {/* @ts-expect-error  a menu event's actions are Bevy's */}
    <node onMenuEvent={({ action }) => action === "open"} />
    {/* @ts-expect-error  layout is column, row or grid */}
    <menuPopup layout="ring" />
  </node>
);
