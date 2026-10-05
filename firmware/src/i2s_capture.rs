// ABOUTME: Owns ESP-IDF I2S RX for 48 kHz stereo LINE IN with the board's clock and DMA settings.
// ABOUTME: Counts receive and queue-overflow interrupts separately without allocating in callbacks.

use std::cell::Cell;
use std::ffi::c_void;
use std::marker::PhantomData;
use std::ptr;
use std::sync::atomic::{AtomicU32, Ordering};

use esp_idf_svc::hal::gpio::{InputPin, OutputPin};
use esp_idf_svc::hal::i2s::I2s;
use esp_idf_svc::sys::{self, esp, EspError};

// Static internal DRAM outlives every callback, including constructor failure and restart.
#[link_section = ".dram1.capture_callbacks"]
static CALLBACKS: AtomicU32 = AtomicU32::new(0);
#[link_section = ".dram1.capture_overflows"]
static QUEUE_OVERFLOWS: AtomicU32 = AtomicU32::new(0);

/// Wrapping, since-boot counts. Neither restarting nor recreating the channel resets them.
/// Queue overflow does not detect every DMA interrupt omission.
#[derive(Debug, Clone, Copy)]
pub struct CaptureCounters {
    pub callbacks: u32,
    pub queue_overflows: u32,
}

pub fn counters() -> CaptureCounters {
    CaptureCounters {
        callbacks: CALLBACKS.load(Ordering::Relaxed),
        queue_overflows: QUEUE_OVERFLOWS.load(Ordering::Relaxed),
    }
}

#[link_section = ".iram1.capture_received"]
unsafe extern "C" fn received(
    _handle: sys::i2s_chan_handle_t,
    _event: *mut sys::i2s_event_data_t,
    _context: *mut c_void,
) -> bool {
    CALLBACKS.fetch_add(1, Ordering::Relaxed);
    false
}

#[link_section = ".iram1.capture_overflow"]
unsafe extern "C" fn overflow(
    _handle: sys::i2s_chan_handle_t,
    _event: *mut sys::i2s_event_data_t,
    _context: *mut c_void,
) -> bool {
    QUEUE_OVERFLOWS.fetch_add(1, Ordering::Relaxed);
    false
}

/// Sole owner of the channel and consumed peripheral/pin lifetimes. Not shared between threads.
pub struct CaptureRx<'d> {
    handle: sys::i2s_chan_handle_t,
    running: bool,
    _ownership: PhantomData<&'d mut Cell<()>>,
}

// IDF permits moving a channel; all operations require exclusive ownership. Static callbacks
// touch only atomics, never the owner. The Cell marker prevents Sync.
unsafe impl Send for CaptureRx<'_> {}

impl<'d> CaptureRx<'d> {
    /// Starts clocks before codec setup, consuming the same resources as the HAL driver.
    pub fn new<I: I2s + 'd>(
        _i2s: I,
        bclk: impl InputPin + OutputPin + 'd,
        din: impl InputPin + 'd,
        mclk: impl InputPin + OutputPin + 'd,
        ws: impl InputPin + OutputPin + 'd,
    ) -> Result<Self, EspError> {
        let channel = sys::i2s_chan_config_t {
            id: I::port(),
            role: sys::i2s_role_t_I2S_ROLE_MASTER,
            dma_desc_num: 10,
            dma_frame_num: 240,
            ..Default::default()
        };
        let config = sys::i2s_std_config_t {
            clk_cfg: sys::i2s_std_clk_config_t {
                sample_rate_hz: 48_000,
                clk_src: sys::soc_periph_i2s_clk_src_t_I2S_CLK_SRC_APLL,
                mclk_multiple: sys::i2s_mclk_multiple_t_I2S_MCLK_MULTIPLE_256,
                ..Default::default()
            },
            slot_cfg: sys::i2s_std_slot_config_t {
                data_bit_width: sys::i2s_data_bit_width_t_I2S_DATA_BIT_WIDTH_32BIT,
                slot_bit_width: sys::i2s_slot_bit_width_t_I2S_SLOT_BIT_WIDTH_AUTO,
                slot_mode: sys::i2s_slot_mode_t_I2S_SLOT_MODE_STEREO,
                slot_mask: sys::i2s_std_slot_mask_t_I2S_STD_SLOT_BOTH,
                ws_width: 32,
                ws_pol: false,
                bit_shift: true,
                msb_right: false,
            },
            gpio_cfg: sys::i2s_std_gpio_config_t {
                mclk: i32::from(mclk.pin()),
                bclk: i32::from(bclk.pin()),
                ws: i32::from(ws.pin()),
                dout: -1,
                din: i32::from(din.pin()),
                ..Default::default()
            },
        };
        let mut owner = Self {
            handle: ptr::null_mut(),
            running: false,
            _ownership: PhantomData,
        };
        // All config pointers remain valid during calls. Owner cleanup covers every later error.
        unsafe {
            esp!(sys::i2s_new_channel(
                &channel,
                ptr::null_mut(),
                &mut owner.handle
            ))?;
            esp!(sys::i2s_channel_init_std_mode(owner.handle, &config))?;
            let callbacks = sys::i2s_event_callbacks_t {
                on_recv: Some(received),
                on_recv_q_ovf: Some(overflow),
                ..Default::default()
            };
            esp!(sys::i2s_channel_register_event_callback(
                owner.handle,
                &callbacks,
                ptr::null_mut()
            ))?;
            esp!(sys::i2s_channel_enable(owner.handle))?;
        }
        owner.running = true;
        Ok(owner)
    }

    /// Reads synchronously in milliseconds; a timeout with partial data returns that data.
    pub fn read(&mut self, bytes: &mut [u8], timeout_ms: u32) -> Result<usize, EspError> {
        if bytes.is_empty() {
            return Ok(0);
        }
        let mut read = 0;
        // The exclusive slice and live owned handle remain valid for this blocking call.
        let result = unsafe {
            esp!(sys::i2s_channel_read(
                self.handle,
                bytes.as_mut_ptr().cast(),
                bytes.len(),
                &mut read,
                timeout_ms
            ))
        };
        match result {
            Ok(()) => Ok(read),
            Err(error) if error.code() == sys::ESP_ERR_TIMEOUT && read > 0 => Ok(read),
            Err(error) => Err(error),
        }
    }

    pub fn restart(&mut self) -> Result<(), EspError> {
        // Only this owner operates the channel. A failed enable leaves it disabled for retry.
        unsafe {
            if self.running {
                esp!(sys::i2s_channel_disable(self.handle))?;
                self.running = false;
            }
            esp!(sys::i2s_channel_enable(self.handle))?;
        }
        self.running = true;
        Ok(())
    }
}

impl Drop for CaptureRx<'_> {
    fn drop(&mut self) {
        if self.handle.is_null() {
            return;
        }
        // The owned handle is live; callback context is static even if cleanup reports an error.
        unsafe {
            if self.running {
                let _ = sys::i2s_channel_disable(self.handle);
            }
            let callbacks = sys::i2s_event_callbacks_t::default();
            let _ =
                sys::i2s_channel_register_event_callback(self.handle, &callbacks, ptr::null_mut());
            let _ = sys::i2s_del_channel(self.handle);
        }
    }
}
