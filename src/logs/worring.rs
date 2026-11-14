//! Bridge from the `log` crate (warnings/errors of third party crates) into the
//! application's own log file.
//!
//! Slint and its dependencies report problems through `log`; without this hook
//! those messages would only reach stderr and be lost in a release build.

use log::{Level, LevelFilter, Metadata, Record};
use std::sync::Once;

/// Handler installed by the application; stored in a static `Option` because it
/// has to be reachable from the `log` callback, which cannot capture anything.
///
/// Safety: it is written exactly once from [`set_warning_hook`] before the
/// logger is activated and only read afterwards, so there is no data race.
static mut HANDLER: Option<Box<dyn Fn(&str) + Send + Sync>> = None;

/// Installs the process wide `log` logger that forwards `warn` and `error`
/// records to `handler` (which usually writes them to the log file).
pub fn set_warning_hook<F>(handler: F)
where
    F: Fn(&str) + Send + Sync + 'static,
{
    unsafe {
        HANDLER = Some(Box::new(handler));
    }

    // `set_boxed_logger` may only be called once per process; `Once` keeps
    // repeated calls (for example in tests) harmless.
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        struct Logger;

        impl log::Log for Logger {
            fn enabled(&self, metadata: &Metadata) -> bool {
                metadata.level() <= Level::Warn
            }

            fn log(&self, record: &Record) {
                unsafe {
                    if let Some(ref handler) = HANDLER {
                        // Only warnings and errors are interesting; info/debug
                        // from dependencies would flood the log file.
                        if record.level() == Level::Warn || record.level() == Level::Error {
                            handler(&format!("{}", record.args()));
                        }
                    }
                }
            }

            fn flush(&self) {}
        }

        log::set_boxed_logger(Box::new(Logger)).unwrap();
        log::set_max_level(LevelFilter::Warn);
    });
}
