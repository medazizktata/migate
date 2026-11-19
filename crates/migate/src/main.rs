fn main() {
    match std::env::args().nth(1).as_deref() {
        Some("--version" | "-V") => println!("migate {}", env!("CARGO_PKG_VERSION")),
        _ => {
            eprintln!("migate: nothing is implemented yet (try --version)");
            std::process::exit(2);
        }
    }
}
