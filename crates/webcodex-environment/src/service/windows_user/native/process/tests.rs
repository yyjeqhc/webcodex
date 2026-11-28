use super::*;

#[test]
fn native_path_comparison_preserves_the_narrow_extended_dos_equivalence() {
    let ordinary = r"C:\Program Files\WebCodex\测试\webcodex-server.exe";
    let extended = format!(r"\\?\{ordinary}");
    for observed in [ordinary, &extended] {
        for expected in [ordinary, &extended] {
            assert!(executable_path_matches(observed, expected));
        }
    }
    assert!(executable_path_matches(
        r"C:\Äpps\WebCodex.exe",
        r"c:\äPPS\WEBCODEX.EXE"
    ));
    for ordinary in [
        r"C:\CON Tools\WebCodex.exe",
        r"D:\COM10\WebCodex.exe",
        r"E:\.local\WebCodex.exe",
    ] {
        assert!(executable_path_matches(
            ordinary,
            &format!(r"\\?\{ordinary}")
        ));
    }
    for ordinary in [
        r"C:Apps\webcodex.exe",
        r"\Apps\webcodex.exe",
        r"Apps\webcodex.exe",
        "C:/Apps/webcodex.exe",
        r"C:\Apps\\webcodex.exe",
        r"C:\Apps\.\webcodex.exe",
        r"C:\Apps\..\webcodex.exe",
        r"C:\Apps.\webcodex.exe",
        r"C:\Apps \webcodex.exe",
        r"C:\Apps\webcodex.exe.",
        r"C:\Apps\webcodex.exe ",
        r"C:\Apps\webcodex.exe:other",
        "C:\\Apps\\",
        "C:\\",
        r"C:\CON\webcodex.exe",
        r"C:\nul.txt\webcodex.exe",
        r"C:\COM1\webcodex.exe",
        r"C:\LPT9\webcodex.exe",
        r"C:\CONIN$\webcodex.exe",
        r"C:\CONOUT$.txt\webcodex.exe",
        r"C:\AUX .txt\webcodex.exe",
        r"C:\Apps\PRN.txt",
        r"C:\Apps\nul.tar.gz",
        r"C:\COM¹\webcodex.exe",
        r"C:\LPT².txt\webcodex.exe",
    ] {
        let extended = format!(r"\\?\{ordinary}");
        assert!(!executable_path_matches(ordinary, &extended));
        assert!(!executable_path_matches(&extended, ordinary));
    }
    for character in ['<', '>', '"', '|', '?', '*', '\0', '\u{1}', '\u{1f}'] {
        let ordinary = format!(r"C:\App{character}s\webcodex.exe");
        assert!(!executable_path_matches(
            &ordinary,
            &format!(r"\\?\{ordinary}")
        ));
    }
    assert!(!executable_path_matches(
        r"\\server\share\webcodex.exe",
        r"\\?\UNC\server\share\webcodex.exe"
    ));
    assert!(!executable_path_matches(
        r"\\.\C:\Apps\webcodex.exe",
        r"C:\Apps\webcodex.exe"
    ));
    assert!(!executable_path_matches("", ""));
}

#[test]
fn native_argv_and_owner_matching_rejects_lookalike_actions() {
    let action = Action {
        program: r"C:\WebCodex\测试\server.exe".into(),
        arguments: r#""--config" "C:\环境\Config.toml" "a\"b" "trailing\\""#.into(),
    };
    let command = format!(r#""{}" {}"#, action.program, action.arguments);
    let observed = Observation {
        pid: 4312,
        created: 1234,
        owner: "S-1-5-21-1-2-3-1001".into(),
        image: action.program.clone(),
        argv: parse_command(&command).unwrap(),
    };
    assert!(observed.matches(&action, &observed.owner));
    for image in [action.program.clone(), format!(r"\\?\{}", action.program)] {
        for argv0 in [action.program.clone(), format!(r"\\?\{}", action.program)] {
            let mut value = observed.clone();
            value.image = image.clone();
            value.argv[0] = argv0;
            assert!(value.matches(&action, &value.owner));
        }
    }
    let mut changed = observed.clone();
    changed.argv[1] = "--CONFIG".into();
    assert!(!changed.matches(&action, &changed.owner));
    assert!(!observed.matches(&action, "S-1-5-21-1-2-3-1002"));
    changed = observed.clone();
    changed.created = 0;
    assert!(!changed.matches(&action, &changed.owner));
    changed = observed.clone();
    changed.image = r"C:\Other\server.exe".into();
    assert!(!changed.matches(&action, &changed.owner));
    assert!(parse_command("").is_none());
    assert!(parse_command("server.exe\0 --config").is_none());
    assert!(parse_command(&"x".repeat(MAX_TEXT + 1)).is_none());
    assert!(parse_command(&format!("server.exe {}", "x ".repeat(4096))).is_none());
}

#[test]
fn native_sid_resolution_preserves_only_the_bare_computer_collision_exception() {
    let owner = "S-1-5-21-1-2-3-1001";
    let domain = "S-1-5-21-1-2-3";
    let resolve = |name: &str| match name {
        "DESKTOP" => Some(Sid {
            text: domain.into(),
            domain: None,
        }),
        "DESKTOP\\DESKTOP" => Some(Sid {
            text: owner.into(),
            domain: Some(domain.into()),
        }),
        _ => None,
    };
    assert!(account_matches_with("DESKTOP", owner, "DESKTOP", resolve));
    assert!(account_matches_with("DESKTOP", owner, "DESKTOP", |name| {
        if name == "DESKTOP" {
            None
        } else {
            resolve(name)
        }
    }));
    assert!(!account_matches_with("DESKTOP", owner, "DESKTOP", |name| {
        if name == "DESKTOP" {
            Some(Sid {
                text: "S-1-5-21-1-2-3-9999".into(),
                domain: Some(domain.into()),
            })
        } else {
            resolve(name)
        }
    }));
    for name in [
        "OTHER",
        "OTHER\\DESKTOP",
        ".\\DESKTOP",
        "DESKTOP@example.test",
        "S-1-5-21-9-9-9-1001",
    ] {
        let mut attempts = 0;
        assert!(!account_matches_with(name, owner, "DESKTOP", |_| {
            attempts += 1;
            None
        }));
        assert_eq!(
            attempts, 1,
            "qualified or unrelated identities must not be reinterpreted"
        );
    }
    assert!(account_matches(owner, owner, "DESKTOP"));
}

#[test]
fn native_current_process_query_reads_stable_creation_sid_image_and_argv() {
    let process = Process::open(std::process::id()).expect("query-only self handle");
    let first = process
        .read()
        .expect("Windows supports bounded native command-line observation");
    let second = process
        .read()
        .expect("same live process remains observable");
    assert_eq!((first.pid, first.created), (second.pid, second.created));
    assert_eq!(first.owner, second.owner);
    assert_eq!(first.argv, std::env::args().collect::<Vec<_>>());
    assert!(executable_path_matches(
        &first.image,
        std::env::current_exe().unwrap().to_str().unwrap()
    ));
    assert!(process.is_alive());
    assert!(Process::open(0).is_none());
}
