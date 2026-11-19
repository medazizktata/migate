| Command | Mean [s] | Min [s] | Max [s] | Relative |
|:---|---:|---:|---:|---:|
| `pg clone + node-pg-migrate up (100 tables) + stop` | 1.060 ± 0.155 | 0.966 | 1.485 | 1.27 ± 0.21 |
| `pg clone + psql -f (100 tables) + stop` | 0.834 ± 0.064 | 0.764 | 0.968 | 1.00 |
