N=1000000, 60 runs, Deno/2.9.7
| form | median ms | p90 ms | heap growth KB over runs |
|---|---|---|---|
| A each(cb) | 2.182 | 2.535 | 16 |
| B for-of destructured | 5.336 | 5.843 | 694 |
| B' for-of, no destructure | 5.212 | 5.937 | 985 |
| A+ each, glam-like addScaled | 5.421 | 5.777 | 16 |
| D raw arrays | 0.582 | 0.619 | 496 |

N=10000, 60 runs, Deno/2.9.7
| form | median ms | p90 ms | heap growth KB over runs |
|---|---|---|---|
| A each(cb) | 0.020 | 0.123 | 859 |
| B for-of destructured | 0.051 | 0.065 | 753 |
| B' for-of, no destructure | 0.052 | 0.154 | 78 |
| A+ each, glam-like addScaled | 0.053 | 0.065 | 16 |
| D raw arrays | 0.016 | 0.018 | 23 |

jitless:
N=100000, 60 runs, Deno/2.9.7
| form | median ms | p90 ms | heap growth KB over runs |
|---|---|---|---|
| A each(cb) | 11.684 | 12.051 | 436 |
| B for-of destructured | 21.207 | 21.760 | 627 |
| B' for-of, no destructure | 13.747 | 14.245 | 183 |
| A+ each, glam-like addScaled | 12.826 | 13.192 | 785 |
| D raw arrays | 2.972 | 3.151 | 393 |
