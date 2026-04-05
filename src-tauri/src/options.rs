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
    latite_nightly: bool,
    latite_debug: bool,
    use_custom_dll: bool,
    custom_dlls: String,
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

        // Try to parse the options file, if it fails due to missing fields, use defaults
        match serde_json::from_reader(options_file) {
            Ok(options) => Ok(Self { options }),
            Err(error) => {
                eprintln!("Failed to parse options file, using defaults: {error}");
                // Remove the old file and create a new one with defaults
                let _ = std::fs::remove_file(&options_path);
                let store = Self::default();
                store.save()?;
                Ok(store)
            }
        }
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

    pub fn get_string(&self, id: &str) -> Result<String, String> {
        match id {
            "custom_dlls" => Ok(self.options.custom_dlls.clone()),
            _ => Err(format!("Unknown string option: {id}")),
        }
    }

    pub fn set_string(&mut self, id: &str, value: String) -> Result<(), String> {
        match id {
            "custom_dlls" => {
                self.options.custom_dlls = value;
                Ok(())
            }
            _ => Err(format!("Unknown string option: {id}")),
        }
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
            latite_nightly: false,
            latite_debug: false,
            use_custom_dll: false,
            custom_dlls: String::new(),
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
            "latite_nightly" => Ok(&self.latite_nightly),
            "latite_debug" => Ok(&self.latite_debug),
            "use_custom_dll" => Ok(&self.use_custom_dll),
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
            "latite_nightly" => Ok(&mut self.latite_nightly),
            "latite_debug" => Ok(&mut self.latite_debug),
            "use_custom_dll" => Ok(&mut self.use_custom_dll),
            _ => Err(format!("Unknown option: {id}")),
        }
    }
}
