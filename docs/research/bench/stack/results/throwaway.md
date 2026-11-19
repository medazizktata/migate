| Command | Mean [ms] | Min [ms] | Max [ms] | Relative |
|:---|---:|---:|---:|---:|
| `sqlite3 :memory: apply schema` | 28.6 ± 18.6 | 17.9 | 80.1 | 1.00 |
| `pg native initdb+start+select1+stop` | 1232.9 ± 144.8 | 1031.6 | 1481.6 | 43.07 ± 28.38 |
| `pg native initdb+start+schema+stop` | 2078.1 ± 398.9 | 1579.4 | 2738.6 | 72.60 ± 49.09 |
| `pg native clone+start+select1+stop` | 670.4 ± 124.7 | 539.9 | 873.6 | 23.42 ± 15.80 |
| `pg native clone+start+schema+stop` | 1355.1 ± 314.7 | 856.9 | 1824.9 | 47.34 ± 32.60 |
| `pglite node start+select1` | 1356.5 ± 259.5 | 1134.1 | 1856.9 | 47.39 ± 32.04 |
| `pglite node start+schema` | 1492.1 ± 226.9 | 1335.8 | 2108.9 | 52.13 ± 34.71 |
| `pglite bun start+schema` | 1538.0 ± 212.8 | 1315.2 | 1907.0 | 53.73 ± 35.62 |
| `pglite-socket node start+psql select1` | 1319.9 ± 223.2 | 1090.1 | 1746.7 | 46.11 ± 30.90 |
| `pglite-socket node start+psql schema` | 1696.3 ± 435.7 | 1413.4 | 2864.4 | 59.26 ± 41.33 |
| `docker pg18 start+select1+kill` | 1224.7 ± 101.0 | 1084.3 | 1385.2 | 42.79 ± 27.96 |
| `docker pg18 start+schema+kill` | 1464.2 ± 110.4 | 1342.7 | 1625.4 | 51.16 ± 33.39 |
