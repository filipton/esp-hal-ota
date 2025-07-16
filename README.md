# esp-hal-ota
OTA for esp-hal (no-std).

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

## Getting started
- Create `partitions.csv` file in project root (copy `partitions.csv.template` file)
- In your project edit `./.cargo/config.toml` file and append `--partition-table ./partitions.csv` to the `runner` attribute
- Optionally append `--erase-parts otadata` to `./cargo/config.toml` to fix some ota issues

```toml
[target.xtensa-esp32s3-none-elf]
runner = "espflash flash --monitor --partition-table ./partitions.csv --erase-parts otadata"
```

## Example
To see real-world example look at `./examples` and `./simple-ota-server` dirs.

```rust
let flash_size = 1234; // get it from OTA server
let target_crc = 65436743; // get it from OTA server

let mut ota = Ota::new(FlashStorage::new()).unwrap();
ota.ota_begin(flash_size, target_crc);

let mut buf = [0; 4096];
loop {
    let n = read_next_chunk(&mut buf);
    if n == 0 {
        break;
    }

    let res = ota.ota_write_chunk(&buf[..n]);
    if res == Ok(true) { // end of flash
        if ota.ota_flush(true, true).is_ok() { // true if you want to verify crc reading flash, and true if you want rollbacks
            esp_hal::reset::software_reset();
        }
    }

    let progress = (ota.get_ota_progress() * 100) as u8;
    log::info!("progress: {}%", progress);
}
```

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
