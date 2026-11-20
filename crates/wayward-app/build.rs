use std::env;
use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-changed=wayward.exe.manifest");
    println!("cargo:rerun-if-changed=build.rs");

    if env::var("CARGO_CFG_TARGET_ENV").as_deref() != Ok("msvc") {
        return;
    }

    let manifest: PathBuf = env::current_dir()
        .expect("cargo sets cwd to the package root")
        .join("wayward.exe.manifest")
        .canonicalize()
        .expect("wayward.exe.manifest must exist");

    println!("cargo:rustc-link-arg-bins=/MANIFEST:EMBED");
    println!(
        "cargo:rustc-link-arg-bins=/MANIFESTINPUT:{}",
        manifest.display()
    );
}
