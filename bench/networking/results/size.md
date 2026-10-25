# The release engine's size and build time, script-networking-2

`bench/networking/size.sh <label> [rev]`, process `net2-size-arms`: the release prebuilt engine
(`cargo static build --release -p bevy-ts-cli`) built from nothing with no sccache, then stripped.
"before" is 194293d3 (stage 1 and the proposal), "after" this change. Three runs an arm, in turn,
on an Apple silicon Mac shared with other agents' builds. Every run is in `size-runs.md`.

| | before | after | delta |
| --- | --- | --- | --- |
| stripped bytes | 159,004,432 | 160,375,168 | +1,370,736 (+0.86%) |
| clean build s (3 runs) | 479, 498, 485 (mean 487) | 599, 612, 673 (mean 628) | +141 |
| CPU s, last run (cargo timings) | 4,493 | 6,736 | +2,243 |
| CPU s, the 20 new crates | | 40 | |

The size is the same in every run. The build time is not the change's: crates it does not touch
took about twice the CPU in the after runs (`wesl` 142 s to 293 s, `bevy_render` 118 s to 251 s),
so the machine was busier then. What the change adds is its 20 crates, about 40 s of CPU: TLS
about 36 s (`rustls` 10, `ring` 8 with its build script's C, `rustls-webpki` 5,
`security-framework` and `-sys` 6, `core-foundation`, `subtle`, `getrandom` and the rest), the
HTTP client about 4 s (`want`, `try-lock`; `hyper`, `http`, `http-body-util`, `smol-hyper` and
`url` were in the build already, `hyper` gaining its `client` feature). They build between about
140 s and 210 s, alongside Bevy; `bevy-ts` waits on Bevy until 330 s or later, so they do not
lengthen the critical path. The size is not split between TLS and the HTTP client: that takes a
build with one and not the other.

Not measured: the time on a quiet machine. Rerun `size.sh` there to read the wall delta alone.
