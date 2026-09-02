//! Leveled logging to stderr with elapsed time. Five macros, one global level.
//!
//! ```ignore
//! log_info!("loaded {} objects", n);
//! log_warn!("mesh {} has {} loose verts", id, count);
//! ```

use core::fmt;
use core::sync::atomic::{AtomicU8, Ordering};
use std::sync::OnceLock;
use std::time::Instant;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
pub enum Level {
    Error = 1,
    Warn = 2,
    Info = 3,
    Debug = 4,
    Trace = 5,
}

impl Level {
    fn from_u8(v: u8) -> Level {
        match v {
            1 => Level::Error,
            2 => Level::Warn,
            3 => Level::Info,
            4 => Level::Debug,
            _ => Level::Trace,
        }
    }

    pub const fn name(self) -> &'static str {
        match self {
            Level::Error => "ERROR",
            Level::Warn => "WARN ",
            Level::Info => "INFO ",
            Level::Debug => "DEBUG",
            Level::Trace => "TRACE",
        }
    }
}

const DEFAULT_LEVEL: u8 = if cfg!(debug_assertions) { Level::Debug as u8 } else { Level::Info as u8 };
static LEVEL: AtomicU8 = AtomicU8::new(DEFAULT_LEVEL);
static START: OnceLock<Instant> = OnceLock::new();

pub fn set_level(level: Level) {
    LEVEL.store(level as u8, Ordering::Relaxed);
}

pub fn level() -> Level {
    Level::from_u8(LEVEL.load(Ordering::Relaxed))
}

#[inline]
pub fn enabled(level: Level) -> bool {
    level as u8 <= LEVEL.load(Ordering::Relaxed)
}

/// Seconds since the first log call (or since [`init`]).
pub fn elapsed() -> f64 {
    START.get_or_init(Instant::now).elapsed().as_secs_f64()
}

/// Start the clock now instead of at the first message.
pub fn init() {
    let _ = START.get_or_init(Instant::now);
}

#[doc(hidden)]
pub fn write(level: Level, module: &str, args: fmt::Arguments<'_>) {
    // Trim the crate prefix so lines read `WARN  mesh::validate: …`.
    let module = module.strip_prefix("prism_").unwrap_or(module);
    eprintln!("[{:9.3}] {} {}: {}", elapsed(), level.name(), module, args);
}

#[macro_export]
macro_rules! log_at {
    ($lvl:expr, $($arg:tt)+) => {
        if $crate::log::enabled($lvl) {
            $crate::log::write($lvl, module_path!(), format_args!($($arg)+));
        }
    };
}

#[macro_export]
macro_rules! log_error { ($($arg:tt)+) => { $crate::log_at!($crate::log::Level::Error, $($arg)+) }; }
#[macro_export]
macro_rules! log_warn { ($($arg:tt)+) => { $crate::log_at!($crate::log::Level::Warn, $($arg)+) }; }
#[macro_export]
macro_rules! log_info { ($($arg:tt)+) => { $crate::log_at!($crate::log::Level::Info, $($arg)+) }; }
#[macro_export]
macro_rules! log_debug { ($($arg:tt)+) => { $crate::log_at!($crate::log::Level::Debug, $($arg)+) }; }
#[macro_export]
macro_rules! log_trace { ($($arg:tt)+) => { $crate::log_at!($crate::log::Level::Trace, $($arg)+) }; }

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn level_gating() {
        set_level(Level::Warn);
        assert!(enabled(Level::Error));
        assert!(enabled(Level::Warn));
        assert!(!enabled(Level::Info));
        assert_eq!(level(), Level::Warn);
        set_level(Level::Trace);
        assert!(enabled(Level::Trace));
        // Macros expand and compile with any formatting.
        log_info!("hello {} {:?}", 1, "two");
        log_trace!("plain");
    }
}
