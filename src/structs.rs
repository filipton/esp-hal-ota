pub(crate) type Result<T> = core::result::Result<T, OtaError>;

/// Errors returned by [`crate::Ota`].
#[derive(Debug, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum OtaError {
    /// The image does not fit into the target OTA partition.
    OtaPartitionTooSmall,
    /// The partition table has less than 2 OTA app partitions, or no valid `otadata` partition.
    NotEnoughPartitions,
    /// [`crate::Ota::ota_begin`] (or [`crate::Ota::ota_resume`]) was not called yet.
    OtaNotStarted,
    /// Reading or writing the flash failed.
    FlashRWError,
    /// The CRC of the written image does not match the expected one.
    WrongCRC,
    /// OTA partitions are not consecutive (`ota_0`, `ota_1`, ...).
    WrongOTAPArtitionOrder,
    /// Reading the image back from flash gave a different CRC.
    OtaVerifyError,
    /// The currently running partition could not be found in `otadata`.
    CannotFindCurrentBootPartition,
}

/// State of an OTA update in progress.
#[derive(Clone, Debug)]
pub struct FlashProgress {
    pub last_crc: u32,
    pub flash_offset: u32,
    pub flash_size: u32,
    pub remaining: u32,

    pub target_partition: usize,
    pub target_crc: u32,
}

/// OTA related partitions read from the partition table.
#[derive(Debug)]
pub struct PartitionInfo {
    /// `(offset, size)` of `ota_0`, `ota_1`, ... (only the first `ota_partitions_count` are valid).
    pub ota_partitions: [(u32, u32); 16],
    pub ota_partitions_count: usize,

    pub otadata_offset: u32,
    pub otadata_size: u32,
}

/// `esp_ota_img_states_t` from ESP-IDF.
#[repr(u32)]
#[derive(Debug, PartialEq, Clone, Copy)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum OtaImgState {
    EspOtaImgNew = 0x0,
    EspOtaImgPendingVerify = 0x1,
    EspOtaImgValid = 0x2,
    EspOtaImgInvalid = 0x3,
    EspOtaImgAborted = 0x4,
    EspOtaImgUndefined = 0xFFFFFFFF,
}

impl OtaImgState {
    /// Unknown values are treated like `Undefined`, as the bootloader does
    /// (only `Invalid` and `Aborted` reject an image).
    pub(crate) const fn from_u32(value: u32) -> Self {
        match value {
            0x0 => Self::EspOtaImgNew,
            0x1 => Self::EspOtaImgPendingVerify,
            0x2 => Self::EspOtaImgValid,
            0x3 => Self::EspOtaImgInvalid,
            0x4 => Self::EspOtaImgAborted,
            _ => Self::EspOtaImgUndefined,
        }
    }
}

/// One `otadata` entry (`esp_ota_select_entry_t` from ESP-IDF).
#[repr(C)]
#[derive(Debug, Clone)]
pub struct EspOtaSelectEntry {
    pub seq: u32,
    pub seq_label: [u8; 20],
    pub ota_state: OtaImgState,
    pub crc: u32,
}

impl EspOtaSelectEntry {
    pub(crate) const SIZE: usize = 32;

    pub(crate) fn from_bytes(bytes: &[u8; Self::SIZE]) -> Self {
        let [s0, s1, s2, s3, label @ .., t0, t1, t2, t3, c0, c1, c2, c3] = *bytes;
        Self {
            seq: u32::from_le_bytes([s0, s1, s2, s3]),
            seq_label: label,
            ota_state: OtaImgState::from_u32(u32::from_le_bytes([t0, t1, t2, t3])),
            crc: u32::from_le_bytes([c0, c1, c2, c3]),
        }
    }

    pub(crate) fn to_bytes(&self) -> [u8; Self::SIZE] {
        let mut bytes = [0; Self::SIZE];
        bytes[0..4].copy_from_slice(&self.seq.to_le_bytes());
        bytes[4..24].copy_from_slice(&self.seq_label);
        bytes[24..28].copy_from_slice(&(self.ota_state as u32).to_le_bytes());
        bytes[28..32].copy_from_slice(&self.crc.to_le_bytes());
        bytes
    }

    /// Check if crc(of seq) is correct, if not - its setting seq to 0
    pub fn check_crc(&mut self) {
        if !crate::helpers::is_crc_seq_correct(self.seq, self.crc) {
            self.seq = 0; // set seq to 0 if crc not correct!
        }
    }
}
