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

    pub fn is_agent(&self) -> bool {
        true
    }

    pub fn agent_client_id(&self) -> Result<&str, String> {
        let client_id = self.client_id.trim();
        if client_id.is_empty() {
            return Err("agent project requires client_id".to_string());
        }
        Ok(client_id)
    }
}
