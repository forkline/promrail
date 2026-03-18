use std::fmt;
use std::io;

use console::Style;
use fern::FormatCallback;

const COLOR_BRIGHT_BLUE: u8 = 33;
const COLOR_BRIGHT_BLACK: u8 = 244;

fn log_format(out: FormatCallback, message: &fmt::Arguments, record: &log::Record) {
    let level = record.level();
    let target = record.target();

    let style = match (level, target) {
        (log::Level::Error, _) => Style::new().red(),
        (log::Level::Warn, _) => Style::new().magenta(),
        (log::Level::Info, "diff") => Style::new().color256(COLOR_BRIGHT_BLACK),
        (log::Level::Info, _) => Style::new().white(),
        (log::Level::Debug, _) => Style::new().color256(COLOR_BRIGHT_BLUE),
        (log::Level::Trace, _) => Style::new().color256(COLOR_BRIGHT_BLACK),
    };

    let prefix = match level {
        log::Level::Error => "ERROR: ",
        log::Level::Warn => "WARN: ",
        log::Level::Info => "",
        log::Level::Debug => "DEBUG: ",
        log::Level::Trace => "",
    };

    out.finish(format_args!(
        "{}",
        style.apply_to(format!("{prefix}{message}"))
    ))
}

pub fn setup_logging(verbosity: u8) -> Result<(), log::SetLoggerError> {
    let level = match verbosity {
        0 => log::LevelFilter::Info,
        1 => log::LevelFilter::Debug,
        _ => log::LevelFilter::Trace,
    };

    fern::Dispatch::new()
        .level(level)
        .format(log_format)
        .chain(
            fern::Dispatch::new()
                .filter(|metadata| metadata.level() < log::LevelFilter::Warn)
                .chain(io::stdout()),
        )
        .chain(
            fern::Dispatch::new()
                .level(log::LevelFilter::Warn)
                .chain(io::stderr()),
        )
        .apply()
}
