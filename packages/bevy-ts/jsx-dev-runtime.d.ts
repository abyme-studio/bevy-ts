export { Fragment, JSX } from "./jsx-runtime";
import type { Element } from "./jsx-runtime";
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
): Element & { source: Source | null };
