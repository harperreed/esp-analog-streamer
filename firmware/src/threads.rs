// ABOUTME: Spawns a std thread with a FreeRTOS task name, priority, core and stack size.
// ABOUTME: ESP-IDF takes the first three from the spawning thread's pthread config, which this sets for the spawn and then resets.

#[cfg(target_os = "espidf")]
use std::ffi::CStr;

#[cfg(target_os = "espidf")]
use anyhow::Result;
#[cfg(target_os = "espidf")]
use esp_idf_svc::hal::cpu::Core;
#[cfg(target_os = "espidf")]
use esp_idf_svc::hal::task::thread::ThreadSpawnConfiguration;

/// Where a thread runs, at what priority, and how big its stack is. Stacks come out of internal RAM.
#[derive(Clone, Copy)]
#[cfg(target_os = "espidf")]
pub struct ThreadSpec {
    /// The FreeRTOS task name; FreeRTOS keeps the first 15 bytes.
    pub name: &'static CStr,
    pub core: Core,
    pub priority: u8,
    pub stack_bytes: usize,
}

#[cfg(target_os = "espidf")]
impl ThreadSpec {
    /// Runs `f` on a new thread as specified.
    pub fn spawn(self, f: impl FnOnce() + Send + 'static) -> Result<()> {
        ThreadSpawnConfiguration {
            name: Some(self.name),
            priority: self.priority,
            pin_to_core: Some(self.core),
            ..Default::default()
        }
        .set()?;
        // Only Builder::stack_size sets a std thread's stack; ThreadSpawnConfiguration's is ignored.
        let spawned = std::thread::Builder::new()
            .stack_size(self.stack_bytes)
            .spawn(f);
        // Reset even when the spawn failed, so this thread's later spawns get the defaults.
        finish_spawn(
            spawned.map_err(anyhow::Error::from),
            ThreadSpawnConfiguration::default()
                .set()
                .map_err(anyhow::Error::from),
            |error| {
                log::error!(
                    "thread {:?}: pthread config reset failed: {error}",
                    self.name
                )
            },
        )
    }
}

/// A reset failure is reported separately from whether the thread was created.
pub(crate) fn finish_spawn<T, E>(
    spawned: std::result::Result<T, E>,
    reset: std::result::Result<(), E>,
    reset_failed: impl FnOnce(E),
) -> std::result::Result<(), E> {
    if let Err(error) = reset {
        reset_failed(error);
    }
    spawned.map(|_| ())
}
