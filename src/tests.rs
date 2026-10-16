extern crate std;

use super::*;
use std::{vec, vec::Vec};

const OTADATA: u32 = 0xD000;
const OTA_0: u32 = 0x10000;
const OTA_1: u32 = 0x20000;
const OTA_SIZE: u32 = 0x10000;

struct MemFlash(Vec<u8>);

impl ReadStorage for MemFlash {
    type Error = ();

    fn read(&mut self, offset: u32, bytes: &mut [u8]) -> core::result::Result<(), ()> {
        let src = self
            .0
            .get(offset as usize..offset as usize + bytes.len())
            .ok_or(())?;
        bytes.copy_from_slice(src);
        Ok(())
    }

    fn capacity(&self) -> usize {
        self.0.len()
    }
}

impl Storage for MemFlash {
    fn write(&mut self, offset: u32, bytes: &[u8]) -> core::result::Result<(), ()> {
        let dst = self
            .0
            .get_mut(offset as usize..offset as usize + bytes.len())
            .ok_or(())?;
        dst.copy_from_slice(bytes);
        Ok(())
    }
}

fn part_entry(p_type: u8, subtype: u8, offset: u32, size: u32) -> [u8; 32] {
    let mut e = [0u8; 32];
    e[0..4].copy_from_slice(&[0xAA, 0x50, p_type, subtype]);
    e[4..8].copy_from_slice(&offset.to_le_bytes());
    e[8..12].copy_from_slice(&size.to_le_bytes());
    e
}

fn flash_with(entries: &[[u8; 32]]) -> MemFlash {
    let mut flash = vec![0xFF; 0x30000];
    for (i, e) in entries.iter().enumerate() {
        let at = PART_OFFSET as usize + i * 32;
        flash[at..at + 32].copy_from_slice(e);
    }
    MemFlash(flash)
}

fn default_flash() -> MemFlash {
    flash_with(&[
        part_entry(1, 2, 0x9000, 0x4000), // nvs
        part_entry(1, 0, OTADATA, 0x2000),
        part_entry(0, 0x10, OTA_0, OTA_SIZE),
        part_entry(0, 0x11, OTA_1, OTA_SIZE),
        part_entry(0, 0x20, 0x2_0000, 0x1000), // test app, not an OTA slot
    ])
}

fn entry_at(ota: &mut Ota<MemFlash>, slot: u8) -> EspOtaSelectEntry {
    ota.read_boot_entry(slot).unwrap()
}

#[test]
fn crc_matches_esp_idf() {
    assert_eq!(crc32_le(0, b"123456789"), 0xCBF43926);
    // values from ESP-IDF otatool: binascii.crc32(struct.pack('<I', seq), 0xFFFFFFFF)
    assert!(helpers::is_crc_seq_correct(1, 0x4743989A));
    assert!(helpers::is_crc_seq_correct(2, 0x55F63774));
    assert!(!helpers::is_crc_seq_correct(2, 0x4743989A));
    // erased otadata is never valid
    assert!(!helpers::is_crc_seq_correct(
        u32::MAX,
        crc32_le(u32::MAX, &[0xFF; 4])
    ));
}

#[test]
fn helpers_do_not_panic_on_bad_input() {
    assert_eq!(helpers::seq_to_part(5, 0), 0);
    assert_eq!(helpers::seq_to_part(0, 2), 0);
    assert_eq!(helpers::seq_to_part(4, 2), 1);
}

#[test]
fn entry_parsing_accepts_any_state_value() {
    let mut bytes = [0xAB; 32];
    bytes[24..28].copy_from_slice(&7u32.to_le_bytes());
    let entry = EspOtaSelectEntry::from_bytes(&bytes);
    assert_eq!(entry.ota_state, OtaImgState::Undefined);
    assert_eq!(entry.seq_label, [0xAB; 20]);

    let mut entry = entry;
    entry.ota_state = OtaImgState::Valid;
    let parsed = EspOtaSelectEntry::from_bytes(&entry.to_bytes());
    assert_eq!(parsed.ota_state, OtaImgState::Valid);
    assert_eq!(parsed.seq, entry.seq);
}

#[test]
fn partition_table_parsing() {
    let ota = Ota::new(default_flash()).unwrap();
    assert_eq!(
        ota.get_partitions(),
        &[(OTA_0, OTA_SIZE), (OTA_1, OTA_SIZE)]
    );
    assert_eq!(ota.pinfo.otadata_offset, OTADATA);

    let no_otadata = flash_with(&[
        part_entry(0, 0x10, OTA_0, OTA_SIZE),
        part_entry(0, 0x11, OTA_1, OTA_SIZE),
    ]);
    assert_eq!(
        Ota::new(no_otadata).err(),
        Some(Error::NoOtadata)
    );

    let out_of_order = flash_with(&[
        part_entry(1, 0, OTADATA, 0x2000),
        part_entry(0, 0x11, OTA_1, OTA_SIZE),
        part_entry(0, 0x10, OTA_0, OTA_SIZE),
    ]);
    assert_eq!(
        Ota::new(out_of_order).err(),
        Some(Error::PartitionOrder)
    );
}

#[test]
fn full_update_writes_otadata() {
    let image: Vec<u8> = (0..5000u32).map(|i| i as u8).collect();
    let crc = crc32_le(0, &image);

    let mut ota = Ota::new(default_flash()).unwrap();
    assert_eq!(ota.write(&image), Err(Error::NotStarted));
    assert_eq!(
        ota.begin(OTA_SIZE + 1, crc),
        Err(Error::PartitionTooSmall)
    );

    ota.begin(image.len() as u32, crc).unwrap();
    assert_eq!(ota.write(&image[..1000]), Ok(false));
    assert_eq!(ota.finish(true), Err(Error::Incomplete));

    // last chunk with trailing garbage, which is ignored
    let mut rest = image[1000..].to_vec();
    rest.extend_from_slice(&[0x55; 100]);
    assert_eq!(ota.write(&rest), Ok(true));
    let progress = ota.progress().unwrap();
    assert_eq!((progress.percent(), progress.permille()), (100, 1000));
    assert_eq!(ota.verify(), Ok(true));
    ota.finish(true).unwrap();

    assert_eq!(&ota.flash.0[OTA_0 as usize..][..image.len()], &image[..]);
    // fresh otadata: entry 2 gets seq 1 (= ota_0)
    let entry = entry_at(&mut ota, 2);
    assert_eq!(entry.seq, 1);
    assert_eq!(entry.ota_state, OtaImgState::New);
    assert_eq!(entry_at(&mut ota, 1).seq, 0);
}

#[test]
fn wrong_crc_does_not_touch_otadata() {
    let mut ota = Ota::new(default_flash()).unwrap();
    ota.begin(4, 1234).unwrap();
    assert_eq!(ota.write(&[1, 2, 3, 4]), Ok(true));
    assert_eq!(ota.finish(false), Err(Error::Crc));
    assert!(
        ota.flash.0[OTADATA as usize..][..0x2000]
            .iter()
            .all(|&b| b == 0xFF)
    );
}

#[test]
fn boot_seq_follows_esp_idf() {
    let mut ota = Ota::new(default_flash()).unwrap();

    ota.set_boot_partition(0, OtaImgState::New)
        .unwrap();
    assert_eq!(
        (entry_at(&mut ota, 1).seq, entry_at(&mut ota, 2).seq),
        (0, 1)
    );

    ota.set_boot_partition(1, OtaImgState::New)
        .unwrap();
    assert_eq!(
        (entry_at(&mut ota, 1).seq, entry_at(&mut ota, 2).seq),
        (2, 1)
    );

    ota.set_boot_partition(0, OtaImgState::New)
        .unwrap();
    assert_eq!(
        (entry_at(&mut ota, 1).seq, entry_at(&mut ota, 2).seq),
        (2, 3)
    );

    // same partition again keeps the seq, written to the inactive entry
    ota.set_boot_partition(0, OtaImgState::New)
        .unwrap();
    assert_eq!(
        (entry_at(&mut ota, 1).seq, entry_at(&mut ota, 2).seq),
        (3, 3)
    );

    assert_eq!(
        ota.set_boot_partition(2, OtaImgState::New),
        Err(Error::NotEnoughPartitions)
    );
}

#[test]
fn resume_validates_input() {
    let image = [7u8; 300];
    let crc = crc32_le(0, &image);

    let mut ota = Ota::new(default_flash()).unwrap();
    assert_eq!(
        ota.resume(
            &ResumeInfo {
                size: 10,
                remaining: 20,
                target_crc: 0,
                last_crc: 0
            },
            false
        ),
        Err(Error::InvalidResumeInfo)
    );

    ota.begin(300, crc).unwrap();
    ota.write(&image[..100]).unwrap();
    let info = ota.resume_info().unwrap();
    assert_eq!(ota.progress().unwrap().percent(), 33);

    let mut ota = Ota::new(MemFlash(ota.flash.0)).unwrap();
    assert_eq!(
        ota.resume(
            &ResumeInfo {
                last_crc: info.last_crc ^ 1,
                ..info
            },
            true
        ),
        Err(Error::Crc)
    );
    ota.resume(&info, true).unwrap();
    assert_eq!(ota.write(&image[100..]), Ok(true));
    ota.finish(false).unwrap();
}
