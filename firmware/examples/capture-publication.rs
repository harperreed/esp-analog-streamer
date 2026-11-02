// ABOUTME: Runs the feature-gated delayed reader beside production capture and real streams.
// ABOUTME: Requires capture-fixture and recognition off; never changes analog or speaker settings.

#[cfg(feature = "capture-fixture")]
fn main() -> anyhow::Result<()> {
    esp_shazam_firmware::run()
}

#[cfg(not(feature = "capture-fixture"))]
fn main() -> anyhow::Result<()> {
    anyhow::bail!("capture-publication requires --features capture-fixture")
}
