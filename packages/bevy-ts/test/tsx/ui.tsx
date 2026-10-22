// Type tests for the UI intrinsics' props: `tsc -p test` must pass, and every `@ts-expect-error`
// must be an error.
import { app, conicGradient, deg, fr, linearGradient, minmax, percent, px, radialGradient, repeat, span } from "bevy-ts";
import { ZIndex } from "bevy-ts/bevy";

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
