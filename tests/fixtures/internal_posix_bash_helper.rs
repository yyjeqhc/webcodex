//! Windows-only test fixture used as a deterministic `bash.exe` shim.
//! It proves Runner-generated POSIX programs are sent to an explicit Bash
//! runtime through stdin instead of the configured PowerShell parser.

use std::io::Read;

fn main() {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args != ["-s"] {
        eprintln!("unexpected argv: {args:?}");
        std::process::exit(64);
    }
    let mut script = String::new();
    std::io::stdin().read_to_string(&mut script).unwrap();
    print!("{script}");
}
