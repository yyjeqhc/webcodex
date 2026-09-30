fn main() {
    // Direct Cargo production builds also embed dist. The Tauri CLI hook alone
    // does not run for that entry point; never compile new native code against
    // a previously built UI tree.
    if !tauri_build::is_dev() {
        let desktop = std::path::PathBuf::from(
            std::env::var_os("CARGO_MANIFEST_DIR").expect("Cargo manifest directory"),
        )
        .parent()
        .expect("Desktop directory")
        .to_owned();
        let status = std::process::Command::new(if cfg!(windows) { "npm.cmd" } else { "npm" })
            .args(["run", "build"])
            .current_dir(desktop)
            .stdin(std::process::Stdio::null())
            .status()
            .expect("Desktop frontend build could not start");
        assert!(
            status.success(),
            "Desktop frontend build failed; refusing to embed stale dist"
        );
    }
    for input in [
        "../src",
        "../index.html",
        "../package.json",
        "../package-lock.json",
        "../vite.config.ts",
        "../tsconfig.json",
        "../../../frontend/src",
    ] {
        println!("cargo:rerun-if-changed={input}");
    }
    tauri_build::build()
}
