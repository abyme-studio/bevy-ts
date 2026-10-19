engine: deno 2.9.7 / v8 15.0.245.2-rusty

### 10,000 rows, ms per run (median)

| pattern | path | 0.3% | 10.0% | 100.0% |
|---|---|---|---|---|
| cond | raw | 0.0083 | 0.013 | 0.033 |
| cond | raw+tick | 0.0083 | 0.012 | 0.036 |
| cond | raw+tick×3 | 0.0083 | 0.013 | 0.038 |
| cond | raw+bit | 0.0067 | 0.011 | 0.039 |
| cond | acc | 0.0093 | 0.014 | 0.032 |
| cond | acc+tick | 0.010 | 0.014 | 0.037 |
| cond | acc+dirty | 0.011 | 0.016 | 0.039 |
| cond | acc+cmd | 0.010 | 0.022 | 0.070 |
| all | raw | 0.024 | 0.024 | 0.024 |
| all | raw+tick | 0.024 | 0.026 | 0.026 |
| all | raw+neq | 0.026 | 0.035 | 0.035 |
| all | acc | 0.025 | 0.025 | 0.025 |
| all | acc+tick | 0.029 | 0.029 | 0.029 |
| all | acc+dirty | 0.028 | 0.028 | 0.028 |
| all | acc+neq | 0.056 | 0.065 | 0.061 |
| all | acc+cmd | 0.059 | 0.059 | 0.059 |
| post | markAll(fill) | 0.0006 | 0.0006 | 0.0006 |
| post | shadowCopy | 0.0056 | 0.0056 | 0.0055 |
| post | shadowDiff | 0.054 | 0.055 | 0.054 |
| post | bitScan | 0.0071 | 0.013 | 0.048 |
| post | applyCmds | 0.010 | 0.028 | 0.121 |

### 100,000 rows, ms per run (median)

| pattern | path | 0.3% | 10.0% | 100.0% |
|---|---|---|---|---|
| cond | raw | 0.066 | 0.146 | 0.324 |
| cond | raw+tick | 0.066 | 0.140 | 0.370 |
| cond | raw+tick×3 | 0.066 | 0.143 | 0.378 |
| cond | raw+bit | 0.066 | 0.137 | 0.390 |
| cond | acc | 0.093 | 0.172 | 0.324 |
| cond | acc+tick | 0.101 | 0.184 | 0.373 |
| cond | acc+dirty | 0.114 | 0.188 | 0.347 |
| cond | acc+cmd | 0.084 | 0.182 | 0.443 |
| all | raw | 0.138 | 0.133 | 0.139 |
| all | raw+tick | 0.145 | 0.145 | 0.145 |
| all | raw+neq | 0.171 | 0.254 | 0.193 |
| all | acc | 0.137 | 0.140 | 0.142 |
| all | acc+tick | 0.159 | 0.171 | 0.171 |
| all | acc+dirty | 0.159 | 0.159 | 0.159 |
| all | acc+neq | 0.341 | 0.385 | 0.353 |
| all | acc+cmd | 0.331 | 0.318 | 0.319 |
| post | markAll(fill) | 0.0031 | 0.0031 | 0.0031 |
| post | shadowCopy | 0.043 | 0.042 | 0.041 |
| post | shadowDiff | 0.292 | 0.289 | 0.283 |
| post | bitScan | 0.054 | 0.115 | 0.279 |
| post | applyCmds | 0.052 | 0.155 | 0.689 |

### 1,000,000 rows, ms per run (median)

| pattern | path | 0.3% | 10.0% | 100.0% |
|---|---|---|---|---|
| cond | raw | 0.523 | 1.13 | 1.88 |
| cond | raw+tick | 0.561 | 1.21 | 2.26 |
| cond | raw+tick×3 | 0.507 | 1.15 | 2.27 |
| cond | raw+bit | 0.545 | 1.18 | 2.59 |
| cond | acc | 0.507 | 1.10 | 1.77 |
| cond | acc+tick | 0.676 | 1.24 | 2.15 |
| cond | acc+dirty | 1.08 | 1.63 | 2.55 |
| cond | acc+cmd | 0.547 | 1.45 | 4.39 |
| all | raw | 1.57 | 1.36 | 1.45 |
| all | raw+tick | 2.26 | 2.68 | 3.19 |
| all | raw+neq | 3.09 | 4.37 | 3.88 |
| all | acc | 2.52 | 2.53 | 2.76 |
| all | acc+tick | 3.14 | 3.14 | 3.14 |
| all | acc+dirty | 3.40 | 3.05 | 3.04 |
| all | acc+neq | 6.13 | 6.53 | 6.08 |
| all | acc+cmd | 4.59 | 4.12 | 6.86 |
| post | markAll(fill) | 0.056 | 0.052 | 0.052 |
| post | shadowCopy | 0.631 | 0.635 | 0.623 |
| post | shadowDiff | 5.90 | 4.98 | 7.68 |
| post | bitScan | 0.877 | 2.03 | 5.23 |
| post | applyCmds | 1.16 | 3.17 | 22.1 |
