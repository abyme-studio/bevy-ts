// `bevy-ts/jsx-runtime`: the resolution target of the automatic JSX transform. Elements are
// plain descriptions; what they do belongs to the TSX change.
export const Fragment = Symbol.for("bevy-ts.fragment");

export function jsx(type, props, key) {
  return { type, props: props || {}, key: key === undefined ? null : key };
}

export const jsxs = jsx;
