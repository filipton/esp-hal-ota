/// Helper function!
/// Change seq to partition number (returns 0 if `partitions_count` is 0)
#[inline(always)]
pub(crate) const fn seq_to_part(seq: u32, partitions_count: usize) -> usize {
    match ((seq as usize).saturating_sub(1)).checked_rem(partitions_count) {
        Some(part) => part,
        None => 0,
    }
}

/// Helper function!
/// Check if crc is correct for given seq (erased `0xFFFFFFFF` seq is never valid, like in ESP-IDF)
#[inline(always)]
pub(crate) fn is_crc_seq_correct(seq: u32, crc: u32) -> bool {
    seq != u32::MAX && crc == crate::crc32_le(u32::MAX, &seq.to_le_bytes())
}
