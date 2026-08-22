use log::{Level, LevelFilter, Metadata, Record};
use std::sync::Once;

static mut HANDLER: Option<Box<dyn Fn(&str) + Send + Sync>> = None;

pub fn set_warning_hook<F>(handler: F)
where
    F: Fn(&str) + Send + Sync + 'static,
{
    unsafe {
        HANDLER = Some(Box::new(handler));
    }

    static INIT: Once = Once::new();
    INIT.call_once(|| {
        struct Logger;
        impl log::Log for Logger {
            fn enabled(&self, metadata: &Metadata) -> bool {
                metadata.level() <= Level::Warn
            }
            fn log(&self, record: &Record) {
                unsafe {
                    if let Some(ref h) = HANDLER {
                        if record.level() == Level::Warn || record.level() == Level::Error {
                            h(&format!("{}", record.args()));
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