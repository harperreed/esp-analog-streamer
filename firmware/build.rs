// ABOUTME: Build script: passes the ESP-IDF build environment from esp-idf-sys on to cargo.
// ABOUTME: Unchanged from the esp-idf-template.

fn main() {
    embuild::espidf::sysenv::output();
}
