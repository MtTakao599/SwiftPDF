//! Where this process actually keeps settings, logs and chairman-seal files.
//!
//! The path is whatever [`crate::portable`] and the desktop `settings_dir` already chose.
//! This module only describes that choice. It does not move folders or change the store.

use std::path::{Path, PathBuf};

use crate::portable::{self, Portable};

/// How the process found the folder it is using.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DataStoreMode {
    /// `%LOCALAPPDATA%\SwiftPDF` (or the platform equivalent). No portable marker.
    Regular,
    /// `Data` beside the executable; the marker was writable.
    Portable,
    /// A portable marker was found, but `Data` could not be written, so the regular folder is used.
    PortableFallback,
}

/// The folder this process is using, and why.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DataStoreInfo {
    pub path: PathBuf,
    pub mode: DataStoreMode,
    /// Operating-system text when portable `Data` could not be written. Not a user path.
    pub fallback_reason: Option<String>,
}

impl DataStoreInfo {
    /// Build from the folder already chosen for settings and the portable probe.
    ///
    /// `settings` is the directory `settings_dir()` returned (portable `Data` or the user folder).
    pub fn from_resolved(settings: PathBuf, portable: &Portable) -> Self {
        if portable.dir.as_ref().is_some_and(|dir| dir == &settings) {
            return Self { path: settings, mode: DataStoreMode::Portable, fallback_reason: None };
        }
        if let Some(unwritable) = &portable.unwritable {
            return Self { path: settings, mode: DataStoreMode::PortableFallback, fallback_reason: Some(unwritable.error.clone()) };
        }
        Self { path: settings, mode: DataStoreMode::Regular, fallback_reason: None }
    }

    /// Japanese or English line for About and the fallback toast.
    pub fn mode_label(&self) -> String {
        match self.mode {
            DataStoreMode::Regular => crate::identity::localized("通常（この Windows ユーザーのフォルダー）", "Regular (this Windows user's folder)"),
            DataStoreMode::Portable => crate::identity::localized("ポータブル（実行ファイル隣の Data）", "Portable (Data beside the executable)"),
            DataStoreMode::PortableFallback => crate::identity::localized(
                "ポータブルの Data が使えず、通常のフォルダーを使用中",
                "Portable Data was not writable; using the regular folder",
            ),
        }
    }

    pub fn fallback_notice(&self) -> Option<String> {
        if self.mode != DataStoreMode::PortableFallback {
            return None;
        }
        let reason = self.fallback_reason.as_deref().unwrap_or("");
        Some(crate::identity::localized(
            &format!(
                "ポータブル用の Data に書き込めなかったため、印影と設定は次の場所に保存しています。保存先を確認してください。\n{}\n（理由: {reason}）",
                self.path.display()
            ),
            &format!(
                "Portable Data could not be written, so seals and settings are in this folder. Check the location.\n{}\n(reason: {reason})",
                self.path.display()
            ),
        ))
    }
}

/// Probe portable mode for this executable and describe `settings`.
pub fn describe(settings: PathBuf) -> DataStoreInfo {
    DataStoreInfo::from_resolved(settings, portable::current())
}

/// Same as [`describe`], for tests that must not touch the real executable directory.
pub fn describe_with(settings: PathBuf, portable: &Portable) -> DataStoreInfo {
    DataStoreInfo::from_resolved(settings, portable)
}

pub fn path_is(info: &DataStoreInfo, expected: &Path) -> bool {
    info.path == expected
}

impl crate::PdfCraftApp {
    /// Record the folder already chosen for settings and seals. Display only.
    pub fn set_data_store(&mut self, info: DataStoreInfo) {
        if let Some(notice) = info.fallback_notice() {
            self.notify(notice);
        }
        self.data_store = Some(info);
    }

    /// The path About and tests should show: the described folder, else the seal store folder.
    pub fn displayed_data_path(&self) -> Option<&Path> {
        self.data_store.as_ref().map(|info| info.path.as_path()).or_else(|| self.seals.store_dir())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::portable::{DATA_DIR, Unwritable};

    #[test]
    fn a_regular_folder_is_labelled_regular() {
        let path = PathBuf::from("C:\\Users\\example\\AppData\\Local\\SwiftPDF");
        let info = describe_with(path.clone(), &Portable::default());
        assert_eq!(info.mode, DataStoreMode::Regular);
        assert_eq!(info.path, path);
        assert!(info.fallback_reason.is_none());
        assert!(info.fallback_notice().is_none());
    }

    #[test]
    fn a_writable_data_folder_is_portable() {
        let path = PathBuf::from("D:\\tools\\SwiftPDF\\Data");
        let portable = Portable { dir: Some(path.clone()), unwritable: None };
        let info = describe_with(path.clone(), &portable);
        assert_eq!(info.mode, DataStoreMode::Portable);
        assert_eq!(info.path, path);
        assert!(path_is(&info, Path::new("D:\\tools\\SwiftPDF\\Data")));
    }

    #[test]
    fn an_unwritable_data_folder_falls_back_and_keeps_the_reason() {
        let user = PathBuf::from("C:\\Users\\example\\AppData\\Local\\SwiftPDF");
        let portable = Portable {
            dir: None,
            unwritable: Some(Unwritable {
                marker: PathBuf::from("E:\\stick\\portable.txt"),
                folder: PathBuf::from("E:\\stick").join(DATA_DIR),
                error: "Access is denied. (os error 5)".into(),
            }),
        };
        let info = describe_with(user.clone(), &portable);
        assert_eq!(info.mode, DataStoreMode::PortableFallback);
        assert_eq!(info.path, user);
        assert_eq!(info.fallback_reason.as_deref(), Some("Access is denied. (os error 5)"));
        let notice = info.fallback_notice().expect("fallback is explained");
        assert!(notice.contains("C:\\Users\\example\\AppData\\Local\\SwiftPDF"), "{notice}");
        assert!(notice.contains("os error 5"), "{notice}");
    }

    /// From the real probe: `Data` blocked by a file, so the folder the caller fell back to is
    /// shown as a fallback with the probe's own error, and set on the app with a warning.
    #[test]
    fn a_probe_failure_is_shown_as_a_fallback() {
        let exe = std::env::temp_dir().join(format!("swiftpdf-datastore-fallback-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&exe);
        std::fs::create_dir_all(&exe).unwrap();
        std::fs::write(exe.join("portable.txt"), b"").unwrap();
        std::fs::write(exe.join(DATA_DIR), b"in the way").unwrap();
        let portable = portable::resolve(Some(&exe));
        let regular = exe.join("regular-stand-in");
        let info = describe_with(regular.clone(), &portable);
        assert_eq!(info.mode, DataStoreMode::PortableFallback);
        assert_eq!(info.path, regular);
        let reason = portable.unwritable.as_ref().unwrap().error.clone();
        assert_eq!(info.fallback_reason.as_deref(), Some(reason.as_str()));
        let mut app = crate::PdfCraftApp::new();
        app.set_data_store(info);
        let toast = app.toast.as_ref().map(|(text, _)| text.clone()).unwrap_or_default();
        assert!(toast.contains(&regular.display().to_string()) && toast.contains(&reason), "{toast}");
        assert_eq!(app.displayed_data_path(), Some(regular.as_path()));
        let _ = std::fs::remove_dir_all(&exe);
    }
}
