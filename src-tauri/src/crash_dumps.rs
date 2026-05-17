use std::{
    fs,
    path::Path,
    time::{Duration, SystemTime},
};

const MAX_CRASH_DUMP_AGE: Duration = Duration::from_secs(7 * 24 * 60 * 60);

pub fn delete_old_crash_dumps() -> Result<usize, String> {
    let crash_dumps_path = crate::paths::get_crash_dumps_path()?;

    if !crash_dumps_path.exists() {
        return Ok(0);
    }

    let now = SystemTime::now();
    let mut deleted_count = 0;
    let entries = fs::read_dir(&crash_dumps_path).map_err(|error| {
        format!(
            "Failed to read crash dumps directory {}: {error}",
            crash_dumps_path.display()
        )
    })?;

    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                crate::log_error!(
                    "Failed to read an entry in crash dumps directory {}: {error}",
                    crash_dumps_path.display()
                );
                continue;
            }
        };
        let path = entry.path();

        if !is_crash_dump_file(&path) {
            continue;
        }

        let metadata = match entry.metadata() {
            Ok(metadata) => metadata,
            Err(error) => {
                crate::log_error!(
                    "Failed to read metadata for crash dump {}: {error}",
                    path.display()
                );
                continue;
            }
        };

        if !metadata.is_file() {
            continue;
        }

        let modified = match metadata.modified() {
            Ok(modified) => modified,
            Err(error) => {
                crate::log_error!(
                    "Failed to read modified time for crash dump {}: {error}",
                    path.display()
                );
                continue;
            }
        };

        let Ok(age) = now.duration_since(modified) else {
            continue;
        };

        if age <= MAX_CRASH_DUMP_AGE {
            continue;
        }

        match fs::remove_file(&path) {
            Ok(()) => deleted_count += 1,
            Err(error) => crate::log_error!(
                "Failed to delete old crash dump {}: {error}",
                path.display()
            ),
        }
    }

    Ok(deleted_count)
}

fn is_crash_dump_file(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("dmp"))
}
