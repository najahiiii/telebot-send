use chrono::Local;
use once_cell::sync::Lazy;
use std::fmt;
use std::sync::Mutex;

static LOG_LOCK: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

pub(crate) fn log(level: &str, args: fmt::Arguments<'_>) {
    if let Ok(guard) = LOG_LOCK.lock() {
        let timestamp = Local::now().format("%Y-%m-%d %H:%M:%S");
        println!("[{}] - {} - {}", timestamp, level, args);
        drop(guard);
    }
}

#[macro_export]
macro_rules! log_info {
    ($($arg:tt)*) => {
        $crate::logger::log("INFO", format_args!($($arg)*));
    };
}

#[macro_export]
macro_rules! log_error {
    ($($arg:tt)*) => {
        $crate::logger::log("ERROR", format_args!($($arg)*));
    };
}

#[macro_export]
macro_rules! log_debug {
    ($($arg:tt)*) => {
        $crate::logger::log("DEBUG", format_args!($($arg)*));
    };
}
