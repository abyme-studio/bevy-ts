// `bevy-ts/jsx-dev-runtime`: as `bevy-ts/jsx-runtime`, plus the TS source of each element.
export { Fragment, JSX } from "bevy-ts/jsx-runtime";
import type { Element } from "bevy-ts/jsx-runtime";
export interface Source {
  fileName: string;
  lineNumber: number;
  columnNumber: number;
}
export declare function jsxDEV(
  type: unknown,
  props: Record<string, unknown> | null,
  key: string | number | undefined,
  isStatic: boolean,
  source?: Source,
  self?: unknown,
): Element;
