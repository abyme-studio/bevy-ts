# Touched-path reconcile: one write's time

Change `touched-path-reconcile`. A level write reconciles only the paths the batch touched, not
the whole level. This is what it costs, by step, at 2,000 and 20,000 entities (1,000 and 10,000
`Grunt` instances, each with an eye).

Bench: `bench/edit-split` (`abyme call processes start name=edit-split-bench`), 3 runs, each the
median of its writes. Medians of the runs, ms. Raw runs: `bench/edit-split/results/`.

## Before: main, at d038fd08d

`results/today.txt`, load 70 to 90.

| entities | move | prop | add | delete | rename |
|---:|---:|---:|---:|---:|---:|
| 2,000 | 437 | 1008 | 231 | 360 | 869 |
| 20,000 | 5606 | 2677 | 3585 | 3314 | 6727 |

## After: whole against partial, one build

`results/partial.txt`, load 8 to 50. The `whole` arm is the same build with `PartialEdits(false)`:
every write takes the whole path. The engine changes speed it up too: the stamp sweeps walk only
under the units, and the kept tree is no longer cloned deep.

| entities | arm | write | total | writer | parse | shift | compose | expand | reconcile | stamp | other |
|---:|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| 2000 | partial | move | 3.6 | 2.3 | 0.3 | 0.0 | 0.2 | 0.1 | 0.1 | 0.0 | 0.5 |
| 2000 | partial | prop | 3.4 | 2.2 | 0.3 | 0.0 | 0.2 | 0.1 | 0.0 | 0.0 | 0.5 |
| 2000 | partial | add | 3.2 | 2.2 | 0.3 | 0.0 | 0.2 | 0.1 | 0.1 | 0.0 | 0.5 |
| 2000 | partial | delete | 4.0 | 2.2 | 0.3 | 0.7 | 0.2 | 0.0 | 0.0 | 0.0 | 0.5 |
| 2000 | partial | rename | 3.6 | 2.2 | 0.3 | 0.2 | 0.2 | 0.1 | 0.0 | 0.0 | 0.5 |
| 2000 | whole | move | 23.3 | 2.6 | 0.1 | 0.0 | 0.0 | 0.0 | 19.5 | 0.0 | 0.7 |
| 2000 | whole | delete | 23.2 | 2.5 | 0.1 | 0.0 | 0.0 | 0.0 | 20.2 | 0.0 | 0.5 |
| 20000 | partial | move | 36.3 | 28.0 | 2.7 | 0.0 | 2.2 | 0.2 | 0.2 | 0.0 | 2.8 |
| 20000 | partial | prop | 36.0 | 28.0 | 2.7 | 0.0 | 2.0 | 0.2 | 0.2 | 0.1 | 2.7 |
| 20000 | partial | add | 36.6 | 28.2 | 2.7 | 0.0 | 2.0 | 0.2 | 0.4 | 0.2 | 2.8 |
| 20000 | partial | delete | 48.0 | 28.7 | 2.7 | 10.7 | 2.4 | 0.0 | 0.3 | 0.0 | 2.8 |
| 20000 | partial | rename | 41.4 | 28.8 | 2.8 | 3.2 | 2.6 | 0.2 | 0.3 | 0.1 | 3.6 |
| 20000 | whole | move | 205.9 | 28.0 | 1.1 | 0.0 | 0.0 | 0.0 | 174.0 | 0.0 | 2.4 |
| 20000 | whole | delete | 251.5 | 28.9 | 1.1 | 0.0 | 0.0 | 0.0 | 216.9 | 0.0 | 2.3 |

In the whole arm, compose, expand and reconcile are all one step, `reconcile`. Every write of every
kind took the partial path: the bench prints a `WHOLE` line with its reason for any that did not,
and there were none.

## Against the targets

| target | result |
|---|---|
| a write under 50 ms at 20,000 entities | met: 36 to 48 |
| a write under 8 ms at 2,000 | met: 3.2 to 4.0 |
| expand under 2 ms at 20,000 | met: 0.2 |
| reconcile under 2 ms at 20,000 | met: 0.2 to 0.4 |
| compose under 2 ms at 20,000 | just missed: 2.0 to 2.6 |

- Compose still walks every sibling of a unit to find its index and key. Untouched siblings are bare
  stubs, but there are 10,000 of them at the top level.
- The writer is most of what is left: 28 ms at 20,000. The design puts it out of scope.
- A delete near the top moves the lines of everything below it. `shift` moves each of those
  entities' places (`TsxNode`, `LevelInstance`, the root's maps): 10.7 ms at 20,000.
- A first pass, before stubs were made bare and places were moved in place, measured compose 9 ms
  and a delete's shift 27 ms at 20,000 (load 60 to 100).
