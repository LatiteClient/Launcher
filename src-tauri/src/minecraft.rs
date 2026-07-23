use std::os::windows::process::CommandExt;
use std::{
    ffi::OsString, fs, os::windows::ffi::OsStringExt, path::PathBuf, process::Command,
    sync::LazyLock,
};

use windows::core::{w, HSTRING};
use windows::Management::Deployment::PackageManager;
use windows::Win32::System::Threading::CREATE_NO_WINDOW;
use windows::Win32::UI::Shell::{FOLDERID_System, SHGetKnownFolderPath, KF_FLAG_DEFAULT};

const POWERSHELL_COMMAND: &str = "Invoke-CommandInDesktopPackage -PackageFamilyName 'Microsoft.MinecraftUWP_8wekyb3d8bbwe' -AppId 'Game' -Command '{}'";

const PACKAGE_FAMILY_NAME: LazyLock<HSTRING> = LazyLock::new(|| unsafe {
    return w!("Microsoft.MinecraftUWP_8wekyb3d8bbwe").to_hstring();
});

const USER_SECURITY_ID: LazyLock<HSTRING> = LazyLock::new(|| unsafe {
    return w!("").to_hstring();
});

static POWERSHELL_EXECUTABLE: LazyLock<PathBuf> = LazyLock::new(|| unsafe {
    let folder = SHGetKnownFolderPath(&FOLDERID_System, KF_FLAG_DEFAULT, None);
    let system = OsString::from_wide(folder.unwrap().as_wide());
    return PathBuf::from(system).join("WindowsPowerShell\\v1.0\\powershell.exe");
});

static PACKAGE_MANAGER: LazyLock<PackageManager> = LazyLock::new(|| PackageManager::new().unwrap());

fn get_installed_minecraft_executable() -> Option<String> {
    let packages = match PACKAGE_MANAGER
        .FindPackagesByUserSecurityIdPackageFamilyName(&USER_SECURITY_ID, &PACKAGE_FAMILY_NAME)
    {
        Ok(value) => value,
        Err(_) => return None,
    };

    let package = match packages.into_iter().nth(0) {
        Some(value) => value,
        None => return None,
    };

    let installed_path = package.InstalledPath().unwrap().to_os_string();
    let minecraft = PathBuf::from(installed_path).join("Minecraft.Windows.exe");

    Some(minecraft.display().to_string())
}

pub fn launch_installed_minecraft_executable() -> Result<(), String> {
    let minecraft = match get_installed_minecraft_executable() {
        Some(value) => value,
        None => return Err("Minecraft isn't installed for the current user.".to_string()),
    };

    if !fs::exists(&minecraft).unwrap_or(false) {
        return Err("Minecraft's executable doesn't exist on disk.".to_string());
    }

    let powershell = POWERSHELL_EXECUTABLE.as_os_str();
    let mut process = Command::new(&powershell);

    process.creation_flags(CREATE_NO_WINDOW.0);

    process.arg("-NoProfile");
    process.arg("-NonInteractive");

    process.arg("-ExecutionPolicy");
    process.arg("Bypass");

    process.arg("-Command");
    process.arg(POWERSHELL_COMMAND.replace("{}", &minecraft));

    match process.spawn() {
        Ok(_) => Ok(()),
        Err(_) => Err("Failed to bootstrap or launch Minecraft.".to_string()),
    }
}
