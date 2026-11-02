// ABOUTME: Checks whether the station's radio readback matches the requested streaming policy.
// ABOUTME: Keeps comparison independent of ESP-IDF so host tests cover rejected radio settings.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RadioPolicy {
    pub protocol: u8,
    pub power_save: u32,
}

#[derive(Debug, PartialEq, Eq)]
pub struct PolicyMismatch {
    pub expected: RadioPolicy,
    pub effective: RadioPolicy,
}

impl RadioPolicy {
    pub fn verify(self, effective: Self) -> Result<(), PolicyMismatch> {
        if self == effective {
            Ok(())
        } else {
            Err(PolicyMismatch {
                expected: self,
                effective,
            })
        }
    }
}
