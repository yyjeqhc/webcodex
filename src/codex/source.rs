use super::edit::validate_binary_size;
use super::url_security::validate_source_url;

const URL_IMPORT_TIMEOUT_SECS: u64 = 10;
const MAX_BINARY_ARTIFACT_SIZE: usize = 5 * 1024 * 1024;

pub(super) fn read_binary_from_url(source_url: &str, rel_path: &str) -> Result<Vec<u8>, String> {
    let url = validate_source_url(source_url)?;
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(URL_IMPORT_TIMEOUT_SECS))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|e| format!("Failed to build URL client: {}", e))?;
    let response = client
        .get(url)
        .send()
        .map_err(|e| format!("Failed to fetch source_url: {}", e))?;
    if response.status().is_redirection() {
        return Err("source_url redirects are not allowed".to_string());
    }
    if !response.status().is_success() {
        return Err(format!("source_url returned HTTP {}", response.status()));
    }
    if let Some(len) = response.content_length() {
        if len as usize > MAX_BINARY_ARTIFACT_SIZE {
            return Err(format!(
                "source_url content for {} exceeds {} bytes",
                rel_path, MAX_BINARY_ARTIFACT_SIZE
            ));
        }
    }
    let mut bytes = Vec::new();
    {
        use std::io::Read;
        let mut limited = response.take((MAX_BINARY_ARTIFACT_SIZE + 1) as u64);
        limited
            .read_to_end(&mut bytes)
            .map_err(|e| format!("Failed to read source_url response: {}", e))?;
    }
    validate_binary_size(bytes, rel_path)
}
