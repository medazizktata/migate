use std::time::Instant;
fn main() {
    let sql = std::fs::read_to_string(std::env::args().nth(1).unwrap()).unwrap();
    let t0 = Instant::now();
    let n = pg_query::parse(&sql, 0).unwrap().protobuf.stmts.len();
    let t1 = Instant::now();
    // sqlparser-rs cannot parse plpgsql function bodies the same way; count what it parses.
    let m = sqlparser::parser::Parser::parse_sql(&sqlparser::dialect::PostgreSqlDialect {}, &sql).map(|v| v.len());
    println!("pg_query stmts={n} parse_ms={:.1} | sqlparser-rs {:?} parse_ms={:.1}", (t1 - t0).as_secs_f64() * 1e3, m, t1.elapsed().as_secs_f64() * 1e3);
}
