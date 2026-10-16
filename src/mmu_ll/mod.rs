#[cfg(feature = "esp32")]
mod esp32;
#[cfg(feature = "esp32")]
pub use esp32::*;

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

#[cfg(any(feature = "esp32c5", feature = "esp32c61", feature = "esp32s31"))]
mod indexed;

#[cfg(feature = "esp32c5")]
mod esp32c5;
#[cfg(feature = "esp32c5")]
pub use esp32c5::*;

#[cfg(feature = "esp32c61")]
mod esp32c61;
#[cfg(feature = "esp32c61")]
pub use esp32c61::*;

#[cfg(feature = "esp32s31")]
mod esp32s31;
#[cfg(feature = "esp32s31")]
pub use esp32s31::*;

#[cfg(not(any(
    feature = "esp32",
    feature = "esp32s2",
    feature = "esp32s3",
    feature = "esp32c2",
    feature = "esp32c3",
    feature = "esp32c6",
    feature = "esp32h2",
    feature = "esp32c5",
    feature = "esp32c61",
    feature = "esp32s31"
)))]
mod not_selected;

#[cfg(not(any(
    feature = "esp32",
    feature = "esp32s2",
    feature = "esp32s3",
    feature = "esp32c2",
    feature = "esp32c3",
    feature = "esp32c6",
    feature = "esp32h2",
    feature = "esp32c5",
    feature = "esp32c61",
    feature = "esp32s31"
)))]
pub use not_selected::*;
