//! Native Application, Folder, and File Search Service.
//!
//! Provides ultra-fast (<1ms) system application, folder, and file discovery
//! across macOS and Windows. Uses lazy caching for installed applications
//! to ensure instant zero-latency feedback during keystroke entry.

#[allow(unused_imports)]
use std::path::{Path, PathBuf};
use std::sync::RwLock;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchItemKind {
    Application,
    Folder,
    File,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchItem {
    pub name: String,
    pub path: String,
    pub kind: SearchItemKind,
    pub details: String,
}

/// Cached list of discovered system applications to avoid repeated disk scans while typing.
static APP_CACHE: RwLock<Option<(Instant, Vec<(String, String)>)>> = RwLock::new(None);
const CACHE_TTL: Duration = Duration::from_secs(60);

fn get_home_dir() -> PathBuf {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

/// Retrieve installed applications on the system (cached for 60 seconds).
pub fn get_installed_applications() -> Vec<(String, String)> {
    if let Ok(lock) = APP_CACHE.read() {
        if let Some((cached_at, ref apps)) = *lock {
            if cached_at.elapsed() < CACHE_TTL {
                return apps.clone();
            }
        }
    }

    let mut apps = Vec::new();

    #[cfg(target_os = "macos")]
    {
        let search_dirs = [
            PathBuf::from("/Applications"),
            PathBuf::from("/System/Applications"),
            PathBuf::from("/System/Applications/Utilities"),
            get_home_dir().join("Applications"),
        ];

        for dir in &search_dirs {
            scan_macos_apps(dir, &mut apps, 2);
        }
    }

    #[cfg(target_os = "windows")]
    {
        let mut search_dirs = Vec::new();

        if let Ok(appdata) = std::env::var("APPDATA") {
            search_dirs.push(PathBuf::from(appdata).join(r"Microsoft\Windows\Start Menu\Programs"));
        }
        if let Ok(programdata) = std::env::var("ProgramData") {
            search_dirs
                .push(PathBuf::from(programdata).join(r"Microsoft\Windows\Start Menu\Programs"));
        }
        if let Ok(localappdata) = std::env::var("LOCALAPPDATA") {
            search_dirs.push(PathBuf::from(localappdata).join(r"Programs"));
        }
        search_dirs.push(PathBuf::from(r"C:\Program Files"));
        search_dirs.push(PathBuf::from(r"C:\Program Files (x86)"));

        for dir in &search_dirs {
            scan_windows_apps(dir, &mut apps, 3);
        }

        // Add built-in common Windows system utilities
        let builtins = [
            ("Notepad", "notepad.exe"),
            ("Calculator", "calc.exe"),
            ("Command Prompt", "cmd.exe"),
            ("PowerShell", "powershell.exe"),
            ("File Explorer", "explorer.exe"),
            ("Task Manager", "taskmgr.exe"),
            ("Settings", "ms-settings:"),
        ];
        for (name, path) in builtins {
            if !apps.iter().any(|(n, _)| n.eq_ignore_ascii_case(name)) {
                apps.push((name.to_string(), path.to_string()));
            }
        }
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let desktop_dirs = [
            PathBuf::from("/usr/share/applications"),
            get_home_dir().join(".local/share/applications"),
        ];
        for dir in &desktop_dirs {
            if let Ok(entries) = std::fs::read_dir(dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.extension().and_then(|e| e.to_str()) == Some("desktop") {
                        let name = path
                            .file_stem()
                            .and_then(|s| s.to_str())
                            .unwrap_or("")
                            .to_string();
                        if !name.is_empty() {
                            apps.push((name, path.to_string_lossy().to_string()));
                        }
                    }
                }
            }
        }
    }

    // Sort alphabetically by name
    apps.sort_by(|a, b| a.0.to_lowercase().cmp(&b.0.to_lowercase()));
    apps.dedup_by(|a, b| a.0.eq_ignore_ascii_case(&b.0));

    if let Ok(mut lock) = APP_CACHE.write() {
        *lock = Some((Instant::now(), apps.clone()));
    }

    apps
}

#[cfg(target_os = "macos")]
fn scan_macos_apps(dir: &Path, apps: &mut Vec<(String, String)>, depth: usize) {
    if depth == 0 || !dir.exists() {
        return;
    }
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("app") {
                let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
                if !stem.is_empty() {
                    apps.push((stem.to_string(), path.to_string_lossy().to_string()));
                }
            } else if path.is_dir() && !path.is_symlink() {
                scan_macos_apps(&path, apps, depth - 1);
            }
        }
    }
}

#[cfg(target_os = "windows")]
fn scan_windows_apps(dir: &Path, apps: &mut Vec<(String, String)>, depth: usize) {
    if depth == 0 || !dir.exists() {
        return;
    }
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_lowercase();
            if ext == "lnk" || ext == "exe" {
                let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
                let lower = stem.to_lowercase();
                // Filter out uninstallers and helpers
                if !stem.is_empty()
                    && !lower.contains("uninstall")
                    && !lower.contains("helper")
                    && !lower.contains("crash")
                    && !lower.contains("update")
                {
                    apps.push((stem.to_string(), path.to_string_lossy().to_string()));
                }
            } else if path.is_dir() && !path.is_symlink() {
                scan_windows_apps(&path, apps, depth - 1);
            }
        }
    }
}

/// Search applications, common folders, and recently accessed / user files.
pub fn search_apps_and_files(query: &str) -> Vec<SearchItem> {
    let q = query.trim().to_lowercase();
    if q.is_empty() {
        return Vec::new();
    }

    let mut results: Vec<SearchItem> = Vec::new();

    // 1. Applications matching query
    let apps = get_installed_applications();
    for (name, path) in &apps {
        let name_lower = name.to_lowercase();
        if name_lower == q || name_lower.starts_with(&q) || name_lower.contains(&q) {
            results.push(SearchItem {
                name: name.clone(),
                path: path.clone(),
                kind: SearchItemKind::Application,
                details: "Application".to_string(),
            });
            if results.len() >= 6 {
                break;
            }
        }
    }

    // 2. Common User Folders matching query
    let home = get_home_dir();
    let standard_folders = [
        ("Downloads", home.join("Downloads")),
        ("Documents", home.join("Documents")),
        ("Desktop", home.join("Desktop")),
        ("Pictures", home.join("Pictures")),
        ("Music", home.join("Music")),
        ("Videos", home.join("Videos")),
        ("Movies", home.join("Movies")),
        ("Projects", home.join("Projects")),
        ("Developer", home.join("Developer")),
        ("Code", home.join("Code")),
        ("Home", home.clone()),
    ];

    for (name, folder_path) in standard_folders {
        if folder_path.exists() {
            let name_lower = name.to_lowercase();
            if name_lower == q || name_lower.starts_with(&q) || name_lower.contains(&q) {
                results.push(SearchItem {
                    name: name.to_string(),
                    path: folder_path.to_string_lossy().to_string(),
                    kind: SearchItemKind::Folder,
                    details: folder_path.to_string_lossy().to_string(),
                });
            }
        }
    }

    // 3. Search top-level subfolders & files in Desktop, Documents, Downloads
    let browse_roots = [
        home.join("Downloads"),
        home.join("Documents"),
        home.join("Desktop"),
    ];

    for root in &browse_roots {
        if let Ok(entries) = std::fs::read_dir(root) {
            for entry in entries.flatten() {
                let path = entry.path();
                let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                let file_lower = file_name.to_lowercase();

                // Skip hidden files
                if file_name.starts_with('.') {
                    continue;
                }

                if file_lower.contains(&q) {
                    let is_dir = path.is_dir();
                    results.push(SearchItem {
                        name: file_name.to_string(),
                        path: path.to_string_lossy().to_string(),
                        kind: if is_dir {
                            SearchItemKind::Folder
                        } else {
                            SearchItemKind::File
                        },
                        details: path.to_string_lossy().to_string(),
                    });
                }

                if results.len() >= 8 {
                    break;
                }
            }
        }
        if results.len() >= 8 {
            break;
        }
    }

    // Rank results: exact name match first, prefix next, then substring
    results.sort_by(|a, b| {
        let a_exact = a.name.eq_ignore_ascii_case(&q);
        let b_exact = b.name.eq_ignore_ascii_case(&q);
        if a_exact != b_exact {
            return b_exact.cmp(&a_exact);
        }

        let a_prefix = a.name.to_lowercase().starts_with(&q);
        let b_prefix = b.name.to_lowercase().starts_with(&q);
        if a_prefix != b_prefix {
            return b_prefix.cmp(&a_prefix);
        }

        // Applications before folders, folders before files
        let kind_weight = |k: &SearchItemKind| match k {
            SearchItemKind::Application => 0,
            SearchItemKind::Folder => 1,
            SearchItemKind::File => 2,
        };
        kind_weight(&a.kind).cmp(&kind_weight(&b.kind))
    });

    results.truncate(6);
    results
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_search_empty_query() {
        let results = search_apps_and_files("");
        assert!(results.is_empty());
        let results_spaces = search_apps_and_files("   ");
        assert!(results_spaces.is_empty());
    }

    #[test]
    fn test_get_installed_applications_not_empty() {
        let apps = get_installed_applications();
        assert!(
            !apps.is_empty(),
            "Should discover system applications or built-in tools"
        );
    }

    #[test]
    fn test_search_common_folder() {
        let results = search_apps_and_files("desktop");
        assert!(
            results
                .iter()
                .any(|r| r.name.eq_ignore_ascii_case("desktop")),
            "Should find Desktop folder"
        );
    }
}
