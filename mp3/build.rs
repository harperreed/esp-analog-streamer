// ABOUTME: Compiles the pinned standalone Shine library for host and device.
// ABOUTME: Uses the target C toolchain without requiring ESP-IDF headers.
fn main() {
    let mut build = cc::Build::new();
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("espidf") {
        // Pinned esp-idf-sys exports its activated tool path as build metadata.
        // This build-script process is single-threaded; no machine PATH is changed.
        let path = std::env::var("DEP_ESP_IDF_EMBUILD_ENV_PATH")
            .expect("esp-idf-sys must provide its installed compiler path");
        std::env::set_var("PATH", path);
        assert_eq!(
            std::env::var("TARGET").as_deref(),
            Ok("xtensa-esp32-espidf")
        );
        // The generic Xtensa executable has different architecture defaults.
        // Use IDF's classic-ESP32 compiler driver and long-call convention.
        build
            .compiler("xtensa-esp32-elf-gcc")
            .archiver("xtensa-esp32-elf-ar")
            .flag("-mlongcalls");
    }
    build
        .include("vendor/shine")
        .opt_level(3)
        .flag("-fsigned-char");
    for name in [
        "bitstream",
        "huffman",
        "l3bitstream",
        "l3loop",
        "l3mdct",
        "l3subband",
        "layer3",
        "reservoir",
        "tables",
    ] {
        build.file(format!("vendor/shine/{name}.c"));
    }
    build.file("src/bridge.c").compile("shazam_mp3");
    println!("cargo:rustc-link-lib=m");
    println!("cargo:rerun-if-changed=vendor/shine");
    println!("cargo:rerun-if-changed=src/bridge.c");
}
