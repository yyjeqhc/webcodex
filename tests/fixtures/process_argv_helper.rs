//! Standalone test fixture compiled with `rustc` by structured-process tests.
//! It has no WebCodex dependencies and never invokes a shell.

use std::io::Read;
use std::time::Duration;

#[cfg(windows)]
fn write_raw_stdout_stderr(bytes: &[u8]) {
    use std::io::Write;

    std::io::stdout().write_all(bytes).unwrap();
    std::io::stdout().flush().unwrap();
    std::io::stderr().write_all(bytes).unwrap();
    std::io::stderr().flush().unwrap();
}

#[cfg(windows)]
fn write_utf16le_stdout_stderr(text: &str) {
    let mut bytes = vec![0xFF, 0xFE];
    for unit in text.encode_utf16() {
        bytes.extend_from_slice(&unit.to_le_bytes());
    }
    write_raw_stdout_stderr(&bytes);
}

#[cfg(windows)]
fn encode_active_oem(text: &str) -> Option<Vec<u8>> {
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetOEMCP() -> u32;
        fn WideCharToMultiByte(
            code_page: u32,
            flags: u32,
            wide: *const u16,
            wide_len: i32,
            output: *mut u8,
            output_len: i32,
            default_char: *const u8,
            used_default_char: *mut i32,
        ) -> i32;
    }

    const WC_NO_BEST_FIT_CHARS: u32 = 0x0000_0400;
    let wide = text.encode_utf16().collect::<Vec<_>>();
    let mut used_default = 0_i32;
    // SAFETY: GetOEMCP has no arguments.
    let code_page = unsafe { GetOEMCP() };
    let mut used_default_ptr = &mut used_default as *mut i32;
    // SAFETY: all buffers are valid for the explicit lengths. The first call
    // only asks Windows for the required output size.
    let mut flags = WC_NO_BEST_FIT_CHARS;
    let mut needed = unsafe {
        WideCharToMultiByte(
            code_page,
            flags,
            wide.as_ptr(),
            wide.len() as i32,
            std::ptr::null_mut(),
            0,
            std::ptr::null(),
            used_default_ptr,
        )
    };
    if needed <= 0 {
        // UTF-8 code pages reject WC_NO_BEST_FIT_CHARS because every Unicode
        // scalar is representable. Flags zero is deterministic there.
        flags = 0;
        used_default = 0;
        if code_page == 65_001 {
            // CP_UTF8 requires both default-character arguments to be null.
            used_default_ptr = std::ptr::null_mut();
        }
        // SAFETY: same sizing call and valid input buffer as above.
        needed = unsafe {
            WideCharToMultiByte(
                code_page,
                flags,
                wide.as_ptr(),
                wide.len() as i32,
                std::ptr::null_mut(),
                0,
                std::ptr::null(),
                used_default_ptr,
            )
        };
    }
    if needed <= 0 || used_default != 0 {
        return None;
    }
    let mut output = vec![0_u8; needed as usize];
    used_default = 0;
    // SAFETY: `output` has the exact capacity returned by the sizing call.
    let written = unsafe {
        WideCharToMultiByte(
            code_page,
            flags,
            wide.as_ptr(),
            wide.len() as i32,
            output.as_mut_ptr(),
            needed,
            std::ptr::null(),
            used_default_ptr,
        )
    };
    (written == needed && (used_default_ptr.is_null() || used_default == 0)).then_some(output)
}

#[cfg(windows)]
fn active_oem_sample() -> (String, Vec<u8>) {
    // Cover common Western, Greek, Cyrillic, Hebrew, Arabic, CJK, and Hangul
    // OEM pages without assuming a fixed machine locale.
    for sample in [
        "é", "Ç", "ü", "ä", "ö", "ñ", "ß", "Ω", "Ж", "א", "ش", "中", "あ", "한",
    ] {
        if let Some(bytes) = encode_active_oem(sample) {
            if bytes.iter().any(|byte| !byte.is_ascii()) {
                return (sample.to_string(), bytes);
            }
        }
    }
    panic!("active OEM code page has no representable non-ASCII test sample");
}

fn write_chatty(chunks: usize) {
    use std::io::Write;

    let stdout_payload = vec![b'x'; 8 * 1024];
    let stderr_payload = vec![b'y'; 8 * 1024];
    let stdout = std::io::stdout();
    let stderr = std::io::stderr();
    let mut stdout = stdout.lock();
    let mut stderr = stderr.lock();
    for _ in 0..chunks {
        stdout.write_all(&stdout_payload).unwrap();
        stdout.write_all(b"\n").unwrap();
        stderr.write_all(&stderr_payload).unwrap();
        stderr.write_all(b"\n").unwrap();
    }
    stdout.flush().unwrap();
    stderr.flush().unwrap();
}

fn append_start_marker(marker: &str, nonce: &str) {
    use std::io::Write;

    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(marker)
        .unwrap();
    writeln!(file, "{}:{nonce}", std::process::id()).unwrap();
}

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
        Some("chatty") => {
            let chunks = args.next().unwrap().parse::<usize>().unwrap();
            write_chatty(chunks);
        }
        Some("mark-chatty") => {
            let marker = args.next().unwrap();
            let nonce = args.next().unwrap();
            let chunks = args.next().unwrap().parse::<usize>().unwrap();
            append_start_marker(&marker, &nonce);
            write_chatty(chunks);
        }
        Some("mark-chatty-sleep") => {
            let marker = args.next().unwrap();
            let ready = args.next().unwrap();
            let nonce = args.next().unwrap();
            let chunks = args.next().unwrap().parse::<usize>().unwrap();
            let millis = args.next().unwrap().parse::<u64>().unwrap();
            append_start_marker(&marker, &nonce);
            write_chatty(chunks);
            std::fs::write(ready, "drained\n").unwrap();
            std::thread::sleep(Duration::from_millis(millis));
        }
        Some("spawn-pipe-descendant") => {
            let marker = args.next().unwrap();
            let millis = args.next().unwrap().parse::<u64>().unwrap();
            let child = std::process::Command::new(std::env::current_exe().unwrap())
                .arg("pipe-descendant")
                .arg(marker)
                .arg(millis.to_string())
                .spawn()
                .unwrap();
            println!("DESCENDANT_PID={}", child.id());
        }
        Some("pipe-descendant") => {
            let marker = args.next().unwrap();
            let millis = args.next().unwrap().parse::<u64>().unwrap();
            std::fs::write(marker, std::process::id().to_string()).unwrap();
            std::thread::sleep(Duration::from_millis(millis));
        }
        Some("mark") => {
            let marker = args.next().unwrap();
            std::fs::write(marker, "started\n").unwrap();
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
        #[cfg(windows)]
        Some("windows-utf8-output") => {
            write_raw_stdout_stderr("UTF8 中文 🙂\r\n".as_bytes());
            std::process::exit(17);
        }
        #[cfg(windows)]
        Some("windows-utf16-output") => {
            write_utf16le_stdout_stderr("UTF16 中文 🙂\r\n");
        }
        #[cfg(windows)]
        Some("windows-oem-output") => {
            let expected_path = args.next().unwrap();
            let repeat = args
                .next()
                .map(|value| value.parse::<usize>().unwrap())
                .unwrap_or(1);
            let (sample, sample_bytes) = active_oem_sample();
            let expected = sample.repeat(repeat);
            let bytes = sample_bytes.repeat(repeat);
            std::fs::write(expected_path, &expected).unwrap();
            write_raw_stdout_stderr(&bytes);
            std::process::exit(23);
        }
        #[cfg(windows)]
        Some("windows-utf8-split-output") => {
            use std::io::Write;

            let marker = args.next().unwrap();
            std::fs::write(marker, std::process::id().to_string()).unwrap();
            let bytes = "split 中 🙂\r\n".as_bytes();
            let split = bytes.iter().position(|byte| *byte >= 0x80).unwrap() + 1;
            std::io::stdout().write_all(&bytes[..split]).unwrap();
            std::io::stdout().flush().unwrap();
            std::thread::sleep(Duration::from_millis(300));
            std::io::stdout().write_all(&bytes[split..]).unwrap();
            std::io::stdout().flush().unwrap();
        }
        #[cfg(windows)]
        Some("windows-oem-split-output") => {
            use std::io::Write;

            let expected_path = args.next().unwrap();
            let marker = args.next().unwrap();
            let (expected, bytes) = active_oem_sample();
            std::fs::write(expected_path, expected).unwrap();
            std::fs::write(marker, std::process::id().to_string()).unwrap();
            let split = 1.min(bytes.len());
            std::io::stdout().write_all(&bytes[..split]).unwrap();
            std::io::stdout().flush().unwrap();
            std::io::stderr().write_all(&bytes[..split]).unwrap();
            std::io::stderr().flush().unwrap();
            std::thread::sleep(Duration::from_millis(300));
            std::io::stdout().write_all(&bytes[split..]).unwrap();
            std::io::stdout().flush().unwrap();
            std::io::stderr().write_all(&bytes[split..]).unwrap();
            std::io::stderr().flush().unwrap();
        }
        #[cfg(windows)]
        Some("windows-mark-output-sleep") => {
            use std::io::Write;

            let marker = args.next().unwrap();
            let millis = args.next().unwrap().parse::<u64>().unwrap();
            let mut file = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(marker)
                .unwrap();
            writeln!(file, "{}", std::process::id()).unwrap();
            write_raw_stdout_stderr("partial 中文 🙂\r\n".as_bytes());
            std::thread::sleep(Duration::from_millis(millis));
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
