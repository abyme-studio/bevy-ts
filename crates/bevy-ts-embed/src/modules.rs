//! What a compiled module is on disk: its inline source map, and a `.wgsl` import's module. Shared
//! by the compiler, `bevy-ts build` and a shipped game's reader.

use std::path::Path;

/// Decodes standard base64, padding optional; other bytes are skipped.
pub fn decode_base64(s: &str) -> Vec<u8> {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = Vec::with_capacity(s.len() / 4 * 3);
    let (mut acc, mut bits) = (0u32, 0);
    for v in s.bytes().filter_map(|c| T.iter().position(|t| *t == c)) {
        acc = (acc << 6 | v as u32) & 0xffffff;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
        }
    }
    out
}

/// The source map a compiled module carries inline, if any.
pub fn inline_source_map(code: &str) -> Option<String> {
    let (_, b64) = code.rsplit_once("//# sourceMappingURL=data:application/json;base64,")?;
    String::from_utf8(decode_base64(b64.trim())).ok()
}

/// The extension of a shader file a module imports (script-runtime: a `.wgsl` import).
pub const SHADER_EXT: &str = ".wgsl";

/// A `.wgsl` import's synthetic module: its default export is an opaque shader-file value naming
/// the file. The WGSL stays on the host; script materials read it there.
pub fn shader_module(path: &Path) -> String {
    let p = serde_json::to_string(&path.to_string_lossy()).expect("a string");
    format!("export default Object.freeze({{ kind: \"shaderFile\", path: {p} }});\n")
}
