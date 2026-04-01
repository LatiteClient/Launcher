use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize, Debug)]
struct Options {
    rpc_enabled: bool,
    rpc_show_server: bool,
    rpc_show_mc_version: bool,
    rpc_show_time_played: bool,

    misc_hide_system_tray: bool,
    misc_close_after_injected: bool,
}

static mut OPTIONS: Options = Options {
    rpc_enabled: true,
    rpc_show_server: true,
    rpc_show_mc_version: true,
    rpc_show_time_played: true,

    misc_hide_system_tray: false,
    misc_close_after_injected: false,
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
