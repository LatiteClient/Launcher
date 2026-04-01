pub fn get_launcher_path() -> std::path::PathBuf {
    let appdata = std::env::var("LOCALAPPDATA").unwrap();
    let launcher_path = format!("{}\\Latite\\Launcher", appdata);

    std::fs::create_dir_all(&launcher_path).unwrap();
    std::fs::canonicalize(launcher_path).unwrap()
}

pub fn get_dlls_path() -> std::path::PathBuf {
    let path = get_launcher_path().join("DLLs");
    std::fs::create_dir_all(&path).unwrap();
    path
}

/// will not create file if doesn't exist
pub fn get_options_path() -> std::path::PathBuf {
    get_launcher_path().join("options.json")
}