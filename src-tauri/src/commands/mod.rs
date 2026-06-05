pub mod ai;
pub mod app;
pub mod backfill;
pub mod chat;
pub mod database;
pub mod debug;
pub mod engines;
pub mod folders;
pub mod jobs;
pub mod library;
pub mod process;
pub mod search;
pub mod updates;

use std::{path::Path, process::Command};

/// Open a file or folder with the OS default handler / file manager.
///
/// Used instead of the `opener` plugin's `open_path`, which is granted no path
/// scope in our capabilities and therefore rejects every path. Strips any
/// Windows `\\?\` verbatim prefix first, since `explorer.exe` mishandles those
/// (it opens the Documents library instead of the target).
pub(crate) fn open_in_file_manager(path: &Path) -> std::io::Result<()> {
    let path = dunce::simplified(path);
    #[cfg(windows)]
    {
        Command::new("explorer.exe").arg(path).spawn().map(|_| ())
    }
    #[cfg(target_os = "macos")]
    {
        Command::new("open").arg(path).spawn().map(|_| ())
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        Command::new("xdg-open").arg(path).spawn().map(|_| ())
    }
}
