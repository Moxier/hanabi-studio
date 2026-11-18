//! Links shown on the welcome screen, and opening web pages or folders with the system.

/// Support page.
pub const KOFI: &str = "https://ko-fi.com/ccygnus";
/// Source code page (GitHub and "Report a problem" buttons). If emptied, those buttons show
/// "coming soon".
pub const GITHUB: &str = "https://github.com/Moxier/hanabi-studio";

pub fn github_issues() -> Option<String> {
    (!GITHUB.is_empty()).then(|| format!("{}/issues", GITHUB.trim_end_matches('/')))
}

/// Open a web page or a file/folder with the system's default program.
pub fn open_external(target: &std::ffi::OsStr) {
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        // explorer.exe opens files, folders and web links with their default program without
        // going through a command shell (cmd /C start would run anything after an "&" in a name).
        let _ = std::process::Command::new("explorer.exe").arg(target).creation_flags(0x0800_0000).spawn();
    }
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("open").arg(target).spawn();
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let _ = std::process::Command::new("xdg-open").arg(target).spawn();
    }
}

pub fn open_url(url: &str) {
    // Only web links are opened this way.
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        return;
    }
    open_external(std::ffi::OsStr::new(url));
}
