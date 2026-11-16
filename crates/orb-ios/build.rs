//! Compiles the Objective-C shims of the iOS build. A desktop build has none.

fn main() {
    println!("cargo:rerun-if-changed=src/audio_session.m");
    println!("cargo:rerun-if-changed=src/menus.m");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("ios") {
        return;
    }
    cc::Build::new()
        .file("src/audio_session.m")
        .file("src/menus.m")
        .flag("-fobjc-arc")
        .compile("orb_ios_shim");
    println!("cargo:rustc-link-lib=framework=AVFoundation");
}
