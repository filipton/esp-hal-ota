# Migrating

## 0.6 → 0.7

0.7 cleans up the API: shorter names, `Result` everywhere, no floats, and an
`Ota::finish` that always verifies the image. Flash/otadata behaviour is unchanged.

### Renamed methods

| 0.6 | 0.7 |
|---|---|
| `ota_begin(size, crc)` | `begin(size, crc)` |
| `ota_write_chunk(chunk)` | `write(chunk)` |
| `ota_flush(verify, rollback)` | `finish(rollback)` (always verifies) |
| `ota_verify()` | `verify()` |
| `ota_resume(size, remaining, crc, last_crc, verify)` | `resume(&ResumeInfo, verify)` |
| `get_progress_details()` | `resume_info()` |
| `get_ota_progress() -> f32` | `progress() -> Option<Progress>` |
| `ota_mark_app_valid()` | `mark_app_valid()` |
| `ota_mark_app_invalid_rollback()` | `mark_app_invalid()` |
| `get_ota_image_state()` | `image_state()` |
| `get_currently_booted_partition()` | `booted_partition()` |
| `get_next_ota_partition()` | `next_partition()` |
| `get_ota_boot_entries()` | `boot_entries()` (now returns `Result`) |
| `set_target_ota_boot_partition(target, state)` (errors only logged) | `set_boot_partition(target, state) -> Result<()>` |
| `set_ota_state(slot, state)` | removed (use `mark_app_valid` / `mark_app_invalid`) |

```rust
// 0.6
ota.ota_begin(size, crc)?;
if ota.ota_write_chunk(chunk)? {
    ota.ota_flush(true, true)?;
}
let percent = (ota.get_ota_progress() * 100.0) as u8;

// 0.7
ota.begin(size, crc)?;
if ota.write(chunk)? {
    ota.finish(true)?;
}
let percent = ota.progress().map(|p| p.percent()).unwrap_or(0);
```

`finish` checks, in this order: everything written (`Error::Incomplete`), crc of the
written data (`Error::Crc`), crc read back from flash (`Error::Verify`). Skipping the
read-back is no longer possible.

### Resuming

```rust
// 0.6
let details = ota.get_progress_details().unwrap(); // remaining, last_crc
// ... persist size, crc, details ...
ota.ota_resume(size, details.remaining, crc, details.last_crc, true)?;

// 0.7
let info = ota.resume_info().unwrap(); // size, remaining, target_crc, last_crc
// ... persist info ...
ota.resume(&info, true)?;
```

`ResumeInfo` also carries `size` and `target_crc`, so nothing else has to be stored.

### Errors

`OtaError` is now `Error`, implements `Display` + `core::error::Error`, and is `Copy`.

| 0.6 | 0.7 |
|---|---|
| `OtaPartitionTooSmall` | `PartitionTooSmall` |
| `NotEnoughPartitions` (also used for missing otadata) | `NotEnoughPartitions` / `NoOtadata` |
| `OtaNotStarted` | `NotStarted` |
| `FlashRWError` | `Flash` |
| `WrongCRC` (also used for an incomplete image) | `Crc` / `Incomplete` |
| `WrongOTAPArtitionOrder` | `PartitionOrder` |
| `OtaVerifyError` | `Verify` |
| `CannotFindCurrentBootPartition` | `UnknownBootPartition` |
| – | `SeqExhausted`, `InvalidResumeInfo` |

### Image state

`OtaImgState` variants lost their `EspOtaImg` prefix: `EspOtaImgValid` → `Valid`,
`EspOtaImgNew` → `New`, `EspOtaImgPendingVerify` → `PendingVerify`,
`EspOtaImgInvalid` → `Invalid`, `EspOtaImgAborted` → `Aborted`,
`EspOtaImgUndefined` → `Undefined`.

### Removed from the public API

- `structs`, `helpers`, `mmu_hal`, `mmu_ll` modules (all public types are at the crate root).
- `FlashProgress`, `PartitionInfo`, `ProgressDetails` (use `Progress` / `ResumeInfo`).
- `EspOtaSelectEntry::check_crc` (entries from `boot_entries` are already checked).
- the deprecated `OtaConfiguratuion` alias (use `OtaConfiguration`).
