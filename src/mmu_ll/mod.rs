#[cfg(feature = "esp32s2")]
mod esp32s2;
#[cfg(feature = "esp32s2")]
pub use esp32s2::*;

#[cfg(feature = "esp32s3")]
mod esp32s3;
#[cfg(feature = "esp32s3")]
pub use esp32s3::*;

#[cfg(feature = "esp32c2")]
mod esp32c2;
#[cfg(feature = "esp32c2")]
pub use esp32c2::*;

#[cfg(feature = "esp32c3")]
mod esp32c3;
#[cfg(feature = "esp32c3")]
pub use esp32c3::*;

#[cfg(feature = "esp32c6")]
mod esp32c6;
#[cfg(feature = "esp32c6")]
pub use esp32c6::*;

#[cfg(feature = "esp32h2")]
mod esp32h2;
#[cfg(feature = "esp32h2")]
pub use esp32h2::*;
