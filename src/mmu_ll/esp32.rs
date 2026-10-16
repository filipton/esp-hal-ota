const SOC_MMU_VADDR_MASK: u32 = 0x3FFFFF;
const SOC_MMU_INVALID: u32 = 1 << 8;
const MMU_LL_PSRAM_ENTRY_START_ID: u32 = 1152;
const SOC_IRAM0_CACHE: core::ops::Range<u32> = 0x400D0000..0x40400000;
const SOC_IRAM1_CACHE: core::ops::Range<u32> = 0x40400000..0x40800000;
const SOC_IROM0_CACHE: core::ops::Range<u32> = 0x40800000..0x40C00000;
const SOC_DRAM1_CACHE: core::ops::Range<u32> = 0x3F800000..0x3FC00000;
const SOC_DROM0_CACHE: core::ops::Range<u32> = 0x3F400000..0x3F800000;
const DPORT_PRO_FLASH_MMU_TABLE: u32 = 0x3FF10000;

pub fn mmu_ll_get_page_size(_mmu_id: u32) -> u32 {
    crate::mmu_hal::MMU_PAGE_64KB
}

pub fn mmu_ll_get_entry_id(_mmu_id: u32, vaddr: u32) -> u32 {
    let mut offset = 0;
    let mut shift_code = 0;
    let mut vaddr_mask = 0;

    if SOC_DROM0_CACHE.contains(&vaddr) {
        offset = 0;
        shift_code = 16;
        vaddr_mask = SOC_MMU_VADDR_MASK;
    } else if SOC_IRAM0_CACHE.contains(&vaddr) {
        offset = 64;
        shift_code = 16;
        vaddr_mask = SOC_MMU_VADDR_MASK;
    } else if SOC_IRAM1_CACHE.contains(&vaddr) {
        offset = 128;
        shift_code = 16;
        vaddr_mask = SOC_MMU_VADDR_MASK;
    } else if SOC_IROM0_CACHE.contains(&vaddr) {
        offset = 192;
        shift_code = 16;
        vaddr_mask = SOC_MMU_VADDR_MASK;
    } else if SOC_DRAM1_CACHE.contains(&vaddr) {
        offset = MMU_LL_PSRAM_ENTRY_START_ID;
        shift_code = 15;
        vaddr_mask = SOC_MMU_VADDR_MASK >> 1;
    } else {
        error!("mmu_ll_get_entry_id failed!");
    }

    offset + ((vaddr & vaddr_mask) >> shift_code)
}

pub fn mmu_ll_entry_id_to_paddr_base(_mmu_id: u32, entry_id: u32) -> u32 {
    let mmu_val = mmu_entry(entry_id);

    if entry_id >= MMU_LL_PSRAM_ENTRY_START_ID {
        mmu_val << 15
    } else {
        mmu_val << 16
    }
}

pub fn mmu_ll_check_entry_valid(_mmu_id: u32, entry_id: u32) -> bool {
    let mmu_val = mmu_entry(entry_id);

    (mmu_val & SOC_MMU_INVALID) == 0
}

/// Reads MMU table entry `entry_id`.
fn mmu_entry(entry_id: u32) -> u32 {
    // SAFETY: DPORT reads need high-priority interrupts masked (ESP32 DPORT erratum, same as
    // ESP-IDF's `DPORT_INTERRUPT_DISABLE`). The table is always mapped and `entry_id` comes from
    // a masked virtual address, so it stays inside it. The old level is restored right after.
    unsafe {
        let level = dport_interrupt_disable();
        let mmu_val = (DPORT_PRO_FLASH_MMU_TABLE as *const u32)
            .add(entry_id as usize)
            .read_volatile();
        dport_interrupt_restore(level);
        mmu_val
    }
}

const SOC_DPORT_WORKAROUND_DIS_INTERRUPT_LVL: u32 = 5;

/// Raises the interrupt level, returns the old `PS` value.
///
/// # Safety
/// Must be paired with [`dport_interrupt_restore`] with the returned value.
#[inline(always)]
unsafe fn dport_interrupt_disable() -> u32 {
    let level: u32;

    // no `nomem`: the asm must act as a compiler barrier around the DPORT read
    unsafe {
        core::arch::asm!(
            "rsil {0}, {1}",
            out(reg) level,
            const SOC_DPORT_WORKAROUND_DIS_INTERRUPT_LVL,
            options(nostack)
        );
    }

    level
}

/// Restores `PS` saved by [`dport_interrupt_disable`].
///
/// # Safety
/// `level` must come from [`dport_interrupt_disable`].
#[inline(always)]
unsafe fn dport_interrupt_restore(level: u32) {
    unsafe {
        core::arch::asm!(
            "wsr.ps {0}; rsync",
            in(reg) level,
            options(nostack)
        );
    }
}
