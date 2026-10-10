use std::process::Command;

// Captures the compiler version at build time as RUSTC_VERSION
// (e.g. "rustc 1.99.0 (abcdef012 2026-08-27)") for the /version endpoint.
fn main() {
    let rustc = std::env::var("RUSTC").unwrap_or_else(|_| "rustc".into());
    let version = Command::new(rustc)
        .arg("--version")
        .output()
        .ok()
        .filter(|out| out.status.success())
        .and_then(|out| String::from_utf8(out.stdout).ok())
        .map(|v| v.trim().to_owned())
        .unwrap_or_else(|| "unknown".into());

    println!("cargo:rustc-env=RUSTC_VERSION={version}");
    println!("cargo:rerun-if-changed=build.rs");
}
