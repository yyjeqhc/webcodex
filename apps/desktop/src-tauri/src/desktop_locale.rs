use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DesktopLocale {
    #[default]
    #[serde(rename = "zh-CN")]
    ZhCn,
    #[serde(rename = "zh-TW")]
    ZhTw,
    #[serde(rename = "en-US")]
    EnUs,
    #[serde(rename = "ja-JP")]
    JaJp,
    #[serde(rename = "ko-KR")]
    KoKr,
    #[serde(rename = "de-DE")]
    DeDe,
    #[serde(rename = "fr-FR")]
    FrFr,
}

// Embed the UI catalogs so native menus use the same translations without a WebView.
static CATALOGS: LazyLock<[HashMap<String, String>; 7]> = LazyLock::new(|| {
    [
        include_str!("../../src/i18n/messages/zh-CN.json"),
        include_str!("../../src/i18n/messages/zh-TW.json"),
        include_str!("../../src/i18n/messages/en-US.json"),
        include_str!("../../src/i18n/messages/ja-JP.json"),
        include_str!("../../src/i18n/messages/ko-KR.json"),
        include_str!("../../src/i18n/messages/de-DE.json"),
        include_str!("../../src/i18n/messages/fr-FR.json"),
    ]
    .map(|json| serde_json::from_str(json).expect("valid embedded Desktop catalog"))
});

impl DesktopLocale {
    pub fn text(self, key: &str) -> &'static str {
        CATALOGS[self as usize]
            .get(key)
            .expect("complete embedded Desktop catalog")
    }
}

pub struct DesktopLocaleState {
    path: PathBuf,
    locale: Mutex<DesktopLocale>,
}

impl DesktopLocaleState {
    pub fn load(data_dir: &Path) -> Self {
        let path = data_dir.join("desktop-locale.json");
        let locale = std::fs::read(&path)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        Self {
            path,
            locale: Mutex::new(locale),
        }
    }

    pub fn get(&self) -> DesktopLocale {
        *self
            .locale
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    pub fn set(&self, locale: DesktopLocale) -> std::io::Result<()> {
        let mut current = self
            .locale
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        // Apply even if disk persistence fails; the UI still changed its language.
        *current = locale;
        crate::state::write_atomic_file(&self.path, &serde_json::to_vec(&locale)?)
    }
}

#[cfg(test)]
mod tests;
