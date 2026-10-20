// ABOUTME: The line input's gain at runtime: loads the persisted dB from NVS, owns the codec's I2C bus after boot,
// ABOUTME: writes the PGA register with read-back on change, and publishes the live register for /api/metrics.
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Mutex;

use anyhow::{anyhow, Context, Result};
use esp_idf_svc::hal::i2c::I2cDriver;
use esp_idf_svc::nvs::{EspDefaultNvsPartition, EspNvs, NvsDefault};
use esp_shazam_core::input_gain::InputGain;

const NAMESPACE: &str = "input";
const KEY: &str = "gain_db";

/// Register 0x09 as last written and read back, for `/api/metrics`' `settings.pga_gain`.
pub static LIVE_REGISTER: AtomicU8 = AtomicU8::new(0);

/// The persisted gain, or the build's `pga_gain` register when nothing is saved or the saved value is not a step.
pub fn load(
    partition: EspDefaultNvsPartition,
    fallback_register: u8,
) -> Result<(EspNvs<NvsDefault>, InputGain)> {
    let nvs = EspNvs::new(partition, NAMESPACE, true).context("input gain NVS namespace")?;
    let fallback = InputGain::from_register(fallback_register).unwrap_or(InputGain::ZERO);
    let gain = match nvs.get_u8(KEY) {
        Ok(Some(db)) => InputGain::from_db(db).unwrap_or_else(|error| {
            log::warn!(
                "input gain: saved {db} dB is not usable ({error}); using {} dB",
                fallback.db()
            );
            fallback
        }),
        Ok(None) => fallback,
        Err(error) => {
            log::warn!(
                "input gain: cannot read the saved value ({error}); using {} dB",
                fallback.db()
            );
            fallback
        }
    };
    Ok((nvs, gain))
}

pub struct Handle {
    i2c: Mutex<I2cDriver<'static>>,
    nvs: Mutex<EspNvs<NvsDefault>>,
    gain: Mutex<InputGain>,
}

impl Handle {
    /// `gain` is what the codec setup already wrote at boot.
    pub fn new(i2c: I2cDriver<'static>, nvs: EspNvs<NvsDefault>, gain: InputGain) -> Self {
        LIVE_REGISTER.store(gain.register(), Ordering::Relaxed);
        Handle {
            i2c: Mutex::new(i2c),
            nvs: Mutex::new(nvs),
            gain: Mutex::new(gain),
        }
    }

    pub fn gain(&self) -> InputGain {
        *self.gain.lock().unwrap()
    }

    /// Writes the codec first and persists only a value the codec read back; a failed write leaves the old gain.
    pub fn apply(&self, gain: InputGain) -> Result<()> {
        let mut i2c = self
            .i2c
            .try_lock()
            .map_err(|_| anyhow!("input gain busy"))?;
        crate::codec::set_pga_gain(&mut i2c, gain.register())?;
        LIVE_REGISTER.store(gain.register(), Ordering::Relaxed);
        *self.gain.lock().unwrap() = gain;
        drop(i2c);
        self.nvs
            .lock()
            .unwrap()
            .set_u8(KEY, gain.db())
            .context("input gain persist")?;
        log::info!(
            "input gain: {} dB (register 0x{:02x}) applied and saved",
            gain.db(),
            gain.register()
        );
        Ok(())
    }
}
