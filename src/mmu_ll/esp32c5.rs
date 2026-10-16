//! ESP32-C5: page size fixed at 64 KB, 512 entries, valid bit 10.
super::indexed::indexed_mmu_ll!(esp32c5, entries: 512, valid: 1 << 10, val_mask: 0x1ff,
    page_size: |spi0| MMU_PAGE_64KB);
