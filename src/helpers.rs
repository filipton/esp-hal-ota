/// Helper function!
/// Change seq to partition number (returns 0 if `partitions_count` is 0)
#[inline(always)]
pub const fn seq_to_part(seq: u32, partitions_count: usize) -> usize {
    match ((seq as usize).saturating_sub(1)).checked_rem(partitions_count) {
        Some(part) => part,
        None => 0,
    }
}

/// Helper function!
/// If `seq` is 4 bytes and its crc is correct it returns seq, otherwise default value is returned
#[inline(always)]
pub fn seq_or_default(seq: &[u8], crc: u32, default: u32) -> u32 {
    match <[u8; 4]>::try_from(seq) {
        Ok(bytes) if is_crc_seq_correct(u32::from_le_bytes(bytes), crc) => {
            u32::from_le_bytes(bytes)
        }
        _ => default,
    }
}

/// Helper function!
/// Check if crc is correct for given seq (erased `0xFFFFFFFF` seq is never valid, like in ESP-IDF)
#[inline(always)]
pub fn is_crc_seq_correct(seq: u32, crc: u32) -> bool {
    seq != u32::MAX && crc == crate::crc32_le(u32::MAX, &seq.to_le_bytes())
}
