use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, LazyLock, Mutex},
};
use waybar_cffi::gtk::{
    gio::DesktopAppInfo,
    prelude::{AppInfoExt, IconExt, Cast, FileExt, IconThemeExt},
};

#[derive(Debug, Clone, Default)]
pub struct IconResolver(Arc<Mutex<HashMap<String, PathBuf>>>);

impl IconResolver {
    pub fn new() -> Self {
        Self::default()
    }

    #[tracing::instrument(level = "TRACE", ret)]
    pub fn resolve(&self, app_id: &str) -> Option<PathBuf> {
        let mut cache = self.0.lock().expect("icon resolver lock");

        if !cache.contains_key(app_id) {
            if let Some(path) = search_for_icon(app_id) {
                cache.insert(app_id.to_string(), path);
            }
        }

        cache.get(app_id).cloned()
    }
}

fn search_for_icon(app_id: &str) -> Option<PathBuf> {
    for directory in DATA_DIRECTORIES.iter() {
        for suffix in ["", ".desktop"] {
            let app_path = directory.join(format!("applications/{app_id}{suffix}"));
            if let Some(info) = DesktopAppInfo::from_filename(&app_path) {
                if let Some(path) = extract_icon_path(&info) {
                    return Some(path);
                }
            }
        }

        for prefix in ["applications/kde/", "applications/org.kde."] {
            for suffix in ["", ".desktop"] {
                let kde_path = directory.join(format!("{prefix}{app_id}{suffix}"));
                if let Some(info) = DesktopAppInfo::from_filename(&kde_path) {
                    if let Some(path) = extract_icon_path(&info) {
                        return Some(path);
                    }
                }
            }
        }
    }

    let search_results = DesktopAppInfo::search(app_id);
    for candidates in search_results.into_iter() {
        for candidate in candidates {
            if let Some(info) = DesktopAppInfo::new(&candidate) {
                if let Some(path) = extract_icon_path(&info) {
                    return Some(path);
                }
            }
        }
    }

    query_icon_theme(app_id)
}

fn query_icon_theme(icon_name: &str) -> Option<PathBuf> {
    use waybar_cffi::gtk::{IconTheme, IconLookupFlags};
    
    let icon_theme = IconTheme::default()?;
    
    let icon_info = icon_theme.lookup_icon(icon_name, 512, IconLookupFlags::empty())?;
    
    icon_info.filename()
}

fn extract_icon_path(info: &DesktopAppInfo) -> Option<PathBuf> {
    use waybar_cffi::gtk::gio::FileIcon;
    
    info.icon().and_then(|icon| {
        if let Some(file_icon) = icon.downcast_ref::<FileIcon>() {
            return file_icon.file().path();
        }

        IconExt::to_string(&icon)
            .and_then(|name| query_icon_theme(&name))
    })
}

static DATA_DIRECTORIES: LazyLock<Vec<PathBuf>> = LazyLock::new(|| {
    let mut directories = Vec::new();

    if let Ok(home) = std::env::var("HOME") {
        let home_path = PathBuf::from(home);
        directories.push(home_path.join(".local/share"));
        directories.push(home_path.join(".local/share/flatpak/exports/share"));
    }

    if let Ok(xdg_data) = std::env::var("XDG_DATA_DIRS") {
        directories.extend(xdg_data.split(':').map(PathBuf::from));
    } else {
        directories.extend([
            PathBuf::from("/usr/local/share"),
            PathBuf::from("/usr/share"),
        ]);
    }

    directories.push(PathBuf::from("/var/lib/flatpak/exports/share"));
    directories
});