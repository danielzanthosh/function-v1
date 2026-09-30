use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if target_os != "windows" {
        return;
    }

    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());

    // Locate icon.ico from workspace assets
    let icon_src = manifest_dir.join("../../assets/icon.ico");
    println!("cargo:rerun-if-changed={}", icon_src.display());

    if icon_src.exists() {
        let icon_dest = out_dir.join("icon.ico");
        if let Err(e) = fs::copy(&icon_src, &icon_dest) {
            eprintln!("Failed to copy icon to OUT_DIR: {}", e);
            return;
        }

        let rc_path = out_dir.join("app.rc");
        let obj_path = out_dir.join("icon.o");
        let rc_content = "1 ICON \"icon.ico\"\nMAINICON ICON \"icon.ico\"\n";

        if let Ok(_) = fs::write(&rc_path, rc_content) {
            let status = Command::new("windres")
                .current_dir(&out_dir)
                .arg("-i")
                .arg("app.rc")
                .arg("-o")
                .arg("icon.o")
                .status();

            if let Ok(s) = status {
                if s.success() && obj_path.exists() {
                    let obj_str = obj_path.to_string_lossy().replace('\\', "/");
                    println!("cargo:rustc-link-arg={}", obj_str);
                }
            }
        }
    }
}
