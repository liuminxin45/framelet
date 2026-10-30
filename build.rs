fn main() {
    use std::{path::PathBuf, process::Command};
    let root = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let output = PathBuf::from(std::env::var_os("OUT_DIR").unwrap()).join("framelet.res");
    let sdk = std::env::var_os("WindowsSdkDir")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(std::env::var_os("ProgramFiles(x86)").expect("Windows SDK required"))
                .join("Windows Kits/10")
        });
    let mut candidates: Vec<_> = std::fs::read_dir(sdk.join("bin"))
        .expect("Windows SDK bin directory")
        .filter_map(Result::ok)
        .map(|e| e.path().join("x64/rc.exe"))
        .filter(|p| p.is_file())
        .collect();
    candidates.sort();
    let rc = candidates
        .last()
        .expect("Install Windows SDK resource compiler (rc.exe)");
    let status = Command::new(rc)
        .current_dir(root.join("assets"))
        .arg("/nologo")
        .arg("/fo")
        .arg(&output)
        .arg("framelet.rc")
        .status()
        .expect("run Windows resource compiler");
    assert!(status.success(), "Windows resource compilation failed");
    println!("cargo:rustc-link-arg={}", output.display());
    println!("cargo:rerun-if-changed=assets/framelet.rc");
    println!("cargo:rerun-if-changed=assets/framelet.manifest");
    println!("cargo:rerun-if-changed=assets/framelet.ico");
}
