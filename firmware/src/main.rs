// ABOUTME: Starts the production firmware through its shared boot routine.
// ABOUTME: Hardware examples use that same routine without copying application setup.

fn main() -> anyhow::Result<()> {
    esp_shazam_firmware::run()
}
