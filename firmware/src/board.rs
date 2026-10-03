// ABOUTME: Bring-up only: probes both known Audio Kit I2C pin sets to find which codec revision is fitted.
// ABOUTME: Pin sets come from the Hardware table in the spec.

use anyhow::Result;
use esp_idf_svc::hal::delay::TickType;
use esp_idf_svc::hal::gpio::{InputPin, OutputPin};
use esp_idf_svc::hal::i2c::{I2c, I2cConfig, I2cDriver};
use esp_idf_svc::hal::units::FromValueType;

const ES8388_ADDR: u8 = 0x10;
const AC101_ADDR: u8 = 0x1A;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Revision {
    /// ES8388; I2C on SDA 33 / SCL 32; BCLK on GPIO27.
    Es8388V1,
    /// ES8388; I2C on SDA 18 / SCL 23; BCLK on GPIO5.
    Es8388V2,
    /// AC101; I2C on SDA 33 / SCL 32.
    Ac101,
}

fn open<'d>(
    i2c: impl I2c + 'd,
    sda: impl InputPin + OutputPin + 'd,
    scl: impl InputPin + OutputPin + 'd,
) -> Result<I2cDriver<'d>> {
    // I2cConfig defaults to 1 MHz; the codecs want 100 kHz.
    Ok(I2cDriver::new(
        i2c,
        sda,
        scl,
        &I2cConfig::new().baudrate(100.kHz().into()),
    )?)
}

/// Reads N bytes from `reg`; None when nothing ACKs.
fn read<const N: usize>(bus: &mut I2cDriver<'_>, addr: u8, reg: u8) -> Option<[u8; N]> {
    let mut value = [0u8; N];
    bus.write_read(addr, &[reg], &mut value, TickType::new_millis(50).ticks())
        .ok()
        .map(|()| value)
}

/// Asks the v1 pin set (on I2C0) for an ES8388 or AC101, then the v2 pin set (on I2C1) for an ES8388.
pub fn probe<'d>(
    i2c0: impl I2c + 'd,
    sda_v1: impl InputPin + OutputPin + 'd,
    scl_v1: impl InputPin + OutputPin + 'd,
    i2c1: impl I2c + 'd,
    sda_v2: impl InputPin + OutputPin + 'd,
    scl_v2: impl InputPin + OutputPin + 'd,
) -> Result<Option<Revision>> {
    let mut v1 = open(i2c0, sda_v1, scl_v1)?;
    if let Some([reg08]) = read::<1>(&mut v1, ES8388_ADDR, 0x08) {
        log::info!("bring-up: ES8388 answered on SDA 33 / SCL 32 (reg 0x08 = 0x{reg08:02x})");
        return Ok(Some(Revision::Es8388V1));
    }
    if let Some(id) = read::<2>(&mut v1, AC101_ADDR, 0x00) {
        let id = u16::from_be_bytes(id);
        log::info!("bring-up: 0x1A answered on SDA 33 / SCL 32 (reg 0x00 = 0x{id:04x}; an AC101 reads 0x0101)");
        return Ok((id == 0x0101).then_some(Revision::Ac101));
    }
    let mut v2 = open(i2c1, sda_v2, scl_v2)?;
    if let Some([reg08]) = read::<1>(&mut v2, ES8388_ADDR, 0x08) {
        log::info!("bring-up: ES8388 answered on SDA 18 / SCL 23 (reg 0x08 = 0x{reg08:02x})");
        return Ok(Some(Revision::Es8388V2));
    }
    Ok(None)
}
