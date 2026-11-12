//! Gives FontCraft.exe its icon on Windows: assets/icon.png is turned into a multi-size
//! .ico and built into the program. (macOS and Linux get the icon when they are packaged.)

use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-changed=assets/icon.png");
    println!("cargo:rerun-if-changed=build.rs");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let out = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR"));
    let ico = out.join("fontcraft.ico");
    if let Err(e) = write_ico(&ico) {
        println!("cargo:warning=The program icon could not be made: {e}");
        return;
    }
    let rc = out.join("fontcraft.rc");
    let path = ico.to_string_lossy().replace('\\', "\\\\");
    if let Err(e) = std::fs::write(&rc, format!("1 ICON \"{path}\"\n")) {
        println!("cargo:warning=The program icon could not be made: {e}");
        return;
    }
    // A resource compiler named in RC_<target> or RC (for example the windres of a portable
    // llvm-mingw kit) is found even when its folder isn't on PATH.
    let target = std::env::var("TARGET").unwrap_or_default();
    let named = std::env::var_os(format!("RC_{}", target.replace('-', "_")))
        .or_else(|| std::env::var_os(format!("RC_{target}")))
        .or_else(|| std::env::var_os("RC"));
    if let Some(dir) = named.map(PathBuf::from).filter(|p| p.is_file()).and_then(|p| p.parent().map(|d| d.to_path_buf())) {
        let mut paths = vec![dir];
        paths.extend(std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()));
        if let Ok(joined) = std::env::join_paths(paths) {
            std::env::set_var("PATH", joined);
        }
    }
    match embed_resource::compile(&rc, embed_resource::NONE) {
        embed_resource::CompilationResult::Ok => {}
        other => println!("cargo:warning=The program icon was left out ({other})"),
    }
}

fn write_ico(dest: &std::path::Path) -> Result<(), String> {
    use image::codecs::ico::{IcoEncoder, IcoFrame};
    let img = image::open("assets/icon.png").map_err(|e| e.to_string())?;
    let mut frames = vec![];
    for s in [16u32, 20, 24, 32, 40, 48, 64, 128, 256] {
        let r = img.resize_exact(s, s, image::imageops::FilterType::Lanczos3).to_rgba8();
        frames.push(IcoFrame::as_png(r.as_raw(), s, s, image::ExtendedColorType::Rgba8).map_err(|e| e.to_string())?);
    }
    let f = std::fs::File::create(dest).map_err(|e| e.to_string())?;
    IcoEncoder::new(f).encode_images(&frames).map_err(|e| e.to_string())
}
