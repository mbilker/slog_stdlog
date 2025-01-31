//! `log` crate adapter for slog-rs
//!
//! This crate provides two way compatibility with Rust standard `log` crate.
//!
//! ### `log` -> `slog`
//!
//! After calling `init()` `slog-stdlog` will take a role of `log` crate
//! back-end, forwarding all the `log` logging to `slog_scope::logger()`.
//! In other words, any `log` crate logging statement will behave like it was `slog`
//! logging statement executed with logger returned by `slog_scope::logger()`.
//!
//! See documentation of `slog-scope` for more information about logging scopes.
//!
//! See [`init` documentation](fn.init.html) for an example.
//!
//! ### `slog` -> `log`
//!
//! `StdLog` is `slog::Drain` that will pass all `Record`s passing through it to
//! `log` crate just like they were crated with `log` crate logging macros in
//! the first place.
//!
//! ## `slog-scope`
//!
//! Since `log` does not have any form of context, and does not support `Logger`
//! `slog-stdlog` relies on "logging scopes" to establish it.
//!
//! You must set up logging context for `log` -> `slog` via `slog_scope::scope`
//! or `slog_scope::set_global_logger`. Setting a global logger upfront via
//! `slog_scope::set_global_logger` is highly recommended.
//!
//! Note: Since `slog-stdlog` v2, unlike previous releases, `slog-stdlog` uses
//! logging scopes provided by `slog-scope` crate instead of it's own.
//!
//! Refer to `slog-scope` crate documentation for more information.
//!
//! ### Warning
//!
//! Be careful when using both methods at the same time, as a loop can be easily
//! created: `log` -> `slog` -> `log` -> ...
//!
//! ## Compile-time log level filtering
//!
//! For filtering `debug!` and other `log` statements at compile-time, configure
//! the features on the `log` crate in your `Cargo.toml`:
//!
//! ```norust
//! log = { version = "*", features = ["max_level_trace", "release_max_level_warn"] }
//! ```
#![warn(missing_docs)]

#[macro_use]
extern crate slog;
extern crate log;
extern crate slog_scope;

use log::LogMetadata;
use std::{fmt, io};

use slog::Level;
use slog::KV;

struct Logger;

fn log_to_slog_level(level: log::LogLevel) -> Level {
    match level {
        log::LogLevel::Trace => Level::Trace,
        log::LogLevel::Debug => Level::Debug,
        log::LogLevel::Info => Level::Info,
        log::LogLevel::Warn => Level::Warning,
        log::LogLevel::Error => Level::Error,
    }
}

impl log::Log for Logger {
    fn enabled(&self, _: &LogMetadata) -> bool {
        true
    }

    fn log(&self, r: &log::LogRecord) {
        let level = log_to_slog_level(r.metadata().level());

        let args = r.args();
        let target = r.target();
        let module = r.location().__module_path;
        let file = r.location().__file;
        let line = r.location().line();

        let s = slog::RecordStatic {
            location: &slog::RecordLocation {
                file: file,
                line: line,
                column: 0,
                function: "",
                module: module,
            },
            level: level,
            tag: target,
        };
        slog_scope::with_logger(|logger| logger.log(&slog::Record::new(&s, args, b!())))
    }
}

/// Register `slog-stdlog` as `log` backend.
///
/// This will pass all logging statements crated with `log`
/// crate to current `slog-scope::logger()`.
///
/// ```
/// #[macro_use]
/// extern crate log;
/// #[macro_use(slog_o, slog_kv)]
/// extern crate slog;
/// extern crate slog_stdlog;
/// extern crate slog_scope;
/// extern crate slog_term;
/// extern crate slog_async;
///
/// use slog::Drain;
///
/// fn main() {
///     let decorator = slog_term::TermDecorator::new().build();
///     let drain = slog_term::FullFormat::new(decorator).build().fuse();
///     let drain = slog_async::Async::new(drain).build().fuse();
///     let logger = slog::Logger::root(drain, slog_o!("version" => env!("CARGO_PKG_VERSION")));
///
///     let _scope_guard = slog_scope::set_global_logger(logger);
///     let _log_guard = slog_stdlog::init().unwrap();
///     // Note: this `info!(...)` macro comes from `log` crate
///     info!("standard logging redirected to slog");
/// }
/// ```
pub fn init() -> Result<(), log::SetLoggerError> {
    init_with_level(log::LogLevel::max())
}

/// Register `slog-stdlog` as `log` backend.
/// Pass a log::LogLevel to do the log filter explicitly.
///
/// This will pass all logging statements crated with `log`
/// crate to current `slog-scope::logger()`.
///
/// ```
/// #[macro_use]
/// extern crate log;
/// #[macro_use(slog_o, slog_kv)]
/// extern crate slog;
/// extern crate slog_stdlog;
/// extern crate slog_scope;
/// extern crate slog_term;
/// extern crate slog_async;
///
/// use slog::Drain;
///
/// fn main() {
///     let decorator = slog_term::TermDecorator::new().build();
///     let drain = slog_term::FullFormat::new(decorator).build().fuse();
///     let drain = slog_async::Async::new(drain).build().fuse();
///     let logger = slog::Logger::root(drain, slog_o!("version" => env!("CARGO_PKG_VERSION")));
///
///     let _scope_guard = slog_scope::set_global_logger(logger);
///     let _log_guard = slog_stdlog::init_with_level(log::LogLevel::Error).unwrap();
///     // Note: this `info!(...)` macro comes from `log` crate
///     info!("standard logging redirected to slog");
///     error!("standard logging redirected to slog");
/// }
/// ```
pub fn init_with_level(level: log::LogLevel) -> Result<(), log::SetLoggerError> {
    log::set_logger(|max_log_level| {
        max_log_level.set(level.to_log_level_filter());
        Box::new(Logger)
    })
}

/// Drain logging `Record`s into `log` crate
///
/// Any `Record` passing through this `Drain` will be forwarded
/// to `log` crate, just like it was created with `log` crate macros
/// in the first place. The message and key-value pairs will be formated
/// to be one string.
///
/// Caution needs to be taken to prevent circular loop where `Logger`
/// installed via `slog-stdlog::set_logger` would log things to a `StdLog`
/// drain, which would again log things to the global `Logger` and so on
/// leading to an infinite recursion.
pub struct StdLog;

struct LazyLogString<'a> {
    info: &'a slog::Record<'a>,
    logger_values: &'a slog::OwnedKVList,
}

impl<'a> LazyLogString<'a> {
    fn new(info: &'a slog::Record, logger_values: &'a slog::OwnedKVList) -> Self {
        LazyLogString {
            info: info,
            logger_values: logger_values,
        }
    }
}

impl<'a> fmt::Display for LazyLogString<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        try!(write!(f, "{}", self.info.msg()));

        let io = io::Cursor::new(Vec::new());
        let mut ser = KSV::new(io);

        let res = {
            || -> io::Result<()> {
                try!(self.logger_values.serialize(self.info, &mut ser));
                try!(self.info.kv().serialize(self.info, &mut ser));
                Ok(())
            }
        }().map_err(|_| fmt::Error);

        try!(res);

        let values = ser.into_inner().into_inner();

        write!(f, "{}", String::from_utf8_lossy(&values))
    }
}

impl slog::Drain for StdLog {
    type Err = io::Error;
    type Ok = ();
    fn log(&self, info: &slog::Record, logger_values: &slog::OwnedKVList) -> io::Result<()> {
        let level = match info.level() {
            slog::Level::Critical | slog::Level::Error => log::LogLevel::Error,
            slog::Level::Warning => log::LogLevel::Warn,
            slog::Level::Info => log::LogLevel::Info,
            slog::Level::Debug => log::LogLevel::Debug,
            slog::Level::Trace => log::LogLevel::Trace,
        };

        let mut target = info.tag();
        if target.is_empty() {
            target = info.module();
        }

        let location = log::LogLocation {
            __module_path: info.module(),
            __file: info.file(),
            __line: info.line(),
        };

        let lazy = LazyLogString::new(info, logger_values);
        // Please don't yell at me for this! :D
        // https://github.com/rust-lang-nursery/log/issues/95
        log::__log(level, target, &location, format_args!("{}", lazy));

        Ok(())
    }
}

/// Key-Separator-Value serializer
struct KSV<W: io::Write> {
    io: W,
}

impl<W: io::Write> KSV<W> {
    fn new(io: W) -> Self {
        KSV { io: io }
    }

    fn into_inner(self) -> W {
        self.io
    }
}

impl<W: io::Write> slog::Serializer for KSV<W> {
    fn emit_arguments(&mut self, key: slog::Key, val: &fmt::Arguments) -> slog::Result {
        try!(write!(self.io, ", {}: {}", key, val));
        Ok(())
    }
}
