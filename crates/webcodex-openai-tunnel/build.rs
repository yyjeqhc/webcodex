fn main() {
    // Crates.io/library consumers need neither Git nor a WebCodex checkout.
    // Packaging supplies provenance explicitly for a standalone binary build.
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=WEBCODEX_TUNNEL_SOURCE_COMMIT");
    println!(
        "cargo:rustc-env=WEBCODEX_TUNNEL_TARGET={}",
        std::env::var("TARGET").expect("Cargo TARGET")
    );
}
