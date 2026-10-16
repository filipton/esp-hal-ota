const SOC_MMU_VADDR_MASK: u32 = 0x1FFFFFF;
const DR_REG_MMU_TABLE: u32 = 0x600C5000;
const SOC_MMU_VALID_VAL_MASK: u32 = 0x3fff;
const SOC_MMU_INVALID: u32 = 1 << 14;

pub fn mmu_ll_get_page_size(_mmu_id: u32) -> u32 {
    crate::mmu_hal::MMU_PAGE_64KB
}

pub fn mmu_ll_get_entry_id(_mmu_id: u32, vaddr: u32) -> u32 {
    (vaddr & SOC_MMU_VADDR_MASK) >> 16
}

pub fn mmu_ll_entry_id_to_paddr_base(_mmu_id: u32, entry_id: u32) -> u32 {
    (mmu_entry(entry_id) & SOC_MMU_VALID_VAL_MASK) << 16
}

pub fn mmu_ll_check_entry_valid(_mmu_id: u32, entry_id: u32) -> bool {
    (mmu_entry(entry_id) & SOC_MMU_INVALID) == 0
}

/// Reads MMU table entry `entry_id`.
fn mmu_entry(entry_id: u32) -> u32 {
    // SAFETY: `DR_REG_MMU_TABLE` is the always-mapped MMU table and `entry_id` comes from a
    // masked virtual address, so it stays inside the table. Volatile, as it is hardware state.
    unsafe { ((DR_REG_MMU_TABLE + entry_id * 4) as *const u32).read_volatile() }
}
