/** A Vite (and vitest) plugin compiling TS through bevy-ts. */
export default function bevyTs(): { name: string; enforce: "pre"; transform(code: string, id: string): { code: string; map: null } | null };
