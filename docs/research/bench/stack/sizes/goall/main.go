// Size probe: one binary linking a driver for each of SQLite, Postgres, MySQL and SQL Server.
package main

import (
	"database/sql"
	"fmt"

	_ "github.com/go-sql-driver/mysql"
	_ "github.com/jackc/pgx/v5/stdlib"
	_ "github.com/microsoft/go-mssqldb"
	_ "modernc.org/sqlite"
)

func main() { fmt.Println(sql.Drivers()) }
