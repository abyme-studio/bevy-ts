# What one slot costs the parallel executor (2026-10-20, native task 4.4)

A slot is one exclusive bevy-ts stage system ordered before or after an exposed Rust set. The
multithreaded executor will not start an exclusive system while any other system runs
(`MultiThreadedExecutor::can_run`, `bevy_ecs/src/schedule/executor/multi_threaded.rs:533`), and it
checks that before evaluating run conditions, so even a slot whose condition skips it is a drain
point. That is why native makes slots only for sets a generation names (design decision 7).

## Measurement

`crates/bevy-ts-cli/examples/slot_barrier.rs`, release, process `bench-slot`: one schedule with
eight parallel systems before a set, eight in it and eight after it, each summing 20,000 floats;
2,000 runs after 50 warm-up runs, median of 5. Variants: no slot; an empty exclusive slot after
the set; the same slot skipped by a run condition.

| run | load average | none (µs) | empty slot (µs) | skipped slot (µs) |
|---|---|---|---|---|
| 1 | 35 | 261.5 | 309.5 | 195.6 |
| 2 | 43 | 390.4 | 442.8 | 272.9 |
| 3 | 43 | 347.2 | 341.4 | 317.3 |
| 4 | 43 | 276.6 | 282.1 | 343.3 |

Apple M5 Max, 18 cores, with several agents compiling at the same time (load 35 to 43). The
spread between runs of one variant (276 to 390 µs) is larger than any difference between
variants, so these runs do not resolve the cost of one barrier; they bound it below the noise of
a saturated machine, tens of microseconds at most per schedule run here. Rerun `bench-slot` on a
quiet machine for a number.

## Conclusion

The barrier itself is established from the executor's source, not from this measurement, and the
design already avoids paying it for unused sets. A named set's two slots are the price of
ordering TS against Rust at all; a project pays it only for the sets its TS names.
