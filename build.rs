fn main() {
    println!("cargo:rerun-if-changed=native/game_center.m");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos")
        && std::env::var_os("CARGO_FEATURE_GAME_CENTER").is_some()
    {
        cc::Build::new()
            .file("native/game_center.m")
            .flag("-fobjc-arc")
            .flag("-fblocks")
            .compile("todora_game_center");
        for framework in ["GameKit", "AppKit", "Foundation"] {
            println!("cargo:rustc-link-lib=framework={framework}");
        }
    }
}
