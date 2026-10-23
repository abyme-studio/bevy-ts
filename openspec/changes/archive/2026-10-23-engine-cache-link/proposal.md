## Why

macOS syspolicyd scans every new executable path before its first launch, one at a time. On this machine a fresh binary waits 57-110 s. A symlink to a binary already scanned starts in 0.08 s; hard links and APFS clones are new paths and are scanned again. Today `engine::fetch_shared` copies the shared-cache engine into each project's `.bevy-ts/engines/<hash>/`, so every project pays its own scan for a binary the machine has already seen.

## What Changes

- **A shared-cache hit links, it does not copy.** `.bevy-ts/engines/<hash>/bevy-ts` becomes a symlink to `<engineCache>/<hash>/<target>/bevy-ts`, by absolute path. The local path stays where it is, so the supervisor, logs and `bevy-ts/hello` see no difference. The scan is paid once per machine per engine hash.
- **The checksum guards the shared file.** The link is made only after the shared file matches its recorded SHA-256. A stamp beside the link (`bevy-ts.link`) records the SHA-256 and the target's size, modification time and inode. Every local hit through a link stats the target; a differing stamp re-hashes it, and a mismatch removes the link and refuses to run it.
- **A broken link is a local miss.** A dangling link (share unmounted, entry deleted) goes back through the shared cache, then a build, as a miss does today, and says the link was broken.
- **Copy where linking fails.** Windows (no syspolicyd, and symlinks need a privilege) copies as today. On any platform where creating the link fails, bevy-ts copies and logs why once.
- **The shared cache is never rewritten in place.** bevy-ts never writes into it. The docs say an entry is published by writing a new directory and renaming it into place, and never changed after: a rewrite would be rescanned and would change the binary under running projects.
- **Unchanged:** cargo-built engines are installed as real files in the local cache; `bevy-ts build` copies the engine into the game folder as a real file, since it ships.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `project-engine`: a shared-cache hit links the local engine to the verified shared file; the stamp, broken links, the copy fallback and the never-rewritten rule.

## Impact

- `crates/bevy-ts-cli/src/engine.rs`: `fetch_shared` links instead of copying; `obtain`'s local hit checks a link's stamp and treats a dangling link as a miss; tests.
- `crates/bevy-ts-cli/src/build.rs`: no change; a test that the shipped engine is a real file when the local engine is a link.
- `docs/native.md`: "Sharing engines in a team" says entries are linked, are never changed after publishing, and that an unreachable share means a miss.
- Offline cost: a project linked to a network share no longer runs its engine while the share is unmounted. It falls back to a build, or fails without a toolchain.
- No new dependency, service, AI prompt or data access.
