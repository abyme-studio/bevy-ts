engine: node v26.12.0 / v8 14.6.202.34-node.39

### 10,000 rows, ms per run (median)

| pattern | path | 0.3% | 10.0% | 100.0% |
|---|---|---|---|---|
| cond | raw | 0.0050 | 0.0083 | 0.024 |
| cond | raw+tick | 0.0062 | 0.0094 | 0.026 |
| cond | raw+tick×3 | 0.0050 | 0.010 | 0.031 |
| cond | raw+bit | 0.0051 | 0.014 | 0.033 |
| cond | acc | 0.0077 | 0.011 | 0.029 |
| cond | acc+tick | 0.0075 | 0.012 | 0.034 |
| cond | acc+dirty | 0.011 | 0.017 | 0.031 |
| cond | acc+cmd | 0.0082 | 0.014 | 0.062 |
| all | raw | 0.023 | 0.023 | 0.024 |
| all | raw+tick | 0.026 | 0.024 | 0.027 |
| all | raw+neq | 0.030 | 0.039 | 0.034 |
| all | acc | 0.024 | 0.024 | 0.024 |
| all | acc+tick | 0.030 | 0.030 | 0.030 |
| all | acc+dirty | 0.028 | 0.027 | 0.027 |
| all | acc+neq | 0.032 | 0.043 | 0.039 |
| all | acc+cmd | 0.052 | 0.056 | 0.055 |
| post | markAll(fill) | 0.0027 | 0.0026 | 0.0026 |
| post | shadowCopy | 0.0059 | 0.0059 | 0.0059 |
| post | shadowDiff | 0.044 | 0.039 | 0.039 |
| post | bitScan | 0.0079 | 0.013 | 0.043 |
| post | applyCmds | 0.0077 | 0.021 | 0.106 |

### 100,000 rows, ms per run (median)

| pattern | path | 0.3% | 10.0% | 100.0% |
|---|---|---|---|---|
| cond | raw | 0.074 | 0.126 | 0.269 |
| cond | raw+tick | 0.050 | 0.146 | 0.279 |
| cond | raw+tick×3 | 0.058 | 0.093 | 0.307 |
| cond | raw+bit | 0.051 | 0.139 | 0.260 |
| cond | acc | 0.058 | 0.128 | 0.247 |
| cond | acc+tick | 0.063 | 0.121 | 0.273 |
| cond | acc+dirty | 0.090 | 0.194 | 0.348 |
| cond | acc+cmd | 0.087 | 0.169 | 0.634 |
| all | raw | 0.187 | 0.188 | 0.187 |
| all | raw+tick | 0.208 | 0.191 | 0.229 |
| all | raw+neq | 0.255 | 0.326 | 0.275 |
| all | acc | 0.182 | 0.206 | 0.230 |
| all | acc+tick | 0.286 | 0.314 | 0.340 |
| all | acc+dirty | 0.322 | 0.322 | 0.294 |
| all | acc+neq | 0.315 | 0.395 | 0.348 |
| all | acc+cmd | 0.412 | 0.414 | 0.435 |
| post | markAll(fill) | 0.029 | 0.027 | 0.028 |
| post | shadowCopy | 0.046 | 0.043 | 0.047 |
| post | shadowDiff | 0.305 | 0.336 | 0.409 |
| post | bitScan | 0.058 | 0.184 | 0.455 |
| post | applyCmds | 0.085 | 0.250 | 1.25 |

### 1,000,000 rows, ms per run (median)

| pattern | path | 0.3% | 10.0% | 100.0% |
|---|---|---|---|---|
| cond | raw | 0.761 | 1.30 | 2.84 |
| cond | raw+tick | 0.744 | 1.65 | 2.84 |
| cond | raw+tick×3 | 0.588 | 1.34 | 3.28 |
| cond | raw+bit | 0.570 | 1.22 | 2.51 |
| cond | acc | 0.699 | 1.23 | 2.18 |
| cond | acc+tick | 0.528 | 1.27 | 2.71 |
| cond | acc+dirty | 0.730 | 1.39 | 2.61 |
| cond | acc+cmd | 0.571 | 1.52 | 5.34 |
| all | raw | 1.88 | 1.90 | 2.02 |
| all | raw+tick | 2.18 | 2.18 | 2.29 |
| all | raw+neq | 2.41 | 5.17 | 4.44 |
| all | acc | 2.09 | 2.44 | 2.30 |
| all | acc+tick | 2.13 | 2.14 | 3.40 |
| all | acc+dirty | 3.24 | 3.22 | 3.52 |
| all | acc+neq | 3.38 | 4.85 | 4.83 |
| all | acc+cmd | 7.21 | 7.15 | 6.79 |
| post | markAll(fill) | 0.314 | 0.317 | 0.314 |
| post | shadowCopy | 0.629 | 0.664 | 0.652 |
| post | shadowDiff | 6.08 | 5.73 | 6.01 |
| post | bitScan | 0.854 | 1.90 | 5.57 |
| post | applyCmds | 1.20 | 3.46 | 13.0 |
