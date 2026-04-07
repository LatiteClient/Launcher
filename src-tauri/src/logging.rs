use std::{
    fmt,
    fs::{File, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
};

use chrono::Local;

static LOGGER: OnceLock<Logger> = OnceLock::new();

#[derive(Clone, Copy)]
pub enum LogLevel {
    Info,
    Error,
}

impl LogLevel {
    fn label(self) -> &'static str {
        match self {
            Self::Info => "INFO",
            Self::Error => "ERROR",
        }
    }
}

struct Logger {
    state: Mutex<LoggerState>,
}

struct LoggerState {
    logs_path: PathBuf,
    latest_file: File,
    archive_file: File,
    archive_date: String,
}

impl Logger {
    fn new() -> Result<(Self, PathBuf, PathBuf), String> {
        let logs_path = crate::paths::get_logs_path()?;
        let latest_log_path = logs_path.join("latest.log");
        let archive_date = current_date_string();
        let archive_log_path = archive_log_path(&logs_path, &archive_date);

        let latest_file = open_latest_log(&latest_log_path)?;
        let archive_file = open_archive_log(&archive_log_path)?;

        Ok((
            Self {
                state: Mutex::new(LoggerState {
                    logs_path,
                    latest_file,
                    archive_file,
                    archive_date,
                }),
            },
            latest_log_path,
            archive_log_path,
        ))
    }

    fn write_entry(&self, entry: &str) -> Result<(), String> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| "Launcher logger state is unavailable.".to_string())?;
        state.rotate_archive_if_needed()?;

        state
            .latest_file
            .write_all(entry.as_bytes())
            .map_err(|error| format!("Failed to write latest.log: {error}"))?;
        state
            .latest_file
            .flush()
            .map_err(|error| format!("Failed to flush latest.log: {error}"))?;

        state
            .archive_file
            .write_all(entry.as_bytes())
            .map_err(|error| format!("Failed to write archive log: {error}"))?;
        state
            .archive_file
            .flush()
            .map_err(|error| format!("Failed to flush archive log: {error}"))?;

        Ok(())
    }
}

impl LoggerState {
    fn rotate_archive_if_needed(&mut self) -> Result<(), String> {
        let current_date = current_date_string();
        if current_date == self.archive_date {
            return Ok(());
        }

        let archive_log_path = archive_log_path(&self.logs_path, &current_date);
        self.archive_file = open_archive_log(&archive_log_path)?;
        self.archive_date = current_date;
        Ok(())
    }
}

pub fn init() -> Result<(), String> {
    if LOGGER.get().is_some() {
        return Ok(());
    }

    let (logger, latest_log_path, archive_log_path) = Logger::new()?;
    if LOGGER.set(logger).is_err() {
        return Ok(());
    }

    log(
        LogLevel::Info,
        format_args!(
            "Launcher logging initialized. Writing to {} and {}.",
            latest_log_path.display(),
            archive_log_path.display()
        ),
    );

    Ok(())
}

pub fn log(level: LogLevel, args: fmt::Arguments<'_>) {
    let message = format!("{args}");
    let timestamp = timestamp_string();
    let mut wrote_line = false;

    for line in message.lines() {
        wrote_line = true;
        write_entry(level, &timestamp, line);
    }

    if !wrote_line {
        write_entry(level, &timestamp, "");
    }
}

pub fn log_startup_error(message: &str) {
    let entry = build_entry(LogLevel::Error, &timestamp_string(), message);
    write_console(LogLevel::Error, &entry);
}

fn write_entry(level: LogLevel, timestamp: &str, message: &str) {
    let entry = build_entry(level, timestamp, message);
    write_console(level, &entry);

    if let Some(logger) = LOGGER.get() {
        if let Err(error) = logger.write_entry(&entry) {
            let failure = build_entry(
                LogLevel::Error,
                &timestamp_string(),
                &format!("Failed to write launcher log files: {error}"),
            );
            write_console(LogLevel::Error, &failure);
        }
    }
}

fn build_entry(level: LogLevel, timestamp: &str, message: &str) -> String {
    format!("{timestamp} [{}] {message}\n", level.label())
}

fn write_console(level: LogLevel, entry: &str) {
    match level {
        LogLevel::Info => {
            let mut stdout = io::stdout().lock();
            let _ = stdout.write_all(entry.as_bytes());
            let _ = stdout.flush();
        }
        LogLevel::Error => {
            let mut stderr = io::stderr().lock();
            let _ = stderr.write_all(entry.as_bytes());
            let _ = stderr.flush();
        }
    }
}

fn open_latest_log(path: &Path) -> Result<File, String> {
    OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(path)
        .map_err(|error| format!("Failed to open {}: {error}", path.display()))
}

fn open_archive_log(path: &Path) -> Result<File, String> {
    OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|error| format!("Failed to open {}: {error}", path.display()))
}

fn archive_log_path(logs_path: &Path, date: &str) -> PathBuf {
    logs_path.join(format!("LatiteLauncher-{date}.log"))
}

fn current_date_string() -> String {
    Local::now().format("%Y-%m-%d").to_string()
}

fn timestamp_string() -> String {
    Local::now().format("%Y-%m-%d %H:%M:%S%.3f").to_string()
}

#[macro_export]
macro_rules! log_info {
    ($($arg:tt)*) => {
        $crate::logging::log($crate::logging::LogLevel::Info, format_args!($($arg)*))
    };
}

#[macro_export]
macro_rules! log_error {
    ($($arg:tt)*) => {
        $crate::logging::log($crate::logging::LogLevel::Error, format_args!($($arg)*))
    };
}
