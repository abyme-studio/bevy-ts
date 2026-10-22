## Context

`obtain` (engine.rs) looks in `.bevy-ts/engines/<hash>/`, then the shared directory `engineCache` names, then builds. `fetch_shared` copies the shared binary into a temporary directory, checks its SHA-256 against `bevy-ts.sha256` beside the source, and `install`s it as a new file. Each project therefore runs its own new path, and macOS scans it on first launch: 57-110 s measured here, serialised across launches. A symlink to a path already scanned is not scanned again (0.08 s); hard links and APFS clones are.

The copy's job today is integrity: the checksum is taken on the bytes the project will run. With a link those bytes are the shared file's, so the check has to follow them.

## Goals / Non-Goals

**Goals:**
- A second project on the same machine starting the same shared engine pays no scan.
- No binary runs that has not matched its recorded SHA-256 since it last changed.
- A broken link behaves like a miss, never like a crash.

**Non-Goals:**
- Linking cargo-built engines across projects. A build is a new binary and pays its scan once anyway; publishing built engines to a cache is the absent publish command, out of scope.
- A machine-wide cache for projects with no `engineCache`.
- Authenticity. The checksum still proves integrity only.

## Decisions

**Link by absolute path to the shared file.** `engineCache` may be relative to the project; the link target is the canonical absolute path of `<cache>/<hash>/<target>/bevy-ts`. The link lives at the local engine path (`.bevy-ts/engines/<hash>/bevy-ts`), so callers keep one stable path. It is created as `bevy-ts.tmp` and renamed into place, as `install` does for files.

**A stamp detects change, the SHA-256 decides it.** Hashing a 100+ MB engine on every start costs a few hundred ms, more over a share. So on link bevy-ts verifies the shared file and writes `bevy-ts.link` beside the link: the SHA-256, the target path, and the target's size, modification time and inode (on Windows the copy fallback has no stamp). On a local hit through a link, `obtain` stats the target. Same stamp: run. Different stamp: hash the target against the SHA-256 recorded in the stamp; equal, rewrite the stamp and run; different, remove the link and stamp, report the mismatch, and treat the hash as a miss whose shared copy failed verification (today's path). A local hit on a real file stays as it is.

Rejected: re-hashing on every start (cost above); trusting the link with no stamp (a rewritten shared file would run unverified).

**Dangling means miss.** A link whose target does not exist is removed with its stamp, and `obtain` goes on to the shared cache and the build, adding "the link to <target> was broken" to its why line. If the shared entry is back, it is linked again.

**Copy where a link cannot be made.** On Windows bevy-ts copies, as today: there is no syspolicyd, and symlinks need Developer Mode or a privilege. On Unix, if `symlink` fails (a filesystem without links, permissions), bevy-ts copies and logs the error once per fetch. The copy path is today's `fetch_shared` unchanged.

**The shared entry is immutable.** bevy-ts never writes under `engineCache`. Entries are content-addressed by hash, so a correct entry never needs to change. The docs say to publish by writing `<hash>.tmp/` and renaming it to `<hash>/`. A rewrite in place would make every linked project rescan, and could swap the binary under a running engine; the stamp catches it on the next start.

**`bevy-ts build` keeps copying.** It reads the engine through the local path; `std::fs::copy` follows the link and writes a real file into the game folder. A test pins it.

**`current_exe` resolves through the link.** `play::beside_exe` canonicalises the running engine's path, so a linked dev engine looks for `game/game.json` beside the shared file, not the project's. A shared cache entry never holds a game folder, so nothing changes; noted so no one adds one.

Rejected alternative: copy the shared entry once into a per-user machine cache (`~/Library/Caches/bevy-ts/engines/`) and link projects to that. It keeps running offline and never executes from a share, but adds a new home directory to manage and clean. Kept as the next step if linking to network shares proves slow or flaky.

## Risks / Trade-offs

- [Offline] A project linked to a network share cannot start its engine while the share is unmounted; it falls back to a build or fails without a toolchain → documented; the machine cache above is the fix if it bites.
- [Executing from a share] The engine's pages are read over the network on launch → measured in the tasks; most teams unpack CI artefacts locally.
- [Stamp blind spot] A rewrite that keeps size, mtime and inode passes the stat check → only possible by deliberately restoring mtime in place; integrity, not authenticity, was always the claim.
- [Deleted entry under a running engine] Unix keeps the running inode; the next start sees a dangling link and misses.

## Migration Plan

Existing local entries are real files and stay valid local hits. Only new shared-cache fetches link. Rollback is reverting the change; links left behind still resolve, or miss when dangling.

## Open Questions

None.
