// ABOUTME: Checks an uploaded app image before a byte reaches flash: the ESP32 image header, the embedded app
// ABOUTME: descriptor (version, ELF sha) and the size against the slot; and compares the upload token in constant time.

/// Bytes needed to judge an image: the 24-byte image header, the 8-byte first segment header and the 256-byte app
/// descriptor that ESP-IDF places first in the first segment.
pub const HEAD_BYTES: usize = 32 + 256;
const IMAGE_MAGIC: u8 = 0xE9;
const DESCRIPTOR_MAGIC: u32 = 0xABCD_5432;
const CHIP_ESP32: u16 = 0x0000;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AppDescription {
    pub version: String,
    pub project: String,
    pub idf_version: String,
    pub elf_sha256: [u8; 32],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImageError {
    /// Fewer than HEAD_BYTES were offered.
    Short,
    NotAnImage,
    NotForEsp32,
    NoDescriptor,
    /// Longer than the update slot.
    TooLarge {
        bytes: usize,
        slot: usize,
    },
}

impl std::fmt::Display for ImageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ImageError::Short => write!(f, "image shorter than its header"),
            ImageError::NotAnImage => write!(f, "not an ESP32 app image"),
            ImageError::NotForEsp32 => write!(f, "image built for another chip"),
            ImageError::NoDescriptor => write!(f, "image carries no app descriptor"),
            ImageError::TooLarge { bytes, slot } => {
                write!(f, "image of {bytes} bytes exceeds the {slot}-byte slot")
            }
        }
    }
}

fn text(bytes: &[u8]) -> String {
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    String::from_utf8_lossy(&bytes[..end]).into_owned()
}

/// Judges the first HEAD_BYTES of an upload. `declared` is the Content-Length, judged against the slot before
/// anything is written.
pub fn inspect(head: &[u8], declared: usize, slot: usize) -> Result<AppDescription, ImageError> {
    if declared > slot {
        return Err(ImageError::TooLarge {
            bytes: declared,
            slot,
        });
    }
    if head.len() < HEAD_BYTES {
        return Err(ImageError::Short);
    }
    if head[0] != IMAGE_MAGIC {
        return Err(ImageError::NotAnImage);
    }
    if u16::from_le_bytes([head[12], head[13]]) != CHIP_ESP32 {
        return Err(ImageError::NotForEsp32);
    }
    let d = &head[32..32 + 256];
    if u32::from_le_bytes([d[0], d[1], d[2], d[3]]) != DESCRIPTOR_MAGIC {
        return Err(ImageError::NoDescriptor);
    }
    let mut elf_sha256 = [0u8; 32];
    elf_sha256.copy_from_slice(&d[144..176]);
    Ok(AppDescription {
        version: text(&d[16..48]),
        project: text(&d[48..80]),
        idf_version: text(&d[112..144]),
        elf_sha256,
    })
}

/// Same length and same bytes, without an early exit on the first difference.
pub fn token_matches(expected: &str, offered: &str) -> bool {
    if expected.is_empty() {
        return false;
    }
    let (a, b) = (expected.as_bytes(), offered.as_bytes());
    let mut diff = (a.len() ^ b.len()) as u8;
    for i in 0..a.len().max(b.len()) {
        diff |= a.get(i).copied().unwrap_or(0) ^ b.get(i).copied().unwrap_or(0);
    }
    diff == 0
}

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    // The first 288 bytes of the home board's input-gain image (espflash save-image of ELF 008bc545…).
    const HEAD_HEX: &str = "e906022fbc520840ee00000000000000008f0100000000012000403ff4e305003254cdab00000000000000000000000032326366653335000000000000000000000000000000000000000000000000006c6962657370696466000000000000000000000000000000000000000000000030383a30353a323300000000000000004f63742031312032303236000000000076352e352e350000000000000000000000000000000000000000000000000000008bc545b14430a4c230f39495885ec11b7579ed1b1a24975a42652f533e92dc000063001000000000000000000000000000000000000000000000000000000000000000000000000000000000000000";

    fn head() -> Vec<u8> {
        let mut bytes: Vec<u8> = (0..HEAD_HEX.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&HEAD_HEX[i..i + 2], 16).unwrap())
            .collect();
        bytes.resize(HEAD_BYTES, 0);
        bytes
    }

    #[test]
    fn a_real_image_head_reports_its_descriptor() {
        let description = inspect(&head(), 1_957_568, 2_031_616).unwrap();
        assert_eq!(description.version, "22cfe35");
        assert_eq!(description.project, "libespidf");
        assert_eq!(description.idf_version, "v5.5.5");
        assert_eq!(
            hex(&description.elf_sha256),
            "008bc545b14430a4c230f39495885ec11b7579ed1b1a24975a42652f533e92dc"
        );
    }

    #[test]
    fn size_is_judged_before_the_bytes() {
        assert_eq!(
            inspect(&head(), 2_031_617, 2_031_616),
            Err(ImageError::TooLarge {
                bytes: 2_031_617,
                slot: 2_031_616
            })
        );
        assert_eq!(
            inspect(&[], 2_031_617, 2_031_616),
            Err(ImageError::TooLarge {
                bytes: 2_031_617,
                slot: 2_031_616
            })
        );
        assert_eq!(
            inspect(&head()[..100], 100, 2_031_616),
            Err(ImageError::Short)
        );
    }

    #[test]
    fn wrong_magic_chip_or_descriptor_is_refused() {
        let mut bytes = head();
        bytes[0] = 0x7f;
        assert_eq!(
            inspect(&bytes, 1000, 2_031_616),
            Err(ImageError::NotAnImage)
        );
        let mut bytes = head();
        bytes[12] = 0x09; // ESP32-S3
        assert_eq!(
            inspect(&bytes, 1000, 2_031_616),
            Err(ImageError::NotForEsp32)
        );
        let mut bytes = head();
        bytes[32] = 0;
        assert_eq!(
            inspect(&bytes, 1000, 2_031_616),
            Err(ImageError::NoDescriptor)
        );
        let elf = vec![0x7f, b'E', b'L', b'F']
            .into_iter()
            .chain(std::iter::repeat(0))
            .take(HEAD_BYTES)
            .collect::<Vec<_>>();
        assert_eq!(
            inspect(&elf, HEAD_BYTES, 2_031_616),
            Err(ImageError::NotAnImage),
            "an ELF is not an image"
        );
    }

    #[test]
    fn token_comparison_needs_the_whole_secret_and_refuses_an_empty_one() {
        assert!(token_matches("correct horse", "correct horse"));
        assert!(!token_matches("correct horse", "correct hors"));
        assert!(!token_matches("correct horse", "correct horsey"));
        assert!(!token_matches("correct horse", ""));
        assert!(
            !token_matches("", ""),
            "an empty configured token disables uploads"
        );
        assert!(!token_matches("", "anything"));
    }

    #[test]
    fn errors_are_sentences() {
        assert_eq!(
            ImageError::TooLarge { bytes: 5, slot: 4 }.to_string(),
            "image of 5 bytes exceeds the 4-byte slot"
        );
        assert_eq!(ImageError::NotAnImage.to_string(), "not an ESP32 app image");
    }
}
