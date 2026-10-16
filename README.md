# esp-hal-ota
OTA for esp-hal (no-std).

Until `esp-bootloader-esp-idf` 0.3, esp-hal had no OTA support and this crate (a port of ESP-IDF's
C OTA logic) was the only OTA solution for esp-hal. esp-hal now ships basic OTA primitives
(`OtaUpdater`); this crate still uses its own implementation because upstream currently mishandles
a few real-world cases (erased otadata after flashing, a corrupt otadata entry), and adds a
complete streaming updater on top:

- streaming writes of image chunks
- CRC32 check against the expected image CRC
- read-back verify before activation
- progress and resume of interrupted updates
- rollback / mark valid on the booted partition
- tolerant of an erased or corrupt otadata entry

[![crates.io](https://img.shields.io/crates/v/esp-hal-ota.svg)](https://crates.io/crates/esp-hal-ota)
[![MIT license](https://img.shields.io/github/license/mashape/apistatus.svg)]()

## Limitations
I cannot test if it works properly on esp32s2,esp32c2,esp32c6 and esp32h2.
But esp32s3, esp32c3 and esp32 are working perfectly fine.

## Features
- Obviously OTA updates
- Dynamic partitions reading (so no macros, no reading from partitions.csv) - fully automatic
- Checking currently booted partition (using some pointer magic from ESP-IDF)
- CRC32 verification
- No allocations, no panics (errors are returned)

## Getting started
- Enable the `embedded-storage` feature of `esp-storage` (opt-in since `esp-storage` 0.10)
- Create `partitions.csv` file in project root (copy `partitions.csv.template` file)
- In your project edit `./.cargo/config.toml` file and append `--partition-table ./partitions.csv` to the `runner` attribute
- Optionally append `--erase-parts otadata` to `./cargo/config.toml` to fix some ota issues

```toml
[target.xtensa-esp32s3-none-elf]
runner = "espflash flash --monitor --partition-table ./partitions.csv --erase-parts otadata"
```

## Upgrading
See [MIGRATING.md](MIGRATING.md) for breaking changes between versions.

## Example
To see real-world example look at `./examples` and `./simple-ota-server` dirs.

```rust,ignore
let flash_size = 1234; // get it from OTA server
let target_crc = 65436743; // get it from OTA server (crc32, e.g. `crc32fast::hash`)

let mut ota = Ota::new(FlashStorage::new(peripherals.FLASH))?;
ota.begin(flash_size, target_crc)?;

let mut buf = [0; 4096];
loop {
    let n = read_next_chunk(&mut buf);
    if n == 0 {
        break;
    }

    if ota.write(&buf[..n])? {
        // whole image written: checks crc (also read back from flash), enables rollback
        ota.finish(true)?;
        esp_hal::system::software_reset();
    }

    if let Some(progress) = ota.progress() {
        log::info!("progress: {}%", progress.percent());
    }
}
```

After booting the new image call `ota.mark_app_valid()`, otherwise (with rollbacks enabled)
the bootloader goes back to the previous image on the next reset.

### Running example
- You can compile your .bin file using esp-flash 
```bash
espflash save-image --chip esp32c3 ./target/riscv32imc-unknown-none-elf/debug/esp-hal-ota-example ../simple-ota-server/firmware.bin
```

This will generate .bin file from build file for chip.

## Todo
- [x] Fully working library
- [x] Simple example
- [x] Better errors
- [x] Other esp32's (like esp32c3, esp32s2, etc..)
- [x] Rollbacks

## Resources
- https://github.com/esp-rs/espflash (this led me to esp-idf-part)
- https://github.com/esp-rs/esp-idf-part
- https://github.com/espressif/esp-idf/blob/master/docs/en/api-reference/system/ota.rst (especially Python API)
- https://github.com/python/cpython/blob/main/Modules/binascii.c (internal_crc32)
- https://github.com/espressif/esp-idf/blob/master/components/app_update/esp_ota_ops.c#L552 (esp get current partition (paddr))
- https://github.com/bjoernQ/esp32c3-ota-experiment (i've only seen this after first experiments, so haven't looked at it yet)
