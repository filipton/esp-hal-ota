use crate::mmu_hal::{MMU_PAGE_16KB, MMU_PAGE_32KB, MMU_PAGE_64KB};

const DR_REG_MMU_TABLE: u32 = 0x600c5000;
const SOC_MMU_VALID_VAL_MASK: u32 = 0x3f;
const SOC_MMU_INVALID: u32 = 1 << 6;

fn soc_mmu_vaddr_mask(mmu_id: u32) -> u32 {
    mmu_ll_get_page_size(mmu_id) * 64 - 1
}

pub fn mmu_ll_get_page_size(_mmu_id: u32) -> u32 {
    // SAFETY: read-only access to a PAC register block that is always valid.
    let extmem = unsafe { &*esp32c2::EXTMEM::ptr() };
    let cache_mmu_page_size = extmem.cache_conf_misc().read().cache_mmu_page_size().bits();
    match cache_mmu_page_size {
        0 => MMU_PAGE_16KB,
        1 => MMU_PAGE_32KB,
        _ => MMU_PAGE_64KB,
    }
}

pub fn mmu_ll_get_entry_id(mmu_id: u32, vaddr: u32) -> u32 {
    let shift_code = match mmu_ll_get_page_size(mmu_id) {
        MMU_PAGE_64KB => 16,
        MMU_PAGE_32KB => 15,
        MMU_PAGE_16KB => 14,
        _ => {
            error!("mmu_ll_get_entry_id failed!");

            0
        }
    };

    (vaddr & soc_mmu_vaddr_mask(mmu_id)) >> shift_code
}

pub fn mmu_ll_entry_id_to_paddr_base(mmu_id: u32, entry_id: u32) -> u32 {
    let shift_code = match mmu_ll_get_page_size(mmu_id) {
        MMU_PAGE_64KB => 16,
        MMU_PAGE_32KB => 15,
        MMU_PAGE_16KB => 14,
        _ => {
            error!("mmu_ll_entry_id_to_paddr_base failed!");

            0
        }
    };

    (mmu_entry(entry_id) & SOC_MMU_VALID_VAL_MASK) << shift_code
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
