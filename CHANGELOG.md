# Changelog

## 0.7.0

Breaking API cleanup, see [MIGRATING.md](MIGRATING.md).

- Methods renamed without the `ota_`/`get_` prefixes (`begin`, `write`, `finish`, ...).
- `finish` always reads the image back and verifies its crc.
- Progress as `Progress { written, total }` with `percent()`/`permille()` instead of `f32`.
- `resume` takes a `ResumeInfo` (returned by `resume_info`) instead of five integers.
- `Error` (was `OtaError`) with more precise variants, `Display` and `core::error::Error`.
- `set_boot_partition` and `boot_entries` return `Result` instead of logging/swallowing errors.
- Internal modules and types are no longer public.

## 0.6.2

- Sound otadata parsing, flash errors propagated, single-write boot entries (see git history).
