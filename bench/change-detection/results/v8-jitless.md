engine: deno 2.9.7 / v8 15.0.245.2-rusty

### 10,000 rows, ms per run (median)

| pattern | path | 0.3% | 10.0% | 100.0% |
|---|---|---|---|---|
| cond | raw | 0.113 | 0.207 | 1.09 |
| cond | raw+tick | 0.120 | 0.196 | 1.03 |
| cond | raw+tick×3 | 0.120 | 0.202 | 1.15 |
| cond | raw+bit | 0.119 | 0.239 | 1.37 |
| cond | acc | 0.366 | 0.680 | 3.32 |
| cond | acc+tick | 0.377 | 0.709 | 3.40 |
| cond | acc+dirty | 0.336 | 0.517 | 3.16 |
| cond | acc+cmd | 0.292 | 0.740 | 4.19 |
| all | raw | 0.948 | 1.04 | 0.990 |
| all | raw+tick | 0.902 | 0.853 | 0.933 |
| all | raw+neq | 0.996 | 1.02 | 1.41 |
| all | acc | 3.33 | 3.51 | 3.24 |
| all | acc+tick | 2.96 | 3.00 | 2.64 |
| all | acc+dirty | 2.90 | 3.68 | 3.65 |
| all | acc+neq | 3.59 | 3.07 | 3.11 |
| all | acc+cmd | 4.61 | 5.23 | 5.24 |
| post | markAll(fill) | 0.0006 | 0.0006 | 0.0008 |
| post | shadowCopy | 0.0055 | 0.0049 | 0.0050 |
| post | shadowDiff | 1.37 | 1.39 | 1.23 |
| post | bitScan | 0.116 | 0.258 | 1.26 |
| post | applyCmds | 0.297 | 0.904 | 6.57 |

### 100,000 rows, ms per run (median)

| pattern | path | 0.3% | 10.0% | 100.0% |
|---|---|---|---|---|
| cond | raw | 1.20 | 1.78 | 7.90 |
| cond | raw+tick | 0.871 | 1.78 | 8.91 |
| cond | raw+tick×3 | 1.09 | 1.81 | 9.41 |
| cond | raw+bit | 0.901 | 1.71 | 9.61 |
| cond | acc | 2.50 | 4.75 | 22.5 |
| cond | acc+tick | 2.62 | 4.93 | 28.7 |
| cond | acc+dirty | 2.82 | 5.46 | 26.8 |
| cond | acc+cmd | 3.32 | 6.61 | 40.3 |
| all | raw | 8.46 | 8.92 | 8.36 |
| all | raw+tick | 9.46 | 8.78 | 9.75 |
| all | raw+neq | 10.3 | 10.2 | 14.1 |
| all | acc | 29.9 | 58.2 | 106 |
| all | acc+tick | 130 | 82.3 | 43.1 |
| all | acc+dirty | 32.0 | 26.3 | 38.5 |
| all | acc+neq | 37.8 | 30.6 | 31.7 |
| all | acc+cmd | 46.4 | 41.0 | 49.4 |
| post | markAll(fill) | 0.0050 | 0.0049 | 0.0051 |
| post | shadowCopy | 0.058 | 0.061 | 0.077 |
| post | shadowDiff | 17.1 | 17.2 | 16.0 |
| post | bitScan | 1.63 | 3.37 | 19.6 |
| post | applyCmds | 5.00 | 12.4 | 115 |

### 1,000,000 rows, ms per run (median)

| pattern | path | 0.3% | 10.0% | 100.0% |
|---|---|---|---|---|
| cond | raw | 19.9 | 65.7 | 239 |
| cond | raw+tick | 22.4 | 60.1 | 299 |
| cond | raw+tick×3 | 26.7 | 85.9 | 366 |
| cond | raw+bit | 29.9 | 74.2 | 463 |
| cond | acc | 43.7 | 71.3 | 322 |
| cond | acc+tick | 30.6 | 78.5 | 1716 |
| cond | acc+dirty | 114 | 422 | 1117 |
| cond | acc+cmd | 139 | 123 | 565 |
| all | raw | 102 | 109 | 110 |
| all | raw+tick | 88.9 | 95.2 | 554 |
| all | raw+neq | 480 | 379 | 1043 |
| all | acc | 1142 | 1459 | 393 |
| all | acc+tick | 390 | 358 | 270 |
| all | acc+dirty | 327 | 260 | 240 |
| all | acc+neq | 245 | 261 | 341 |
| all | acc+cmd | 416 | 416 | 589 |
| post | markAll(fill) | 0.045 | 0.045 | 0.045 |
| post | shadowCopy | 0.660 | 0.660 | 0.665 |
| post | shadowDiff | 155 | 136 | 252 |
| post | bitScan | 31.0 | 119 | 562 |
| post | applyCmds | 175 | 636 | 2612 |
