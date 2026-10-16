pub(crate) type Result<T> = core::result::Result<T, Error>;

/// Errors returned by [`crate::Ota`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Error {
    /// The image does not fit into the target OTA partition.
    PartitionTooSmall,
    /// The partition table has less than 2 OTA app partitions.
    NotEnoughPartitions,
    /// The partition table has no `otadata` partition of at least 0x2000 bytes.
    NoOtadata,
    /// OTA partitions are not consecutive (`ota_0`, `ota_1`, ...).
    PartitionOrder,
    /// [`crate::Ota::begin`] (or [`crate::Ota::resume`]) was not called yet.
    NotStarted,
    /// Not the whole image was written yet.
    Incomplete,
    /// The CRC of the written data does not match the expected one.
    Crc,
    /// Reading the image back from flash gave a different CRC.
    Verify,
    /// Reading or writing the flash failed.
    Flash,
    /// The running partition could not be found in `otadata`.
    UnknownBootPartition,
    /// The `otadata` sequence number cannot be increased any further.
    SeqExhausted,
    /// [`ResumeInfo`] is inconsistent (more remaining than the image size).
    InvalidResumeInfo,
}

impl core::fmt::Display for Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(match self {
            Self::PartitionTooSmall => "image does not fit into the OTA partition",
            Self::NotEnoughPartitions => "less than 2 OTA partitions",
            Self::NoOtadata => "no valid otadata partition",
            Self::PartitionOrder => "OTA partitions are not consecutive",
            Self::NotStarted => "OTA update not started",
            Self::Incomplete => "image not completely written",
            Self::Crc => "image crc mismatch",
            Self::Verify => "image read back from flash has a different crc",
            Self::Flash => "flash read/write failed",
            Self::UnknownBootPartition => "running partition not found in otadata",
            Self::SeqExhausted => "otadata sequence number exhausted",
            Self::InvalidResumeInfo => "inconsistent resume info",
        })
    }
}

impl core::error::Error for Error {}

/// Progress of the running update, see [`crate::Ota::progress`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct Progress {
    /// Bytes written so far.
    pub written: u32,
    /// Size of the whole image.
    pub total: u32,
}

impl Progress {
    /// Progress in percent (`0..=100`).
    pub fn percent(&self) -> u8 {
        (self.permille() / 10) as u8
    }

    /// Progress in permille (`0..=1000`).
    pub fn permille(&self) -> u16 {
        match self.total {
            0 => 1000,
            total => (u64::from(self.written.min(total)) * 1000 / u64::from(total)) as u16,
        }
    }
}

/// What to persist to continue an interrupted update with [`crate::Ota::resume`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct ResumeInfo {
    /// Size of the whole image.
    pub size: u32,
    /// Bytes still to be written.
    pub remaining: u32,
    /// Expected crc32 of the whole image.
    pub target_crc: u32,
    /// crc32 of the bytes written so far.
    pub last_crc: u32,
}

/// State of an OTA update in progress.
#[derive(Clone, Debug)]
pub(crate) struct FlashProgress {
    pub last_crc: u32,
    pub flash_offset: u32,
    pub flash_size: u32,
    pub remaining: u32,

    pub target_partition: usize,
    pub target_crc: u32,
}

/// OTA related partitions read from the partition table.
#[derive(Debug)]
pub(crate) struct PartitionInfo {
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
    New = 0x0,
    PendingVerify = 0x1,
    Valid = 0x2,
    Invalid = 0x3,
    Aborted = 0x4,
    Undefined = 0xFFFFFFFF,
}

impl OtaImgState {
    /// Unknown values are treated like `Undefined`, as the bootloader does
    /// (only `Invalid` and `Aborted` reject an image).
    pub(crate) const fn from_u32(value: u32) -> Self {
        match value {
            0x0 => Self::New,
            0x1 => Self::PendingVerify,
            0x2 => Self::Valid,
            0x3 => Self::Invalid,
            0x4 => Self::Aborted,
            _ => Self::Undefined,
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

    /// Sets `seq` to 0 (invalid entry) if its crc does not match.
    pub(crate) fn check_crc(&mut self) {
        if !crate::helpers::is_crc_seq_correct(self.seq, self.crc) {
            self.seq = 0; // set seq to 0 if crc not correct!
        }
    }
}
