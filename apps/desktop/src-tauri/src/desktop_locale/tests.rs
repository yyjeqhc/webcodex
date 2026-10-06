use super::*;

const LOCALES: [DesktopLocale; 7] = [
    DesktopLocale::ZhCn,
    DesktopLocale::ZhTw,
    DesktopLocale::EnUs,
    DesktopLocale::JaJp,
    DesktopLocale::KoKr,
    DesktopLocale::DeDe,
    DesktopLocale::FrFr,
];

#[test]
fn every_locale_has_all_native_tray_translations() {
    let english = &CATALOGS[DesktopLocale::EnUs as usize];
    let keys: Vec<_> = english
        .keys()
        .filter(|key| key.starts_with("tray."))
        .collect();
    assert_eq!(keys.len(), 19);
    assert!(keys.iter().any(|key| key.as_str() == "tray.lightweight"));
    for locale in LOCALES {
        for key in &keys {
            assert!(!locale.text(key).trim().is_empty(), "{locale:?}: {key}");
            if locale != DesktopLocale::EnUs {
                assert_ne!(locale.text(key), english[*key], "{locale:?}: {key}");
            }
        }
    }
}

#[test]
fn restores_each_language_before_a_webview_is_available() {
    let dir = tempfile::tempdir().unwrap();
    for locale in LOCALES {
        let state = DesktopLocaleState::load(dir.path());
        state.set(locale).unwrap();
        assert_eq!(state.get(), locale);
        assert_eq!(DesktopLocaleState::load(dir.path()).get(), locale);
    }
}

#[test]
fn missing_corrupt_and_unsupported_preferences_default_to_simplified_chinese() {
    let dir = tempfile::tempdir().unwrap();
    assert_eq!(
        DesktopLocaleState::load(dir.path()).get(),
        DesktopLocale::ZhCn
    );
    for bytes in [b"invalid".as_slice(), b"\"unknown\"".as_slice()] {
        std::fs::write(dir.path().join("desktop-locale.json"), bytes).unwrap();
        assert_eq!(
            DesktopLocaleState::load(dir.path()).get(),
            DesktopLocale::ZhCn
        );
    }
    assert!(serde_json::from_str::<DesktopLocale>("\"unknown\"").is_err());
}

#[test]
fn persistence_failure_still_updates_the_current_language() {
    let dir = tempfile::tempdir().unwrap();
    let state = DesktopLocaleState::load(dir.path());
    std::fs::create_dir(dir.path().join("desktop-locale.json")).unwrap();
    assert!(state.set(DesktopLocale::JaJp).is_err());
    assert_eq!(state.get(), DesktopLocale::JaJp);
}
