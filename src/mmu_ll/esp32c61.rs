//! ESP32-C61: page size from `MMU_POWER_CTRL`, 512 entries, valid bit 10.
super::indexed::indexed_mmu_ll!(esp32c61, entries: 512, valid: 1 << 10, val_mask: 0x1ff,
page_size: |spi0| match spi0.mmu_power_ctrl().read().mmu_page_size().bits() {
    0 => MMU_PAGE_64KB,
    1 => MMU_PAGE_32KB,
    2 => MMU_PAGE_16KB,
    _ => MMU_PAGE_8KB,
});
