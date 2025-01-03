#![no_std]
#![cfg_attr(any(feature = "esp32", feature = "esp32s2"), feature(concat_idents))]
#![cfg_attr(feature = "esp32", feature(asm_experimental_arch))]
#![doc = include_str!("../README.md")]

use embedded_storage::{ReadStorage, Storage};
pub use structs::*;

pub mod crc32;
pub mod helpers;
pub mod mmu_hal;
pub mod mmu_ll;
pub mod structs;

const PART_OFFSET: u32 = 0x8000;
const PART_SIZE: u32 = 0xc00;
const FIRST_OTA_PART_SUBTYPE: u8 = 0x10;
const OTA_VERIFY_READ_SIZE: usize = 256;

// NOTE: I need to use generics, because after adding esp-storage dependency to
// this project its not compiling LULE
pub struct Ota<S>
where
    S: ReadStorage + Storage,
{
    flash: S,

    progress: Option<FlashProgress>,
    pinfo: PartitionInfo,
}

impl<S> Ota<S>
where
    S: ReadStorage + Storage,
{
    pub fn new(mut flash: S) -> Result<Self> {
        let pinfo = Self::read_partitions(&mut flash)?;
        if pinfo.ota_partitions_count < 2 {
            #[cfg(feature = "log")]
            log::error!("Not enough OTA partitions! (>= 2)");

            return Err(OtaError::NotEnoughPartitions);
        }

        Ok(Ota {
            flash,
            progress: None,
            pinfo,
        })
    }

    fn get_partitions(&self) -> &[(u32, u32)] {
        &self.pinfo.ota_partitions[..self.pinfo.ota_partitions_count]
    }

    /// To begin ota update (need to provide flash size)
    pub fn ota_begin(&mut self, size: u32, target_crc: u32) -> Result<()> {
        let next_part = self
            .get_next_ota_partition()
            .ok_or(OtaError::NoNextOtaPartition)?;

        let ota_offset = self.get_partitions()[next_part].0;
        self.progress = Some(FlashProgress {
            last_crc: 0,
            flash_size: size,
            remaining: size,
            flash_offset: ota_offset,
            target_partition: next_part,
            target_crc,
        });

        Ok(())
    }

    /// Returns ota progress in f32 (0..1)
    pub fn get_ota_progress(&self) -> f32 {
        if self.progress.is_none() {
            #[cfg(feature = "log")]
            log::warn!("[OTA] Cannot get ota progress! Seems like update wasn't started yet.");

            return 0.0;
        }

        let progress = self.progress.as_ref().unwrap();
        (progress.flash_size - progress.remaining) as f32 / progress.flash_size as f32
    }

    /// Writes next firmware chunk
    pub fn ota_write_chunk(&mut self, chunk: &[u8]) -> Result<bool> {
        let progress = self
            .progress
            .as_mut()
            .ok_or_else(|| OtaError::OtaNotStarted)?;

        if progress.remaining == 0 {
            return Ok(true);
        }

        let write_size = chunk.len() as u32;
        let write_size = write_size.min(progress.remaining) as usize;

        self.flash
            .write(progress.flash_offset, &chunk[..write_size])
            .map_err(|_| OtaError::FlashRWError)?;

        #[cfg(feature = "log")]
        log::debug!(
            "[OTA] Wrote {} bytes to ota partition at 0x{:x}",
            write_size,
            progress.flash_offset
        );

        progress.last_crc = crc32::calc_crc32(&chunk[..write_size], progress.last_crc);

        progress.flash_offset += write_size as u32;
        progress.remaining -= write_size as u32;
        Ok(progress.remaining == 0)
    }

    /// verify - should it read flash and check crc
    pub fn ota_flush(&mut self, verify: bool) -> Result<()> {
        if verify {
            if !self.ota_verify()? {
                #[cfg(feature = "log")]
                log::error!("[OTA] Verify failed! Not flushing...");

                return Err(OtaError::OtaVerifyError);
            }
        }

        let progress = self
            .progress
            .clone()
            .ok_or_else(|| OtaError::OtaNotStarted)?;

        if progress.target_crc != progress.last_crc {
            #[cfg(feature = "log")]
            {
                log::warn!("[OTA] Calculated crc: {:?}", progress.last_crc);
                log::warn!("[OTA] Target crc: {:?}", progress.target_crc);
                log::error!("[OTA] Crc check failed! Cant finish ota update...");
            }

            return Err(OtaError::WrongCRC);
        }

        self.set_target_ota_boot_partition(progress.target_partition);
        Ok(())
    }

    /// It reads written flash and checks crc
    pub fn ota_verify(&mut self) -> Result<bool> {
        let progress = self
            .progress
            .clone()
            .ok_or_else(|| OtaError::OtaNotStarted)?;

        let mut calc_crc = 0;
        let mut bytes = [0; OTA_VERIFY_READ_SIZE];

        let mut partition_offset = self.pinfo.ota_partitions[progress.target_partition].0;
        let mut remaining = progress.flash_size;

        loop {
            let n = remaining.min(OTA_VERIFY_READ_SIZE as u32);
            if n == 0 {
                break;
            }

            _ = self.flash.read(partition_offset, &mut bytes[..n as usize]);
            partition_offset += n;
            remaining -= n;

            calc_crc = crc32::calc_crc32(&bytes[..n as usize], calc_crc);
        }

        Ok(calc_crc == progress.target_crc)
    }

    /// Sets ota boot target partition
    pub fn set_target_ota_boot_partition(&mut self, target: usize) {
        let (slot1, slot2) = self.get_ota_boot_entries();
        let (seq1, seq2) = (slot1.seq, slot2.seq);

        let mut target_seq = seq1.max(seq2);
        while helpers::seq_to_part(target_seq, self.pinfo.ota_partitions_count) != target
            || target_seq == 0
        {
            target_seq += 1;
        }

        let flash = &mut self.flash;
        let target_crc = crc32::calc_crc32(&target_seq.to_le_bytes(), 0xFFFFFFFF);
        if seq1 > seq2 {
            let offset = self.pinfo.otadata_offset + (self.pinfo.otadata_size >> 1);

            _ = flash.write(offset, &target_seq.to_le_bytes());
            _ = flash.write(offset + 32 - 4, &target_crc.to_le_bytes());
        } else {
            _ = flash.write(self.pinfo.otadata_offset, &target_seq.to_le_bytes());
            _ = flash.write(
                self.pinfo.otadata_offset + 32 - 4,
                &target_crc.to_le_bytes(),
            );
        }
    }

    /// Returns current OTA boot sequences
    ///
    /// NOTE: if crc doesn't match, it returns 0 for that seq
    /// NOTE: [Entry struct (link to .h file)](https://github.com/espressif/esp-idf/blob/master/components/bootloader_support/include/esp_flash_partitions.h#L66)
    pub fn get_ota_boot_entries(&mut self) -> (EspOtaSelectEntry, EspOtaSelectEntry) {
        let mut bytes = [0; 32];
        _ = self.flash.read(self.pinfo.otadata_offset, &mut bytes);
        let mut slot1: EspOtaSelectEntry =
            unsafe { core::ptr::read(bytes.as_ptr() as *const EspOtaSelectEntry) };
        slot1.check_crc();

        _ = self.flash.read(
            self.pinfo.otadata_offset + (self.pinfo.otadata_size >> 1),
            &mut bytes,
        );
        let mut slot2: EspOtaSelectEntry =
            unsafe { core::ptr::read(bytes.as_ptr() as *const EspOtaSelectEntry) };
        slot2.check_crc();

        (slot1, slot2)
    }

    /// Returns currently booted partition index
    pub fn get_currently_booted_partition(&self) -> Option<usize> {
        mmu_hal::esp_get_current_running_partition(self.get_partitions())
    }

    /// BUG: this wont work if user has ota partitions not starting from ota0
    /// or if user skips some ota partitions: ota0, ota2, ota3...
    ///
    /// NOTE: This isn't reading from ota_boot_sequences, maybe in the future
    /// it will read from them to eliminate possibility of wrong PADDR result.
    /// (ESP-IDF has if's for PADDR-chain so it can fail somehow)
    pub fn get_next_ota_partition(&self) -> Option<usize> {
        let curr_part = mmu_hal::esp_get_current_running_partition(self.get_partitions());
        curr_part.map(|next_part| (next_part + 1) % self.pinfo.ota_partitions_count)
    }

    fn read_partitions(flash: &mut S) -> Result<PartitionInfo> {
        let mut tmp_pinfo = PartitionInfo {
            ota_partitions: [(0, 0); 16],
            ota_partitions_count: 0,
            otadata_size: 0,
            otadata_offset: 0,
        };

        let mut bytes = [0xFF; 32];
        let mut last_ota_part: i8 = -1;
        for read_offset in (0..PART_SIZE).step_by(32) {
            _ = flash.read(PART_OFFSET + read_offset, &mut bytes);
            if &bytes == &[0xFF; 32] {
                break;
            }

            let magic = &bytes[0..2];
            if magic != &[0xAA, 0x50] {
                continue;
            }

            let p_type = &bytes[2];
            let p_subtype = &bytes[3];
            let p_offset = u32::from_le_bytes(bytes[4..8].try_into().unwrap());
            let p_size = u32::from_le_bytes(bytes[8..12].try_into().unwrap());
            //let p_name = core::str::from_utf8(&bytes[12..28]).unwrap();
            //let p_flags = u32::from_le_bytes(bytes[28..32].try_into().unwrap());
            //log::info!("{magic:?} {p_type} {p_subtype} {p_offset} {p_size} {p_name} {p_flags}");

            if *p_type == 0 && *p_subtype >= FIRST_OTA_PART_SUBTYPE {
                let ota_part_idx = *p_subtype - FIRST_OTA_PART_SUBTYPE;
                if ota_part_idx as i8 - last_ota_part != 1 {
                    return Err(OtaError::WrongOTAPArtitionOrder);
                }

                last_ota_part = ota_part_idx as i8;
                tmp_pinfo.ota_partitions[tmp_pinfo.ota_partitions_count] = (p_offset, p_size);
                tmp_pinfo.ota_partitions_count += 1;
            } else if *p_type == 1 && *p_subtype == 0 {
                //otadata
                tmp_pinfo.otadata_offset = p_offset;
                tmp_pinfo.otadata_size = p_size;
            }
        }

        Ok(tmp_pinfo)
    }
}
