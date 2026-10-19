// `bevy-ts/jsx-runtime`: the automatic JSX runtime's resolution target. What elements do
// belongs to the TSX change; here they are plain descriptions.
export declare const Fragment: unique symbol;
export interface Element {
  type: unknown;
  props: Record<string, unknown>;
  key: string | number | null;
}
export declare function jsx(type: unknown, props: Record<string, unknown> | null, key?: string | number): Element;
export declare const jsxs: typeof jsx;
export declare namespace JSX {
  type Element = import("./jsx-runtime").Element;
  interface IntrinsicElements {
    [name: string]: Record<string, unknown>;
  }
}
