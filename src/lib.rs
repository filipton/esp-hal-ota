#![no_std]
#![cfg_attr(feature = "esp32", feature(asm_experimental_arch))]
#![doc = include_str!("../README.md")]

#[macro_use]
mod logging;

use embedded_storage::{ReadStorage, Storage};
#[cfg(feature = "esp-rom-sys")]
pub(crate) use esp_rom_sys::rom::crc::crc32_le;
pub use structs::*;

pub mod helpers;
pub mod mmu_hal;
pub mod mmu_ll;
pub mod structs;

/// Software version of the ROM `crc32_le` (zlib convention: `crc` is the previous result).
#[cfg(not(feature = "esp-rom-sys"))]
pub(crate) fn crc32_le(crc: u32, buf: &[u8]) -> u32 {
    let mut crc = !crc;
    for &byte in buf {
        crc ^= byte as u32;
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xEDB88320 & (crc & 1).wrapping_neg());
        }
    }
    !crc
}

const PART_OFFSET: u32 = 0x8000;
const PART_SIZE: u32 = 0xc00;
const PART_ENTRY_SIZE: usize = 32;
const FIRST_OTA_PART_SUBTYPE: u8 = 0x10;
const MAX_OTA_PARTITIONS: usize = 16;
/// ESP-IDF stores the second `otadata` entry one flash sector after the first one.
const OTADATA_SLOT_SIZE: u32 = 0x1000;
const OTA_VERIFY_READ_SIZE: usize = 256;

/// OTA updater over a flash [`Storage`] (e.g. `esp_storage::FlashStorage`).
pub struct Ota<S>
where
    S: ReadStorage + Storage,
{
    flash: S,

    progress: Option<FlashProgress>,
    pinfo: PartitionInfo,
}

/// Progress of an update, to persist and pass to [`Ota::ota_resume`] later.
pub struct ProgressDetails {
    pub remaining: u32,
    pub last_crc: u32,
}

/// Configuration of the [`Ota`] process.
#[derive(Debug)]
pub struct OtaConfiguration {
    partition_table_offset: u32,
}

#[deprecated(note = "renamed to `OtaConfiguration`")]
pub type OtaConfiguratuion = OtaConfiguration;

impl OtaConfiguration {
    /// Create default configuration of the [`Ota`] process.
    ///
    /// Notably this uses the default partition table offset of `0x8000`
    pub const fn new() -> Self {
        Self {
            partition_table_offset: PART_OFFSET,
        }
    }

    /// Set custom partition table offset
    pub const fn with_partition_table_offset(self, partition_table_offset: u32) -> Self {
        Self {
            partition_table_offset,
        }
    }
}

impl Default for OtaConfiguration {
    fn default() -> Self {
        Self::new()
    }
}

impl<S> Ota<S>
where
    S: ReadStorage + Storage,
{
    /// Reads the partition table at the default offset (`0x8000`).
    pub fn new(flash: S) -> Result<Self> {
        Self::with_configuration(flash, OtaConfiguration::new())
    }

    /// Reads the partition table with a custom [`OtaConfiguration`].
    pub fn with_configuration(mut flash: S, config: OtaConfiguration) -> Result<Self> {
        let pinfo = Self::read_partitions(&mut flash, config.partition_table_offset)?;
        if pinfo.ota_partitions_count < 2 {
            error!("Not enough OTA partitions! (>= 2)");
            return Err(OtaError::NotEnoughPartitions);
        }

        if pinfo.otadata_size < 2 * OTADATA_SLOT_SIZE {
            error!("No valid otadata partition! (size >= 0x2000)");
            return Err(OtaError::NotEnoughPartitions);
        }

        Ok(Ota {
            flash,
            progress: None,
            pinfo,
        })
    }

    fn get_partitions(&self) -> &[(u32, u32)] {
        self.pinfo
            .ota_partitions
            .get(..self.pinfo.ota_partitions_count)
            .unwrap_or_default()
    }

    /// Partition the update is written to, with its `(offset, size)`.
    ///
    /// The one after the running one, or `ota_0` when not running from an OTA partition
    /// (factory/test app), like `esp_ota_get_next_update_partition`.
    fn next_update_partition(&self) -> Result<(usize, (u32, u32))> {
        let next_part = self.get_next_ota_partition().unwrap_or(0);
        let part = self
            .get_partitions()
            .get(next_part)
            .copied()
            .ok_or(OtaError::NotEnoughPartitions)?;

        Ok((next_part, part))
    }

    /// Starts an update of an image of `size` bytes with crc32 `target_crc`.
    pub fn ota_begin(&mut self, size: u32, target_crc: u32) -> Result<()> {
        let (next_part, (offset, part_size)) = self.next_update_partition()?;
        if part_size < size {
            return Err(OtaError::OtaPartitionTooSmall);
        }

        self.progress = Some(FlashProgress {
            last_crc: 0,
            flash_size: size,
            remaining: size,
            flash_offset: offset,
            target_partition: next_part,
            target_crc,
        });

        Ok(())
    }

    /// Resumes an OTA update after progress has been lost
    ///
    /// `remaining` and `last_crc` come from [`Ota::get_progress_details`]. With `verify_crc` the
    /// already written part is read back and must match `last_crc`.
    pub fn ota_resume(
        &mut self,
        flash_size: u32,
        remaining: u32,
        target_crc: u32,
        last_crc: u32,
        verify_crc: bool,
    ) -> Result<()> {
        let (next_part, (offset, part_size)) = self.next_update_partition()?;
        if part_size < flash_size {
            return Err(OtaError::OtaPartitionTooSmall);
        }

        let written = flash_size
            .checked_sub(remaining)
            .ok_or(OtaError::OtaPartitionTooSmall)?;
        if verify_crc && self.flash_crc(offset, written)? != last_crc {
            return Err(OtaError::WrongCRC);
        }

        self.progress = Some(FlashProgress {
            last_crc,
            flash_size,
            remaining,
            flash_offset: offset + written,
            target_partition: next_part,
            target_crc,
        });

        Ok(())
    }

    /// Returns progress details to save for resumption later
    pub fn get_progress_details(&self) -> Option<ProgressDetails> {
        let details = self.progress.as_ref().map(|progress| ProgressDetails {
            remaining: progress.remaining,
            last_crc: progress.last_crc,
        });
        if details.is_none() {
            warn!("[OTA] Cannot get progress details!");
        }

        details
    }

    /// Returns ota progress in f32 (0..1)
    pub fn get_ota_progress(&self) -> f32 {
        let Some(progress) = self.progress.as_ref() else {
            warn!("[OTA] Cannot get ota progress! Seems like update wasn't started yet.");
            return 0.0;
        };

        if progress.flash_size == 0 {
            return 1.0;
        }

        (progress.flash_size - progress.remaining) as f32 / progress.flash_size as f32
    }

    /// Writes next firmware chunk, returns `true` when the whole image is written.
    ///
    /// Bytes past the announced image size are ignored.
    pub fn ota_write_chunk(&mut self, chunk: &[u8]) -> Result<bool> {
        let progress = self.progress.as_mut().ok_or(OtaError::OtaNotStarted)?;

        let write_size = chunk.len().min(progress.remaining as usize);
        let chunk = &chunk[..write_size];
        if chunk.is_empty() {
            return Ok(progress.remaining == 0);
        }

        self.flash
            .write(progress.flash_offset, chunk)
            .map_err(|_| OtaError::FlashRWError)?;

        debug!(
            "[OTA] Wrote {} bytes to ota partition at 0x{:x}",
            write_size, progress.flash_offset
        );

        progress.last_crc = crc32_le(progress.last_crc, chunk);
        progress.flash_offset += write_size as u32;
        progress.remaining -= write_size as u32;
        Ok(progress.remaining == 0)
    }

    /// Finishes the update and sets the new image as the boot partition.
    ///
    /// verify - should it read flash and check crc
    /// rollback - if rollbacks enable (will set ota_state to ESP_OTA_IMG_NEW)
    pub fn ota_flush(&mut self, verify: bool, rollback: bool) -> Result<()> {
        if verify && !self.ota_verify()? {
            error!("[OTA] Verify failed! Not flushing...");
            return Err(OtaError::OtaVerifyError);
        }

        let progress = self.progress.as_ref().ok_or(OtaError::OtaNotStarted)?;
        if progress.remaining != 0 || progress.target_crc != progress.last_crc {
            warn!("[OTA] Calculated crc: {}", progress.last_crc);
            warn!("[OTA] Target crc: {}", progress.target_crc);
            error!("[OTA] Crc check failed! Cant finish ota update...");
            return Err(OtaError::WrongCRC);
        }

        let img_state = match rollback {
            true => OtaImgState::EspOtaImgNew,
            false => OtaImgState::EspOtaImgUndefined,
        };

        let target = progress.target_partition;
        self.write_boot_partition(target, img_state)
    }

    /// It reads written flash and checks crc
    pub fn ota_verify(&mut self) -> Result<bool> {
        let progress = self.progress.as_ref().ok_or(OtaError::OtaNotStarted)?;
        let (target_crc, size) = (progress.target_crc, progress.flash_size);
        let (offset, _) = self
            .get_partitions()
            .get(progress.target_partition)
            .copied()
            .ok_or(OtaError::NotEnoughPartitions)?;

        Ok(self.flash_crc(offset, size)? == target_crc)
    }

    /// crc32 of `len` bytes of flash starting at `offset`.
    fn flash_crc(&mut self, mut offset: u32, mut len: u32) -> Result<u32> {
        let mut crc = 0;
        let mut bytes = [0; OTA_VERIFY_READ_SIZE];
        while len > 0 {
            let n = len.min(OTA_VERIFY_READ_SIZE as u32);
            let buf = &mut bytes[..n as usize];
            self.flash
                .read(offset, buf)
                .map_err(|_| OtaError::FlashRWError)?;

            crc = crc32_le(crc, buf);
            offset += n;
            len -= n;
        }

        Ok(crc)
    }

    /// Sets ota boot target partition (errors are only logged, see [`Ota::ota_flush`])
    pub fn set_target_ota_boot_partition(&mut self, target: usize, state: OtaImgState) {
        if let Err(_e) = self.write_boot_partition(target, state) {
            error!("[OTA] Cannot set boot partition: {:?}", _e);
        }
    }

    /// Writes a new `otadata` entry that boots partition `target` (like ESP-IDF's
    /// `esp_rewrite_ota_data`): smallest seq >= the active one that maps to `target`, written
    /// over the inactive entry in a single write.
    fn write_boot_partition(&mut self, target: usize, state: OtaImgState) -> Result<()> {
        let count = self.pinfo.ota_partitions_count;
        if target >= count {
            return Err(OtaError::NotEnoughPartitions);
        }

        let (slot1, slot2) = self.read_boot_entries()?;
        let (active_seq, mut entry, offset) = if slot1.seq >= slot2.seq {
            (slot1.seq, slot2, self.otadata_slot_offset(2))
        } else {
            (slot2.seq, slot1, self.otadata_slot_offset(1))
        };

        // seq 1 boots ota_0, seq 2 ota_1, ...
        let base = (target as u32) + 1;
        let count = count as u32;
        let seq = match active_seq
            .saturating_sub(base)
            .div_ceil(count)
            .checked_mul(count)
        {
            Some(steps) => steps.checked_add(base),
            None => None,
        }
        .filter(|&seq| seq != u32::MAX)
        .ok_or(OtaError::WrongCRC)?;

        entry.seq = seq;
        entry.ota_state = state;
        entry.crc = crc32_le(u32::MAX, &seq.to_le_bytes());
        self.flash
            .write(offset, &entry.to_bytes())
            .map_err(|_| OtaError::FlashRWError)
    }

    /// Sets the `ota_state` of `otadata` entry 1 or 2.
    pub fn set_ota_state(&mut self, slot: u8, state: OtaImgState) -> Result<()> {
        if slot != 1 && slot != 2 {
            error!("Use slot1 or slot2!");
            return Err(OtaError::CannotFindCurrentBootPartition);
        }

        // ota_state sits right before the crc at the end of the 32-byte entry
        self.flash
            .write(
                self.otadata_slot_offset(slot) + 24,
                &(state as u32).to_le_bytes(),
            )
            .map_err(|_| OtaError::FlashRWError)
    }

    fn otadata_slot_offset(&self, slot: u8) -> u32 {
        match slot {
            1 => self.pinfo.otadata_offset,
            _ => self.pinfo.otadata_offset + OTADATA_SLOT_SIZE,
        }
    }

    /// Returns current OTA boot sequences
    ///
    /// NOTE: if crc doesn't match (or the flash cannot be read), it returns 0 for that seq
    /// NOTE: [Entry struct (link to .h file)](https://github.com/espressif/esp-idf/blob/master/components/bootloader_support/include/esp_flash_partitions.h#L66)
    pub fn get_ota_boot_entries(&mut self) -> (EspOtaSelectEntry, EspOtaSelectEntry) {
        self.read_boot_entries().unwrap_or_else(|_| {
            let mut invalid = EspOtaSelectEntry::from_bytes(&[0xFF; EspOtaSelectEntry::SIZE]);
            invalid.check_crc();
            (invalid.clone(), invalid)
        })
    }

    fn read_boot_entries(&mut self) -> Result<(EspOtaSelectEntry, EspOtaSelectEntry)> {
        Ok((self.read_boot_entry(1)?, self.read_boot_entry(2)?))
    }

    fn read_boot_entry(&mut self, slot: u8) -> Result<EspOtaSelectEntry> {
        let mut bytes = [0; EspOtaSelectEntry::SIZE];
        self.flash
            .read(self.otadata_slot_offset(slot), &mut bytes)
            .map_err(|_| OtaError::FlashRWError)?;

        let mut entry = EspOtaSelectEntry::from_bytes(&bytes);
        entry.check_crc();
        Ok(entry)
    }

    /// Returns currently booted partition index
    pub fn get_currently_booted_partition(&self) -> Option<usize> {
        mmu_hal::esp_get_current_running_partition(self.get_partitions())
    }

    /// Index of the OTA partition after the running one (`None` if not running from an OTA
    /// partition).
    pub fn get_next_ota_partition(&self) -> Option<usize> {
        let curr_part = self.get_currently_booted_partition()?;
        Some((curr_part + 1) % self.pinfo.ota_partitions_count)
    }

    /// Returns the `otadata` slot (1 or 2) and entry that booted the running partition.
    ///
    /// If both entries point at it, the one with the higher seq is the active one.
    fn get_current_slot(&mut self) -> Result<(u8, EspOtaSelectEntry)> {
        let (slot1, slot2) = self.read_boot_entries()?;
        let current_partition = self
            .get_currently_booted_partition()
            .ok_or(OtaError::CannotFindCurrentBootPartition)?;

        let count = self.pinfo.ota_partitions_count;
        let boots_current = |e: &EspOtaSelectEntry| {
            e.seq != 0 && helpers::seq_to_part(e.seq, count) == current_partition
        };
        match (boots_current(&slot1), boots_current(&slot2)) {
            (true, true) if slot2.seq > slot1.seq => Ok((2, slot2)),
            (true, _) => Ok((1, slot1)),
            (false, true) => Ok((2, slot2)),
            (false, false) => Err(OtaError::CannotFindCurrentBootPartition),
        }
    }

    /// Returns the `ota_state` of the running image.
    pub fn get_ota_image_state(&mut self) -> Result<OtaImgState> {
        Ok(self.get_current_slot()?.1.ota_state)
    }

    /// Marks the running image as valid (cancels the rollback).
    pub fn ota_mark_app_valid(&mut self) -> Result<()> {
        let (current_slot_nmb, current_slot) = self.get_current_slot()?;
        if current_slot.ota_state != OtaImgState::EspOtaImgValid {
            self.set_ota_state(current_slot_nmb, OtaImgState::EspOtaImgValid)?;

            info!("Marked current slot as valid!");
        }

        Ok(())
    }

    /// Marks the running image as invalid, so the bootloader rolls back on the next reset.
    ///
    /// Does nothing if the running image is already marked valid. Does not reset the chip.
    pub fn ota_mark_app_invalid_rollback(&mut self) -> Result<()> {
        let (current_slot_nmb, current_slot) = self.get_current_slot()?;
        if current_slot.ota_state != OtaImgState::EspOtaImgValid {
            self.set_ota_state(current_slot_nmb, OtaImgState::EspOtaImgInvalid)?;

            info!("Marked current slot as invalid!");
        }

        Ok(())
    }

    fn read_partitions(flash: &mut S, partition_table_offset: u32) -> Result<PartitionInfo> {
        let mut pinfo = PartitionInfo {
            ota_partitions: [(0, 0); MAX_OTA_PARTITIONS],
            ota_partitions_count: 0,
            otadata_size: 0,
            otadata_offset: 0,
        };

        let mut bytes = [0xFF; PART_ENTRY_SIZE];
        for read_offset in (0..PART_SIZE).step_by(PART_ENTRY_SIZE) {
            flash
                .read(partition_table_offset + read_offset, &mut bytes)
                .map_err(|_| OtaError::FlashRWError)?;
            if bytes == [0xFF; PART_ENTRY_SIZE] {
                break;
            }

            let [
                0xAA,
                0x50,
                p_type,
                p_subtype,
                o0,
                o1,
                o2,
                o3,
                s0,
                s1,
                s2,
                s3,
                ..,
            ] = bytes
            else {
                continue; // not a partition entry (e.g. the md5 entry)
            };
            let p_offset = u32::from_le_bytes([o0, o1, o2, o3]);
            let p_size = u32::from_le_bytes([s0, s1, s2, s3]);

            match (p_type, p_subtype) {
                // app ota_0..ota_15 (0x20 is the test app, not an OTA slot)
                (0, FIRST_OTA_PART_SUBTYPE..=0x1F) => {
                    let ota_part_idx = (p_subtype - FIRST_OTA_PART_SUBTYPE) as usize;
                    if ota_part_idx != pinfo.ota_partitions_count {
                        return Err(OtaError::WrongOTAPArtitionOrder);
                    }

                    pinfo.ota_partitions[ota_part_idx] = (p_offset, p_size);
                    pinfo.ota_partitions_count += 1;
                }
                // data otadata
                (1, 0) => {
                    pinfo.otadata_offset = p_offset;
                    pinfo.otadata_size = p_size;
                }
                _ => {}
            }
        }

        Ok(pinfo)
    }
}

#[cfg(test)]
mod tests;
