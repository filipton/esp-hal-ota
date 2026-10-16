//! MMU access for chips whose table is read through SPI0 `MMU_ITEM_INDEX` / `MMU_ITEM_CONTENT`
//! (ESP-IDF `esp_hal_cache/<chip>/include/hal/mmu_ll.h` + `soc/<chip>/include/soc/ext_mem_defs.h`).

/// Defines the `mmu_ll_*` functions for `$pac` (flash MMU only, `mmu_id` 0).
macro_rules! indexed_mmu_ll {
    ($pac:ident, entries: $entries:expr, valid: $valid:expr, val_mask: $val_mask:expr,
     page_size: |$spi0:ident| $page_size:expr) => {
        use crate::mmu_hal::page_shift;
        #[allow(unused_imports)]
        use crate::mmu_hal::{
            MMU_PAGE_8KB, MMU_PAGE_16KB, MMU_PAGE_32KB, MMU_PAGE_64KB, MMU_PAGE_128KB,
            MMU_PAGE_256KB,
        };

        fn spi0() -> &'static $pac::spi0::RegisterBlock {
            // SAFETY: PAC register block that is always valid.
            unsafe { &*$pac::SPI0::ptr() }
        }

        pub fn mmu_ll_get_page_size(_mmu_id: u32) -> u32 {
            #[allow(unused_variables)]
            let $spi0 = spi0();
            $page_size
        }

        pub fn mmu_ll_get_entry_id(mmu_id: u32, vaddr: u32) -> u32 {
            let page_size = mmu_ll_get_page_size(mmu_id);
            let shift = page_shift(page_size).unwrap_or(16);
            (vaddr & (page_size.wrapping_mul($entries).wrapping_sub(1))) >> shift
        }

        pub fn mmu_ll_entry_id_to_paddr_base(mmu_id: u32, entry_id: u32) -> u32 {
            let shift = page_shift(mmu_ll_get_page_size(mmu_id)).unwrap_or(16);
            (mmu_entry(entry_id) & $val_mask) << shift
        }

        pub fn mmu_ll_check_entry_valid(_mmu_id: u32, entry_id: u32) -> bool {
            entry_id < $entries && (mmu_entry(entry_id) & $valid) != 0
        }

        /// Reads MMU table entry `entry_id` (index write + content read, unlocked like ESP-IDF).
        fn mmu_entry(entry_id: u32) -> u32 {
            let spi0 = spi0();
            // SAFETY: the index register accepts any value.
            spi0.mmu_item_index()
                .write(|w| unsafe { w.mmu_item_index().bits(entry_id) });
            spi0.mmu_item_content().read().bits()
        }
    };
}
pub(super) use indexed_mmu_ll;
