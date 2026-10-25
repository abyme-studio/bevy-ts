Rows from `bench/networking/size.sh`. The first two were built through sccache, which the
machine's cargo config sets: their sizes stand, their times do not.

| label | commit | stripped bytes | clean build s |
| --- | --- | --- | --- |
| before (sccache) | 194293d3 | 159004432 | 289 |
| after (sccache) | 194293d3 + tree | 160375168 | 330 |
| before | 194293d3 | 159004432 | 479 |
| after | 8717725f + tree | 160375168 | 599 |
| before | 194293d3 | 159004432 | 498 |
| after | 8717725f + tree | 160375168 | 612 |
| before | 194293d3 | 159004432 | 485 |
| after | 8717725f + tree | 160375168 | 673 |
