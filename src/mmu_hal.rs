pub const MMU_PAGE_8KB: u32 = 0x2000;
pub const MMU_PAGE_16KB: u32 = 0x4000;
pub const MMU_PAGE_32KB: u32 = 0x8000;
pub const MMU_PAGE_64KB: u32 = 0x10000;

fn mmu_hal_pages_to_bytes(mmu_id: u32, page_num: u32) -> Option<u32> {
    let shift_code = match crate::mmu_ll::mmu_ll_get_page_size(mmu_id) {
        MMU_PAGE_64KB => 16,
        MMU_PAGE_32KB => 15,
        MMU_PAGE_16KB => 14,
        MMU_PAGE_8KB => 13,
        _page_size => {
            error!("Wrong MMU page size: 0x{:x}", _page_size);
            return None;
        }
    };

    Some(page_num << shift_code)
}

/// Returns the index (in `partitions`) of the partition the running code is mapped from.
pub fn esp_get_current_running_partition(partitions: &[(u32, u32)]) -> Option<usize> {
    // NOTE:
    // mmu_id is always 0 because s_vaddr_to_paddr is using 0 for all targets
    // except esp32p4 (per SOC_MMU_PER_EXT_MEM_TARGET define)
    //
    // https://github.com/espressif/esp-idf/blob/b5ac4fbdf9e9fb320bb0a98ee4fbaa18f8566f37/components/esp_mm/esp_mmu_map.c#L754
    let mmu_id = 0;

    // black_box: xtensa LLVM (esp toolchain 1.96) crashes folding this address at opt-level s/z
    let vaddr = core::hint::black_box(esp_get_current_running_partition as *const ()) as u32;
    let entry_id = crate::mmu_ll::mmu_ll_get_entry_id(mmu_id, vaddr);

    if !crate::mmu_ll::mmu_ll_check_entry_valid(mmu_id, entry_id) {
        error!("mmu_ll_check_entry_valid failed!");

        return None;
    }

    // page_num is always 1
    // https://github.com/espressif/esp-idf/blob/master/components/hal/mmu_hal.c#L129
    let offset = vaddr % mmu_hal_pages_to_bytes(mmu_id, 1)?;
    let paddr = crate::mmu_ll::mmu_ll_entry_id_to_paddr_base(mmu_id, entry_id) | offset;

    partitions
        .iter()
        .position(|&(start, size)| paddr >= start && paddr - start < size)
}
