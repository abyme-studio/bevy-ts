# Burst of 2,000 1 KiB messages in one frame

Headless app (MinimalPlugins), V8 backend, fake transport; the script JSON-parses every message.
`cargo test --release -p bevy-ts-v8 --test net burst_bench -- --ignored --nocapture`, 8 rounds.

| frame | median ms | all ms |
| --- | --- | --- |
| idle | 0.267 | [0.287, 0.231, 0.198, 0.097, 0.267, 0.290, 0.194, 0.384] |
| 2,000 × 1 KiB | 3.001 | [4.811, 3.548, 2.255, 2.809, 2.812, 3.001, 3.061, 2.934] |

The burst's main-thread cost over an idle frame: 2.734 ms.
