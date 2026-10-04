// ABOUTME: Spawns a std thread with a FreeRTOS task name, priority, core and stack size.
// ABOUTME: ESP-IDF takes the first three from the spawning thread's pthread config, which this sets for the spawn and then resets.

use std::ffi::CStr;

use anyhow::Result;
use esp_idf_svc::hal::cpu::Core;
use esp_idf_svc::hal::task::thread::ThreadSpawnConfiguration;

/// Where a thread runs, at what priority, and how big its stack is. Stacks come out of internal RAM.
#[derive(Clone, Copy)]
pub struct ThreadSpec {
    /// The FreeRTOS task name; FreeRTOS keeps the first 15 bytes.
    pub name: &'static CStr,
    pub core: Core,
    pub priority: u8,
    pub stack_bytes: usize,
}

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
        ThreadSpawnConfiguration::default().set()?;
        spawned?;
        Ok(())
    }
}
