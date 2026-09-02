use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct ProjectConfig {
    pub path: String,
    pub client_id: String,
    pub allow_patch: bool,
}

impl ProjectConfig {
    pub fn root(&self) -> PathBuf {
        PathBuf::from(&self.path)
    }

    pub fn allow_patch(&self) -> bool {
        self.allow_patch
    }
}
