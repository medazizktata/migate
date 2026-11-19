| Command | Mean [ms] | Min [ms] | Max [ms] | Relative |
|:---|---:|---:|---:|---:|
| `go pg_query_go (cgo)` | 20.5 ± 1.3 | 19.2 | 23.7 | 1.23 ± 0.11 |
| `go wasilibs/go-pgquery (CGO off)` | 966.7 ± 89.5 | 893.2 | 1180.4 | 58.08 ± 6.46 |
| `node libpg-query (WASM)` | 232.5 ± 10.2 | 217.2 | 258.5 | 13.97 ± 1.06 |
| `rust pg_query.rs + sqlparser-rs` | 16.6 ± 1.0 | 15.0 | 18.3 | 1.00 |
