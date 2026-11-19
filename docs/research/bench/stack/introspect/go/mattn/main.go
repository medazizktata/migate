package main

import (
	"database/sql"
	"fmt"
	"os"
	"time"

	"stack/core"
	_ "github.com/mattn/go-sqlite3"
)

func main() {
	t0 := time.Now()
	db, err := sql.Open("sqlite3", "file:"+os.Args[1]+"?mode=ro")
	if err != nil {
		panic(err)
	}
	db.SetMaxOpenConns(1)
	fmt.Println(core.Introspect(db), fmt.Sprintf("introspect_ms=%.2f", float64(time.Since(t0).Microseconds())/1000))
}
