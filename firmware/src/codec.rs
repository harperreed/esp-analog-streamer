// ABOUTME: ES8388 setup for recording the LINE IN jack (LIN2/RIN2) at 48 kHz, 24-bit, as an I2S slave.
// ABOUTME: Register sequence from Espressif's esp_codec_dev ES8388 driver, trimmed to the ADC path, checked by readback.

use std::time::Duration;

use anyhow::{bail, Context, Result};
use esp_idf_svc::hal::delay::TickType;
use esp_idf_svc::hal::i2c::I2cDriver;

const ADDR: u8 = 0x10;

/// (register, value) in order, up to the state-machine restart.
const SETUP: &[(u8, u8)] = &[
    (0x00, 0x86), // soft-reset pulse: no reset pin, and an ESP32 reset leaves the codec as it was
    (0x00, 0x06), // back to defaults
    (0x01, 0x50), // analog and bias on
    (0x02, 0xFF), // chip power all off...
    (0x02, 0x00), // ...then all on
    (0x08, 0x00), // I2S slave: the chip powers up as master
    (0x00, 0x12), // same sample rate for ADC and DAC, VMID 500k
    (0x2B, 0x80), // ADC and DAC share the one LRCK pin
    (0x03, 0xFF), // ADC off while configuring
    (0x0A, 0x50), // input LIN2/RIN2: the LINE IN jack
    (0x0B, 0x02), // stereo; ADC data output driven
    (0x0C, 0x00), // I2S Philips format, 24-bit words (sent in the top of the ESP32's 32-bit slots)
    (0x0D, 0x02), // single speed, MCLK = 256 x LRCK
    (0x10, 0x00), // left ADC digital volume 0 dB (powers up at -96 dB)
    (0x11, 0x00), // right ADC digital volume 0 dB
];

/// Final values that prove the setup took.
const EXPECTED: &[(u8, u8)] = &[
    (0x02, 0x00),
    (0x03, 0x00),
    (0x08, 0x00),
    (0x0A, 0x50),
    (0x0C, 0x00),
    (0x0D, 0x02),
    (0x10, 0x00),
    (0x11, 0x00),
];

/// Needs MCLK, BCLK and LRCK already running: the codec locks onto them when its state machines restart.
/// `pga_gain` goes to register 0x09, 3 dB per step per nibble: 0x00 is 0 dB on both channels, 0x88 is +24 dB.
/// Line level wants 0 dB: full scale is then 1.0 Vrms.
pub fn setup_line_in(i2c: &mut I2cDriver<'_>, pga_gain: u8) -> Result<()> {
    for &(reg, value) in SETUP {
        write(i2c, reg, value)?;
    }
    write(i2c, 0x09, pga_gain)?;
    write(i2c, 0x02, 0xF0)?; // restart the state machines with the final configuration
    std::thread::sleep(Duration::from_millis(2));
    write(i2c, 0x02, 0x00)?;
    write(i2c, 0x03, 0x00)?; // ADC, analog inputs and bias on

    for &(reg, want) in EXPECTED {
        expect(i2c, reg, want)?;
    }
    expect(i2c, 0x09, pga_gain)?;
    log::info!("codec: ES8388 set up for LINE IN, PGA 0x{pga_gain:02x}");
    Ok(())
}

fn expect(i2c: &mut I2cDriver<'_>, reg: u8, want: u8) -> Result<()> {
    let got = read(i2c, reg)?;
    if got != want {
        bail!("ES8388 register 0x{reg:02x} reads 0x{got:02x}, expected 0x{want:02x}");
    }
    Ok(())
}

fn write(i2c: &mut I2cDriver<'_>, reg: u8, value: u8) -> Result<()> {
    i2c.write(ADDR, &[reg, value], TickType::new_millis(50).ticks())
        .with_context(|| format!("ES8388 write 0x{reg:02x} = 0x{value:02x}"))
}

fn read(i2c: &mut I2cDriver<'_>, reg: u8) -> Result<u8> {
    let mut value = [0u8; 1];
    i2c.write_read(ADDR, &[reg], &mut value, TickType::new_millis(50).ticks())
        .with_context(|| format!("ES8388 read 0x{reg:02x}"))?;
    Ok(value[0])
}
