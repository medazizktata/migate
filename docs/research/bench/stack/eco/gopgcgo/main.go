// Viability probe: Postgres's own parser in cgo (pg_query_go).
package main

import (
	"fmt"
	"os"
	"time"

	pgquery "github.com/pganalyze/pg_query_go/v6"
)

func main() {
	sql, _ := os.ReadFile(os.Args[1])
	t0 := time.Now()
	tree, err := pgquery.Parse(string(sql))
	if err != nil {
		panic(err)
	}
	fmt.Printf("stmts=%d parse_ms=%.1f\n", len(tree.Stmts), float64(time.Since(t0).Microseconds())/1000)
}
