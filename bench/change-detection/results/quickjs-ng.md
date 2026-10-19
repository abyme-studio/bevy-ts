engine: quickjs-ng

### 10000 rows, ms per run (median)

| pattern | path | 0.3% | 10.0% | 100.0% |
|---|---|---|---|---|
| cond | raw | 0.128 | 0.213 | 1.03 |
| cond | raw+tick | 0.127 | 0.238 | 1.24 |
| cond | raw+tick×3 | 0.153 | 0.290 | 1.66 |
| cond | raw+bit | 0.172 | 0.301 | 1.39 |
| cond | acc | 0.465 | 0.897 | 4.63 |
| cond | acc+tick | 0.407 | 0.877 | 4.89 |
| cond | acc+dirty | 0.435 | 0.853 | 4.90 |
| cond | acc+cmd | 0.533 | 1.47 | 8.28 |
| all | raw | 0.763 | 0.792 | 0.792 |
| all | raw+tick | 0.962 | 1.01 | 1.14 |
| all | raw+neq | 1.48 | 1.58 | 2.33 |
| all | acc | 3.75 | 3.72 | 3.94 |
| all | acc+tick | 6.02 | 6.09 | 6.33 |
| all | acc+dirty | 5.52 | 4.33 | 4.62 |
| all | acc+neq | 4.71 | 4.91 | 5.95 |
| all | acc+cmd | 9.66 | 9.38 | 8.33 |
| post | markAll(fill) | 0.0030 | 0.0030 | 0.0030 |
| post | shadowCopy | 0.0050 | 0.0050 | 0.0040 |
| post | shadowDiff | 1.46 | 1.66 | 1.92 |
| post | bitScan | 0.172 | 0.348 | 1.63 |
| post | applyCmds | 0.410 | 1.25 | 9.74 |

### 100000 rows, ms per run (median)

| pattern | path | 0.3% | 10.0% | 100.0% |
|---|---|---|---|---|
| cond | raw | 1.17 | 2.38 | 8.29 |
| cond | raw+tick | 1.16 | 2.08 | 8.93 |
| cond | raw+tick×3 | 1.20 | 2.40 | 12.8 |
| cond | raw+bit | 1.14 | 2.00 | 9.73 |
| cond | acc | 3.66 | 6.82 | 36.7 |
| cond | acc+tick | 4.06 | 8.44 | 50.5 |
| cond | acc+dirty | 4.07 | 8.57 | 50.6 |
| cond | acc+cmd | 4.39 | 12.2 | 79.6 |
| all | raw | 8.55 | 8.42 | 7.39 |
| all | raw+tick | 10.8 | 11.4 | 9.95 |
| all | raw+neq | 15.3 | 22.0 | 62.3 |
| all | acc | 120 | 53.9 | 40.5 |
| all | acc+tick | 44.6 | 64.6 | 59.6 |
| all | acc+dirty | 45.8 | 42.7 | 46.9 |
| all | acc+neq | 36.5 | 59.0 | 76.4 |
| all | acc+cmd | 128 | 112 | 110 |
| post | markAll(fill) | 0.031 | 0.031 | 0.031 |
| post | shadowCopy | 0.059 | 0.062 | 0.062 |
| post | shadowDiff | 21.8 | 41.0 | 27.1 |
| post | bitScan | 2.16 | 4.35 | 34.3 |
| post | applyCmds | 6.45 | 43.6 | 260 |

### 1000000 rows, ms per run (median)

| pattern | path | 0.3% | 10.0% | 100.0% |
|---|---|---|---|---|
| cond | raw | 33.8 | 54.8 | 278 |
| cond | raw+tick | 45.1 | 97.1 | 357 |
| cond | raw+tick×3 | 48.1 | 76.2 | 255 |
| cond | raw+bit | 17.8 | 28.7 | 117 |
| cond | acc | 39.7 | 105 | 2211 |
| cond | acc+tick | 310 | 469 | 915 |
| cond | acc+dirty | 58.0 | 99.0 | 600 |
| cond | acc+cmd | 45.7 | 137 | 3523 |
| all | raw | 385 | 415 | 278 |
| all | raw+tick | 134 | 128 | 115 |
| all | raw+neq | 127 | 106 | 164 |
| all | acc | 369 | 323 | 339 |
| all | acc+tick | 444 | 500 | 463 |
| all | acc+dirty | 612 | 616 | 848 |
| all | acc+neq | 1918 | 1729 | 632 |
| all | acc+cmd | 1618 | 2095 | 963 |
| post | markAll(fill) | 0.277 | 0.388 | 0.359 |
| post | shadowCopy | 0.653 | 0.708 | 0.654 |
| post | shadowDiff | 182 | 205 | 181 |
| post | bitScan | 19.5 | 41.7 | 223 |
| post | applyCmds | 62.5 | 139 | 852 |
