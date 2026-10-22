## 1. Baseline

- [ ] 1.1 Measure first-launch time today: a shared cache entry with a fresh engine, project A `bevy-ts dev` to first frame, then project B with the same hash. Record both times, and B's again on a second launch, in `docs/research/engine-cache-link.md`

## 2. Link

- [ ] 2.1 `fetch_shared`: verify the shared file in place, then on Unix create `bevy-ts.tmp` as a symlink to its canonical absolute path and rename it to the local engine path; write the `bevy-ts.link` stamp (SHA-256, target, size, mtime, inode)
- [ ] 2.2 Copy fallback: on Windows, or when `symlink` fails, keep today's copy path and log why linking failed
- [ ] 2.3 `obtain`: a local hit through a link stats the target against the stamp; equal runs; different re-hashes, rewrites the stamp on a match, or removes link and stamp and falls through as a failed verification
- [ ] 2.4 `obtain`: a dangling link is removed with its stamp and is a miss, the why line naming the broken target
- [ ] 2.5 Confirm nothing writes under `engineCache`

## 3. Tests

- [ ] 3.1 Unit: a shared hit installs a link and a stamp; a second project links to the same target
- [ ] 3.2 Unit: a rewritten shared file is refused and unlinked; a touched but equal file runs and restamps
- [ ] 3.3 Unit: a dangling link is a miss and relinks once the entry is back
- [ ] 3.4 Unit: a tampered shared file creates no link (the existing test, updated)
- [ ] 3.5 Unit: a forced link failure copies
- [ ] 3.6 `bevy-ts build` test: a linked local engine ships as a real file

## 4. Docs

- [ ] 4.1 `docs/native.md` "Sharing engines in a team": linked not copied and why; publish by rename, never change an entry; an unreachable share is a miss

## 5. Measure

- [ ] 5.1 Repeat 1.1 with the change: project B's first launch should be near the 0.08 s symlink figure rather than 57-110 s; record before and after in the research note
