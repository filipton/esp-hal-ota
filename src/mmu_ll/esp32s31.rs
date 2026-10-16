//! ESP32-S31: flash MMU (SPI0 = `FLASH_SPI0`), page size from `MMU_POWER_CTRL`, 1024 entries,
//! flash valid bit 12.
super::indexed::indexed_mmu_ll!(esp32s31, entries: 1024, valid: 1 << 12, val_mask: 0x7ff,
page_size: |spi0| match spi0.mmu_power_ctrl().read().mmu_page_size().bits() {
    0 => MMU_PAGE_256KB,
    1 => MMU_PAGE_128KB,
    2 => MMU_PAGE_64KB,
    _ => MMU_PAGE_32KB,
});
