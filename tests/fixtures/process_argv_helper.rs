//! Standalone test fixture compiled with `rustc` by structured-process tests.
//! It has no WebCodex dependencies and never invokes a shell.

use std::io::Read;
use std::time::Duration;

fn main() {
    let mut args = std::env::args().skip(1);
    match args.next().as_deref() {
        Some("argv") => {
            for value in args {
                println!("{}:{value}", value.len());
            }
        }
        Some("stdin") => {
            let mut input = String::new();
            std::io::stdin().read_to_string(&mut input).unwrap();
            print!("{input}");
        }
        Some("exit") => {
            let code = args.next().unwrap().parse::<i32>().unwrap();
            std::process::exit(code);
        }
        Some("sleep") => {
            let millis = args.next().unwrap().parse::<u64>().unwrap();
            std::thread::sleep(Duration::from_millis(millis));
        }
        Some("mark-sleep") => {
            use std::io::Write;
            let marker = args.next().unwrap();
            let nonce = args.next().unwrap();
            let millis = args.next().unwrap().parse::<u64>().unwrap();
            let mut file = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(marker)
                .unwrap();
            writeln!(file, "{}:{nonce}", std::process::id()).unwrap();
            std::thread::sleep(Duration::from_millis(millis));
            println!("{nonce}");
        }
        Some("gate") => {
            use std::io::Write;
            let started = args.next().unwrap();
            let active = args.next().unwrap();
            let release = args.next().unwrap();
            let nonce = args.next().unwrap();
            let mut started_file = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&started)
                .unwrap();
            writeln!(started_file, "{}:{nonce}", std::process::id()).unwrap();
            std::fs::write(&active, format!("{nonce}\n")).unwrap();
            let deadline = std::time::Instant::now() + Duration::from_secs(30);
            while !std::path::Path::new(&release).exists() {
                if std::time::Instant::now() >= deadline {
                    eprintln!("gate release timed out: {nonce}");
                    std::process::exit(70);
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            std::fs::remove_file(&active).unwrap();
            println!("{nonce}");
        }
        Some(mode) => {
            eprintln!("unknown mode: {mode}");
            std::process::exit(64);
        }
        None => {}
    }
}
