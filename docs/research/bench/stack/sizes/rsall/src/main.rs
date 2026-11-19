// Size probe: connect paths are reachable (behind an env var) so the linker keeps the real driver code.
fn main() {
    let url = std::env::var("PROBE_URL").ok();
    let c = rusqlite::Connection::open_in_memory().unwrap();
    let n: i64 = c.query_row("select count(*) from sqlite_schema", [], |r| r.get(0)).unwrap();
    if let Some(u) = url {
        let _ = postgres::Client::connect(&u, postgres::NoTls).map(|mut c| c.simple_query("select 1"));
        let _ = mysql::Pool::new(u.as_str()).map(|p| p.get_conn().map(|mut c| { use mysql::prelude::Queryable; c.query_drop("select 1") }));
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        rt.block_on(async {
            let cfg = tiberius::Config::from_ado_string(&u).unwrap();
            if let Ok(tcp) = tokio::net::TcpStream::connect(cfg.get_addr()).await {
                use tokio_util::compat::TokioAsyncWriteCompatExt;
                if let Ok(mut client) = tiberius::Client::connect(cfg, tcp.compat_write()).await {
                    let _ = client.simple_query("select 1").await;
                }
            }
        });
    }
    println!("sqlite({n}) postgres mysql mssql");
}
