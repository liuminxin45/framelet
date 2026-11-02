fn main() {
    if let Err(e) = build() {
        eprintln!("Framelet resource build failed: {e}");
        std::process::exit(1);
    }
}
fn build() -> Result<(), Box<dyn std::error::Error>> {
    use std::{path::PathBuf, process::Command};
    let root =
        PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").ok_or("CARGO_MANIFEST_DIR missing")?);
    let output =
        PathBuf::from(std::env::var_os("OUT_DIR").ok_or("OUT_DIR missing")?).join("framelet.res");
    let sdk = if let Some(p) = std::env::var_os("WindowsSdkDir") {
        PathBuf::from(p)
    } else {
        PathBuf::from(std::env::var_os("ProgramFiles(x86)").ok_or("Windows SDK required")?)
            .join("Windows Kits/10")
    };
    let mut candidates: Vec<_> = std::fs::read_dir(sdk.join("bin"))?
        .filter_map(Result::ok)
        .map(|e| e.path().join("x64/rc.exe"))
        .filter(|p| p.is_file())
        .collect();
    candidates.sort();
    let rc = candidates
        .last()
        .ok_or("Install Windows SDK resource compiler (rc.exe)")?;
    let status = Command::new(rc)
        .current_dir(root.join("assets"))
        .arg("/nologo")
        .arg("/fo")
        .arg(&output)
        .arg("framelet.rc")
        .status()?;
    if !status.success() {
        return Err("Windows resource compilation failed".into());
    }
    println!("cargo:rustc-link-arg={}", output.display());
    println!("cargo:rerun-if-changed=assets/framelet.rc");
    println!("cargo:rerun-if-changed=assets/framelet.manifest");
    println!("cargo:rerun-if-changed=assets/framelet.ico");
    Ok(())
}
