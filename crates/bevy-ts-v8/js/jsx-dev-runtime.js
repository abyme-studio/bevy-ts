// `bevy-ts/jsx-dev-runtime`: as `bevy-ts/jsx-runtime`, plus the TS source of each element.
export { Fragment } from "bevy-ts/jsx-runtime";

export function jsxDEV(type, props, key, isStatic, source, self) {
  return { type, props: props || {}, key: key === undefined ? null : key, source: source || null };
}
