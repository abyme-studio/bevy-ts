## MODIFIED Requirements

### Requirement: A shared engine cache
`bevy-ts.json` MAY name a shared engine cache, a directory laid out as `<hash>/<target>/` (a network share, or a CI artefact a team unpacks). On a local miss bevy-ts SHALL try the shared cache before building, SHALL verify a binary against the SHA-256 recorded next to it, and SHALL NOT execute a binary that fails verification. The checksum proves integrity, not authenticity; the docs SHALL say so. bevy-ts SHALL NOT write under the shared cache, and the docs SHALL say an entry is published by renaming a complete directory into place and never changed after. An HTTP cache and a publish command are out of scope for this change. Before any build, `bevy-ts dev` SHALL print the engine hash, why the caches missed and what will compile.

#### Scenario: A gameplay author without Rust
- **WHEN** CI has placed the engine for the current `Cargo.lock` and native sources in the shared directory, and an author with no Rust toolchain runs `bevy-ts dev`
- **THEN** the engine is fetched from the shared cache, verified and started, and no build is attempted

#### Scenario: A tampered download
- **WHEN** the shared binary's SHA-256 does not match its recorded checksum
- **THEN** bevy-ts creates no local link or copy for it, reports the mismatch and does not run it

## ADDED Requirements

### Requirement: A shared engine is linked, not copied
Where the platform allows, a verified shared-cache engine SHALL be installed as a symbolic link at the local engine path (`.bevy-ts/engines/<hash>/<exe>`) to the shared file's absolute path, so every project on a machine runs one path per engine hash. Beside the link bevy-ts SHALL record a stamp: the SHA-256 and the target's size, modification time and inode. On a local hit through a link bevy-ts SHALL compare the target's metadata with the stamp and, when it differs, SHALL hash the target and run it only if the SHA-256 matches. A link whose target is missing SHALL be removed and treated as a local miss. On Windows, or where creating the link fails, bevy-ts SHALL copy the engine as a real file, as before. Engines built with `cargo` SHALL be installed as real files, and `bevy-ts build` SHALL ship a real file, never a link.

#### Scenario: A second project on the same machine
- **WHEN** two projects with the same engine hash and the same `engineCache` each miss locally
- **THEN** both local engines are links to the same shared file, and the second starts without a first-launch scan

#### Scenario: The shared file changed after linking
- **WHEN** the shared file behind a link no longer matches the stamp and its SHA-256 differs from the recorded one
- **THEN** bevy-ts removes the link and stamp, reports the mismatch, does not run it, and goes on as a miss whose shared copy failed verification

#### Scenario: The shared file was touched but not changed
- **WHEN** the shared file's modification time differs from the stamp and its SHA-256 still matches
- **THEN** bevy-ts rewrites the stamp and runs the engine through the link

#### Scenario: A dangling link
- **WHEN** the share is unmounted or the entry was deleted, so the local link's target is missing
- **THEN** bevy-ts removes the link, says it was broken in the miss line, and tries the shared cache and then a build

#### Scenario: No symbolic links
- **WHEN** bevy-ts runs on Windows, or creating the link fails
- **THEN** the verified engine is copied into the local cache as a real file and runs as before

#### Scenario: Shipping a linked engine
- **WHEN** `bevy-ts build` runs in a project whose local release engine is a link
- **THEN** the game folder holds a real copy of the engine, not a link
