use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize, Debug)]
struct Options {
    rpc_enabled: bool,
    rpc_show_server: bool,
    rpc_show_mc_version: bool,
    rpc_show_time_played: bool,

    misc_hide_on_close: bool,
    misc_close_after_injected: bool,
    last_used_version: Option<String>,
}

static mut OPTIONS: Options = Options {
    rpc_enabled: true,
    rpc_show_server: true,
    rpc_show_mc_version: true,
    rpc_show_time_played: true,

    misc_hide_on_close: false,
    misc_close_after_injected: false,
    last_used_version: None,
};

pub fn save_options() {
    let options_path = crate::paths::get_options_path();
    let options_file = std::fs::File::create(options_path).unwrap();
    serde_json::to_writer_pretty(options_file, unsafe { &OPTIONS }).unwrap();
}

pub fn load_options() {
    let options_path = crate::paths::get_options_path();

    println!("Loading options from: {}", options_path.to_str().unwrap());

    if !options_path.exists() {
        save_options();
        return;
    }

    let options_file = std::fs::File::open(options_path).unwrap();
    let opts : Options = serde_json::from_reader(options_file).unwrap();

    unsafe { OPTIONS = opts; }
}

pub fn update_last_used_version(version: &str) {
    unsafe { OPTIONS.last_used_version = Some(version.to_string()); }
    save_options();
}

pub fn get_last_used_version() -> Option<String> {
    unsafe { OPTIONS.last_used_version.clone() }
}

pub fn update_bool_option(id: &str, value: bool) {
    unsafe {
        match id {
            "rpc_enabled" => OPTIONS.rpc_enabled = value,
            "rpc_show_server" => OPTIONS.rpc_show_server = value,
            "rpc_show_mc_version" => OPTIONS.rpc_show_mc_version = value,
            "rpc_show_time_played" => OPTIONS.rpc_show_time_played = value,
            "misc_hide_on_close" => OPTIONS.misc_hide_on_close = value,
            "misc_close_after_injected" => OPTIONS.misc_close_after_injected = value,
            _ => println!("Unknown option: {}", id),
        }
    }

    save_options();
}

pub fn get_bool_option(id: &str) -> bool {
    unsafe {
        match id {
            "rpc_enabled" => OPTIONS.rpc_enabled,
            "rpc_show_server" => OPTIONS.rpc_show_server,
            "rpc_show_mc_version" => OPTIONS.rpc_show_mc_version,
            "rpc_show_time_played" => OPTIONS.rpc_show_time_played,
            "misc_hide_on_close" => OPTIONS.misc_hide_on_close,
            "misc_close_after_injected" => OPTIONS.misc_close_after_injected,
            _ => {
                println!("Unknown option: {}", id);
                false
            },
        }
    }
}