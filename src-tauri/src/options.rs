use std::fs::File;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Options {
    rpc_enabled: bool,
    rpc_show_server: bool,
    rpc_show_mc_version: bool,
    rpc_show_time_played: bool,
    misc_hide_on_close: bool,
    misc_close_after_injected: bool,
    last_used_version: Option<String>,
}

#[derive(Default)]
pub struct OptionsStore {
    options: Options,
}

impl OptionsStore {
    pub fn load() -> Result<Self, String> {
        let options_path = crate::paths::get_options_path()?;
        println!("Loading options from: {}", options_path.display());

        if !options_path.exists() {
            let store = Self::default();
            store.save()?;
            return Ok(store);
        }

        let options_file = File::open(&options_path).map_err(|error| {
            format!(
                "Failed to open options file at {}: {error}",
                options_path.display()
            )
        })?;

        let options = serde_json::from_reader(options_file).map_err(|error| {
            format!(
                "Failed to parse options file at {}: {error}",
                options_path.display()
            )
        })?;

        Ok(Self { options })
    }

    pub fn save(&self) -> Result<(), String> {
        let options_path = crate::paths::get_options_path()?;
        let options_file = File::create(&options_path).map_err(|error| {
            format!(
                "Failed to create options file at {}: {error}",
                options_path.display()
            )
        })?;

        serde_json::to_writer_pretty(options_file, &self.options).map_err(|error| {
            format!(
                "Failed to write options file at {}: {error}",
                options_path.display()
            )
        })
    }

    pub fn get_bool(&self, id: &str) -> Result<bool, String> {
        self.options.bool_option(id).copied()
    }

    pub fn set_bool(&mut self, id: &str, value: bool) -> Result<(), String> {
        *self.options.bool_option_mut(id)? = value;
        Ok(())
    }

    pub fn last_used_version(&self) -> Option<&str> {
        self.options.last_used_version.as_deref()
    }

    pub fn set_last_used_version(&mut self, version: Option<String>) {
        self.options.last_used_version = version;
    }
}

impl Default for Options {
    fn default() -> Self {
        Self {
            rpc_enabled: true,
            rpc_show_server: true,
            rpc_show_mc_version: true,
            rpc_show_time_played: true,
            misc_hide_on_close: false,
            misc_close_after_injected: false,
            last_used_version: None,
        }
    }
}

impl Options {
    fn bool_option(&self, id: &str) -> Result<&bool, String> {
        match id {
            "rpc_enabled" => Ok(&self.rpc_enabled),
            "rpc_show_server" => Ok(&self.rpc_show_server),
            "rpc_show_mc_version" => Ok(&self.rpc_show_mc_version),
            "rpc_show_time_played" => Ok(&self.rpc_show_time_played),
            "misc_hide_on_close" => Ok(&self.misc_hide_on_close),
            "misc_close_after_injected" => Ok(&self.misc_close_after_injected),
            _ => Err(format!("Unknown option: {id}")),
        }
    }

    fn bool_option_mut(&mut self, id: &str) -> Result<&mut bool, String> {
        match id {
            "rpc_enabled" => Ok(&mut self.rpc_enabled),
            "rpc_show_server" => Ok(&mut self.rpc_show_server),
            "rpc_show_mc_version" => Ok(&mut self.rpc_show_mc_version),
            "rpc_show_time_played" => Ok(&mut self.rpc_show_time_played),
            "misc_hide_on_close" => Ok(&mut self.misc_hide_on_close),
            "misc_close_after_injected" => Ok(&mut self.misc_close_after_injected),
            _ => Err(format!("Unknown option: {id}")),
        }
    }
}
