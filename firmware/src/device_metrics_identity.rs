// ABOUTME: Holds an immutable boot token and the running application's ELF digest.
// ABOUTME: Exports fixed hexadecimal values without device addresses or names.
use std::fmt;
use std::sync::OnceLock;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Hex<const N: usize>(pub [u8; N]);

impl<const N: usize> fmt::Display for Hex<N> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in self.0 {
            write!(formatter, "{byte:02x}")?;
        }
        Ok(())
    }
}

impl<const N: usize> serde::Serialize for Hex<N> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
pub(crate) struct Identity {
    pub boot_token: Hex<16>,
    pub build_elf_sha256: Option<Hex<32>>,
}

impl Identity {
    pub(crate) fn new(boot_token: [u8; 16], build_digest: Option<[u8; 32]>) -> Self {
        Self {
            boot_token: Hex(boot_token),
            build_elf_sha256: build_digest
                .filter(|digest| *digest != [0; 32] && *digest != [255; 32])
                .map(Hex),
        }
    }
}

pub(crate) static IDENTITY: OnceLock<Identity> = OnceLock::new();

/// Call only after the Wi-Fi driver starts, so the RF entropy source is active.
#[cfg(target_os = "espidf")]
pub(crate) fn initialize_after_wifi() -> &'static Identity {
    IDENTITY.get_or_init(|| {
        let mut token = [0; 16];
        // SAFETY: The buffer holds exactly the number of bytes requested.
        unsafe { esp_idf_svc::sys::esp_fill_random(token.as_mut_ptr().cast(), token.len()) };
        // SAFETY: IDF returns a pointer to the running application's static descriptor.
        let descriptor = unsafe { esp_idf_svc::sys::esp_app_get_description().as_ref() };
        let digest = descriptor
            .filter(|descriptor| descriptor.magic_word == 0xabcd_5432)
            .map(|descriptor| descriptor.app_elf_sha256);
        Identity::new(token, digest)
    })
}
