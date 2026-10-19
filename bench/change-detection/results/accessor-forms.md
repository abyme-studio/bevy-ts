engine: quickjs-ng, 100000 rows, every row written through 3 setters, ms per run (median)

| form | ms |
|---|---|
| class | 104.79 |
| class+tick | 131.63 |
| class+flag | 120.76 |
| closure | 78.15 |
| closure+tick | 78.97 |
| closure+flag | 76.87 |
| raw | 9.50 |
| raw+tick | 11.10 |

engine: deno v8 15.0.245.2-rusty, 100000 rows, every row written through 3 setters, ms per run (median)

| form | ms |
|---|---|
| class | 31.94 |
| class+tick | 37.80 |
| class+flag | 33.46 |
| closure | 43.37 |
| closure+tick | 53.57 |
| closure+flag | 51.10 |
| raw | 11.01 |
| raw+tick | 13.23 |

engine: deno v8 15.0.245.2-rusty, 100000 rows, every row written through 3 setters, ms per run (median)

| form | ms |
|---|---|
| class | 0.22 |
| class+tick | 0.24 |
| class+flag | 0.23 |
| closure | 25.10 |
| closure+tick | 25.90 |
| closure+flag | 25.42 |
| raw | 0.16 |
| raw+tick | 0.14 |
