//! Where plugins come from: the ones shipped with Kanso and the user's plugins folder.
//!
//! A plugin is a folder with two files, `plugin.json` and `main.js`. Installing one copies
//! exactly those two files into `<app data>/plugins/<id>/`; nothing else in the source folder
//! is ever read. A user plugin with the id of a shipped one replaces it, which is also how a
//! shipped plugin can be updated without waiting for a new version of Kanso.

use super::manifest::{PluginManifest, ENTRY_FILE, MANIFEST_FILE};
use crate::config::is_valid_plugin_id;
use crate::i18n::T;
use std::fs;
use std::path::{Path, PathBuf};

const MAX_MANIFEST_BYTES: u64 = 64 * 1024;
const MAX_CODE_BYTES: u64 = 512 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PluginSource {
    /// Shipped inside Kanso.
    Bundled,
    /// Installed in the user's plugins folder.
    User,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plugin {
    pub manifest: PluginManifest,
    pub code: String,
    pub source: PluginSource,
}

macro_rules! bundled {
    ($id:literal) => {
        (
            include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../../plugins/", $id, "/plugin.json")),
            include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../../plugins/", $id, "/main.js")),
        )
    };
}

/// The official plugins, from the repository's `plugins/` folder, in the order they are listed.
const BUNDLED: [(&str, &str); 8] = [
    bundled!("howlongtobeat"),
    bundled!("steam-reviews"),
    bundled!("steam-news"),
    bundled!("pcgamingwiki"),
    bundled!("nexus-mods"),
    bundled!("price-history"),
    bundled!("play-tonight"),
    bundled!("daily-pick"),
];

/// The plugins shipped with Kanso.
pub fn bundled_plugins() -> Vec<Plugin> {
    BUNDLED
        .iter()
        .filter_map(|(manifest, code)| {
            Some(Plugin {
                manifest: PluginManifest::parse(manifest).ok()?,
                code: (*code).to_string(),
                source: PluginSource::Bundled,
            })
        })
        .collect()
}

/// Every available plugin: the shipped ones (unless replaced) followed by the user's, by name.
/// The second value lists the folders that could not be loaded, one message each.
pub fn load_plugins(user_dir: &Path) -> (Vec<Plugin>, Vec<String>) {
    let mut warnings = Vec::new();
    let mut user_plugins: Vec<Plugin> = Vec::new();

    let mut folders: Vec<PathBuf> = fs::read_dir(user_dir)
        .map(|entries| entries.flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect())
        .unwrap_or_default();
    folders.sort();
    for folder in folders {
        match read_plugin_folder(&folder) {
            Ok(plugin) => {
                if user_plugins.iter().any(|p| p.manifest.id == plugin.manifest.id) {
                    warnings.push(format!(
                        "{} ({}): duplicate id \"{}\"",
                        T::PluginInvalid.s(),
                        folder.display(),
                        plugin.manifest.id
                    ));
                } else {
                    user_plugins.push(plugin);
                }
            }
            Err(error) => warnings.push(format!("{} ({}): {error}", T::PluginInvalid.s(), folder.display())),
        }
    }
    user_plugins.sort_by_key(|plugin| plugin.manifest.id.clone());

    let mut plugins: Vec<Plugin> = bundled_plugins()
        .into_iter()
        .map(|bundled| {
            match user_plugins.iter().position(|p| p.manifest.id == bundled.manifest.id) {
                Some(index) => user_plugins.remove(index),
                None => bundled,
            }
        })
        .collect();
    plugins.extend(user_plugins);
    (plugins, warnings)
}

/// The plugin with this id, if available.
pub fn find_plugin(user_dir: &Path, id: &str) -> Option<Plugin> {
    load_plugins(user_dir)
        .0
        .into_iter()
        .find(|plugin| plugin.manifest.id == id)
}

/// Reads a plugin from its folder (or from the path of the `plugin.json` inside it).
pub fn read_plugin_folder(path: &Path) -> Result<Plugin, String> {
    let folder = if path.is_file() {
        path.parent().unwrap_or(path)
    } else {
        path
    };
    let manifest_raw = read_limited(&folder.join(MANIFEST_FILE), MAX_MANIFEST_BYTES)?;
    let manifest = PluginManifest::parse(&manifest_raw)?;
    let code = read_limited(&folder.join(ENTRY_FILE), MAX_CODE_BYTES)?;
    Ok(Plugin {
        manifest,
        code,
        source: PluginSource::User,
    })
}

fn read_limited(path: &Path, max_bytes: u64) -> Result<String, String> {
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    let size = fs::metadata(path)
        .map_err(|_| format!("{name} not found"))?
        .len();
    if size > max_bytes {
        return Err(format!("{name} is larger than {} kB", max_bytes / 1024));
    }
    fs::read_to_string(path).map_err(|e| format!("{name}: {e}"))
}

/// Installs (or updates) the plugin found at `source` into the user's plugins folder.
pub fn install_plugin(user_dir: &Path, source: &Path) -> Result<Plugin, String> {
    let plugin = read_plugin_folder(source)
        .map_err(|error| format!("{}: {error}", T::PluginInvalid.s()))?;
    let target = user_dir.join(&plugin.manifest.id);
    let folder = if source.is_file() {
        source.parent().unwrap_or(source)
    } else {
        source
    };

    let io_error = |e: std::io::Error| format!("{} {}: {e}", T::CannotSave.s(), target.display());
    fs::create_dir_all(&target).map_err(io_error)?;
    // Installing from the installed copy itself (a plugin being developed in place) is a no-op
    let same_folder = fs::canonicalize(folder).ok() == fs::canonicalize(&target).ok();
    if !same_folder {
        for file in [MANIFEST_FILE, ENTRY_FILE] {
            fs::copy(folder.join(file), target.join(file)).map_err(io_error)?;
        }
    }
    Ok(plugin)
}

/// Removes a user plugin. A shipped plugin that it replaced becomes available again.
pub fn remove_plugin(user_dir: &Path, id: &str) -> Result<(), String> {
    if !is_valid_plugin_id(id) {
        return Err(format!("{}: {id}", T::PluginNotFound.s()));
    }
    // Folders are named after the id on install, but one dropped in by hand may not be
    let folder = fs::read_dir(user_dir)
        .ok()
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .find(|path| read_plugin_folder(path).is_ok_and(|plugin| plugin.manifest.id == id))
        .ok_or_else(|| format!("{}: {id}", T::PluginNotFound.s()))?;
    fs::remove_dir_all(&folder)
        .map_err(|e| format!("{} {}: {e}", T::CannotSave.s(), folder.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn write_plugin(folder: &Path, id: &str, version: &str) {
        fs::create_dir_all(folder).unwrap();
        fs::write(
            folder.join(MANIFEST_FILE),
            format!(
                r#"{{"kansoPlugin": 1, "id": "{id}", "name": "Test {id}", "version": "{version}",
                    "provides": {{"gameDetails": true}}}}"#
            ),
        )
        .unwrap();
        fs::write(folder.join(ENTRY_FILE), "kanso.plugin({ gameDetails: () => null });").unwrap();
    }

    #[test]
    fn the_official_plugins_are_all_valid() {
        let plugins = bundled_plugins();
        assert_eq!(plugins.len(), BUNDLED.len(), "a shipped plugin.json does not parse");
        let ids: Vec<&str> = plugins.iter().map(|p| p.manifest.id.as_str()).collect();
        assert_eq!(
            ids,
            [
                "howlongtobeat",
                "steam-reviews",
                "steam-news",
                "pcgamingwiki",
                "nexus-mods",
                "price-history",
                "play-tonight",
                "daily-pick"
            ]
        );
        for plugin in &plugins {
            assert_eq!(plugin.source, PluginSource::Bundled);
            assert!(!plugin.manifest.description.is_empty(), "{}", plugin.manifest.id);
            assert!(plugin.manifest.author.is_some(), "{}", plugin.manifest.id);
        }
        // Third-party services are contacted only once the user switches the plugin on
        for plugin in &plugins {
            let only_steam = plugin
                .manifest
                .hosts
                .iter()
                .all(|host| host.ends_with("steampowered.com"));
            assert!(
                !plugin.manifest.enabled_by_default || only_steam,
                "{} is on by default but contacts {:?}",
                plugin.manifest.id,
                plugin.manifest.hosts
            );
        }
        // Nobody's Home page gains a part unless they switch its plugin on
        for plugin in &plugins {
            assert!(
                !(plugin.manifest.home && plugin.manifest.enabled_by_default),
                "{} adds a part to Home and is on by default",
                plugin.manifest.id
            );
        }
    }

    #[test]
    fn installs_lists_replaces_and_removes_user_plugins() {
        let data = tempdir().unwrap();
        let user_dir = data.path().join("plugins");
        let downloads = tempdir().unwrap();

        // Nothing installed: only the shipped plugins
        let (plugins, warnings) = load_plugins(&user_dir);
        assert_eq!(plugins.len(), BUNDLED.len());
        assert!(warnings.is_empty());

        // Install from the folder, or from the plugin.json picked in a file dialog
        let source = downloads.path().join("my-plugin-main");
        write_plugin(&source, "my-plugin", "1.0.0");
        fs::write(source.join("README.md"), "not copied").unwrap();
        let installed = install_plugin(&user_dir, &source.join(MANIFEST_FILE)).expect("installs");
        assert_eq!(installed.manifest.id, "my-plugin");
        assert!(user_dir.join("my-plugin").join(ENTRY_FILE).is_file());
        assert!(!user_dir.join("my-plugin").join("README.md").exists(), "only the two files are taken");

        let (plugins, _) = load_plugins(&user_dir);
        assert_eq!(plugins.len(), BUNDLED.len() + 1);
        assert_eq!(plugins.last().unwrap().source, PluginSource::User);
        assert_eq!(find_plugin(&user_dir, "my-plugin").unwrap().manifest.version, "1.0.0");

        // Installing again updates in place
        write_plugin(&source, "my-plugin", "1.1.0");
        install_plugin(&user_dir, &source).expect("updates");
        assert_eq!(find_plugin(&user_dir, "my-plugin").unwrap().manifest.version, "1.1.0");

        // A user plugin with a shipped id takes its place, and gives it back when removed
        let fork = downloads.path().join("fork");
        write_plugin(&fork, "steam-news", "9.9.9");
        install_plugin(&user_dir, &fork).expect("installs");
        let (plugins, _) = load_plugins(&user_dir);
        assert_eq!(plugins.len(), BUNDLED.len() + 1, "replaced, not added");
        let news = find_plugin(&user_dir, "steam-news").unwrap();
        assert_eq!((news.source, news.manifest.version.as_str()), (PluginSource::User, "9.9.9"));
        remove_plugin(&user_dir, "steam-news").expect("removes");
        assert_eq!(find_plugin(&user_dir, "steam-news").unwrap().source, PluginSource::Bundled);

        // Broken folders are reported, not fatal
        fs::create_dir_all(user_dir.join("broken")).unwrap();
        fs::write(user_dir.join("broken").join(MANIFEST_FILE), "{").unwrap();
        fs::create_dir_all(user_dir.join("no-code")).unwrap();
        fs::write(
            user_dir.join("no-code").join(MANIFEST_FILE),
            r#"{"kansoPlugin": 1, "id": "no-code", "name": "x", "version": "1", "provides": {"gameDetails": true}}"#,
        )
        .unwrap();
        let (plugins, warnings) = load_plugins(&user_dir);
        assert_eq!(plugins.len(), BUNDLED.len() + 1);
        assert_eq!(warnings.len(), 2, "{warnings:?}");
        assert!(warnings.iter().any(|w| w.contains("main.js not found")));

        remove_plugin(&user_dir, "my-plugin").expect("removes");
        assert!(find_plugin(&user_dir, "my-plugin").is_none());
        assert!(remove_plugin(&user_dir, "my-plugin").is_err());
        assert!(remove_plugin(&user_dir, "../plugins").is_err());
        assert!(remove_plugin(&user_dir, "play-tonight").is_err(), "shipped plugins cannot be removed");
        assert!(install_plugin(&user_dir, downloads.path()).is_err(), "no plugin.json there");
    }
}
