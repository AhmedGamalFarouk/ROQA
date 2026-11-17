fn main() {
    let root = std::path::PathBuf::from(
        std::env::var_os("CARGO_MANIFEST_DIR").expect("Cargo sets the package directory"),
    );
    let manifest = root.join("app.manifest");
    println!("cargo:rerun-if-changed=app.manifest");
    println!("cargo:rustc-link-arg-bin=roqa=/MANIFEST:EMBED");
    println!(
        "cargo:rustc-link-arg-bin=roqa=/MANIFESTINPUT:{}",
        manifest.display()
    );
    println!("cargo:rerun-if-changed=assets/icon.rc");
    println!("cargo:rerun-if-changed=assets/icon.ico");
    let resource =
        std::path::PathBuf::from(std::env::var_os("OUT_DIR").expect("Cargo sets OUT_DIR"))
            .join("icon.res");
    let status = std::process::Command::new(resource_compiler())
        .current_dir(root.join("assets"))
        .arg("/nologo")
        .arg("/fo")
        .arg(&resource)
        .arg("icon.rc")
        .status()
        .expect("Run the Windows SDK resource compiler");
    assert!(status.success(), "Windows icon resource compilation failed");
    println!("cargo:rustc-link-arg-bin=roqa={}", resource.display());
}

fn resource_compiler() -> std::path::PathBuf {
    let path = std::env::var_os("PATH").unwrap_or_default();
    if let Some(compiler) = std::env::split_paths(&path)
        .map(|dir| dir.join("rc.exe"))
        .find(|file| file.is_file())
    {
        return compiler;
    }
    let kits = std::path::PathBuf::from(
        std::env::var_os("ProgramFiles(x86)").expect("Windows sets ProgramFiles(x86)"),
    )
    .join("Windows Kits/10/bin");
    let mut compilers: Vec<_> = std::fs::read_dir(kits)
        .expect("Windows SDK is required to build ROQA")
        .map(|entry| {
            entry
                .expect("Read Windows SDK directory")
                .path()
                .join("x64/rc.exe")
        })
        .filter(|file| file.is_file())
        .collect();
    compilers.sort();
    compilers.pop().expect("Find the Windows SDK rc.exe")
}
