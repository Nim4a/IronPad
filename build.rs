fn main() {
    println!("cargo:rerun-if-changed=assets/icon.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let out = std::env::var("OUT_DIR").unwrap();
    let ico = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("assets/icon.ico")
        .to_string_lossy()
        .replace('\\', "/");
    let rc = format!("{out}/icon.rc");
    let obj = format!("{out}/icon.o");
    std::fs::write(&rc, format!("1 ICON \"{ico}\"\n")).unwrap();
    // windres comes from the GNU toolchain (w64devkit); skip silently if absent.
    if let Ok(st) = std::process::Command::new("windres")
        .args([&rc, "-O", "coff", "-o", &obj])
        .status()
    {
        if st.success() {
            println!("cargo:rustc-link-arg={obj}");
        }
    }
}
