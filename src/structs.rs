pub(crate) type Result<T> = core::result::Result<T, OtaError>;

#[derive(Debug, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum OtaError {
    NotEnoughPartitions,
    OtaNotStarted,
    FlashRWError,
    WrongCRC,
    WrongOTAPArtitionOrder,
    OtaVerifyError,
    CannotFindCurrentBootPartition,
}

#[derive(Clone)]
pub struct FlashProgress {
    pub last_crc: u32,
    pub flash_offset: u32,
    pub flash_size: u32,
    pub remaining: u32,

    pub target_partition: usize,
    pub target_crc: u32,
}

#[derive(Debug)]
pub struct PartitionInfo {
    pub ota_partitions: [(u32, u32); 16],
    pub ota_partitions_count: usize,

    pub otadata_offset: u32,
    pub otadata_size: u32,
}

#[repr(u32)]
#[derive(Debug, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum OtaImgState {
    EspOtaImgNew = 0x0,
    EspOtaImgPendingVerify = 0x1,
    EspOtaImgValid = 0x2,
    EspOtaImgInvalid = 0x3,
    EspOtaImgAborted = 0x4,
    EspOtaImgUndefined = 0xFFFFFFFF,
}

#[repr(C)]
#[derive(Debug)]
pub struct EspOtaSelectEntry {
    pub seq: u32,
    pub seq_label: [u8; 20],
    pub ota_state: OtaImgState,
    pub crc: u32,
}

impl EspOtaSelectEntry {
    /// Check if crc(of seq) is correct, if not - its setting seq to 0
    pub fn check_crc(&mut self) {
        if !crate::helpers::is_crc_seq_correct(self.seq, self.crc) {
            self.seq = 0; // set seq to 0 if crc not correct!
        }
    }
}
