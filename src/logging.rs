macro_rules! debug {
    ($($arg:tt)*) => {
        #[cfg(feature = "defmt")]
        defmt::debug!($($arg)*);
        #[cfg(feature = "log")]
        log::debug!($($arg)*);
    };
}

macro_rules! info {
    ($($arg:tt)*) => {
        #[cfg(feature = "defmt")]
        defmt::info!($($arg)*);
        #[cfg(feature = "log")]
        log::info!($($arg)*);
    };
}

macro_rules! warn {
    ($($arg:tt)*) => {
        #[cfg(feature = "defmt")]
        defmt::warn!($($arg)*);
        #[cfg(feature = "log")]
        log::warn!($($arg)*);
    };
}

macro_rules! error {
    ($($arg:tt)*) => {
        #[cfg(feature = "defmt")]
        defmt::error!($($arg)*);
        #[cfg(feature = "log")]
        log::error!($($arg)*);
    };
}
