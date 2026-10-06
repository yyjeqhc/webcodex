fn main() {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    if arguments == ["--version"] {
        println!("webcodex-browser-bridge {}", env!("CARGO_PKG_VERSION"));
        return;
    }
    if let Err(message) = webcodex_browser::run_native_host(&arguments) {
        // Native Messaging reserves stdout exclusively for framed JSON.
        eprintln!("{message}");
        std::process::exit(1);
    }
}
