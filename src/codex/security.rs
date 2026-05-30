const SENSITIVE_PATHS: &[&str] = &[
    ".git",
    ".env",
    ".pem",
    ".key",
    "id_rsa",
    "id_ed25519",
    "target",
    "node_modules",
    "/etc",
    "/root/.ssh",
];

pub fn is_sensitive_path(path: &str) -> bool {
    if path == ".gitignore" {
        return false;
    }
    let lower = path.to_lowercase();
    for sensitive in SENSITIVE_PATHS {
        if *sensitive == ".env" {
            // Match .env exactly or .env.* files
            let parts: Vec<&str> = path.split('/').collect();
            if parts.iter().any(|p| *p == ".env" || p.starts_with(".env.")) {
                return true;
            }
        } else if *sensitive == ".pem" || *sensitive == ".key" {
            if lower.ends_with(sensitive) {
                return true;
            }
        } else if lower.contains(sensitive) {
            return true;
        }
    }
    false
}
