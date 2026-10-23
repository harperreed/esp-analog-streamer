// ABOUTME: Passes the ESP-IDF build environment to Cargo with MP3 C code in IRAM.
// ABOUTME: Derives the linker script from the pinned IDF output without changing data placement.
#[path = "build_support/mp3_iram.rs"]
mod mp3_iram;

fn main() {
    // Disabled SDK options are still valid names in the isolated diagnostic example.
    for name in [
        "esp_idf_heap_tracing_standalone",
        "esp_idf_esp_coredump_enable_to_uart",
        "esp_idf_esp_coredump_capture_dram",
    ] {
        println!("cargo:rustc-check-cfg=cfg({name})");
    }
    if let Some(cfg) = embuild::espidf::sysenv::cfg_args() {
        cfg.output();
    }
    let mut link = embuild::espidf::sysenv::link_args()
        .expect("MP3 IRAM placement requires ESP-IDF link arguments");
    let out_dir = std::env::var_os("OUT_DIR").expect("Cargo must provide OUT_DIR");
    let original = mp3_iram::place_mp3(&mut link.args, std::path::Path::new(&out_dir))
        .unwrap_or_else(|error| panic!("{error}"));
    println!("cargo:rerun-if-changed={}", original.display());
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=build_support/mp3_iram.rs");
    link.output();
}
