fn main() {
    let manifest = std::path::PathBuf::from(
        std::env::var_os("CARGO_MANIFEST_DIR").expect("Cargo sets the package directory"),
    )
    .join("app.manifest");
    println!("cargo:rerun-if-changed=app.manifest");
    println!("cargo:rustc-link-arg-bin=roqa=/MANIFEST:EMBED");
    println!(
        "cargo:rustc-link-arg-bin=roqa=/MANIFESTINPUT:{}",
        manifest.display()
    );
}
