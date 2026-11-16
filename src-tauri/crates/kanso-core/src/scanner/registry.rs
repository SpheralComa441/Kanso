//! Thin, OS-independent view of the Windows Registry for the store scanners.
//!
//! Scanners never touch the registry themselves: they receive plain [`RegistryEntry`] values,
//! so all of their logic runs (and is unit-tested) on any OS with mocked entries.

use std::collections::HashMap;

/// One registry subkey together with its string values.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RegistryEntry {
    pub subkey_name: String,
    /// Value names are lower-cased: registry value names are case-insensitive.
    values: HashMap<String, String>,
}

impl RegistryEntry {
    pub fn new(subkey_name: &str, values: &[(&str, &str)]) -> Self {
        Self {
            subkey_name: subkey_name.to_string(),
            values: values
                .iter()
                .map(|(name, value)| (name.to_ascii_lowercase(), value.to_string()))
                .collect(),
        }
    }

    /// Trimmed string value, or `""` when the value is missing or not a string.
    pub fn get(&self, value_name: &str) -> &str {
        self.values
            .get(&value_name.to_ascii_lowercase())
            .map(|v| v.trim())
            .unwrap_or_default()
    }
}

/// Registry root a path is read from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegistryHive {
    LocalMachine,
    CurrentUser,
}

/// `Uninstall` keys where installed programs register themselves: 32-bit and 64-bit views of
/// `HKLM`, plus per-user installs in `HKCU`.
const UNINSTALL_KEYS: &[(RegistryHive, &str)] = &[
    (
        RegistryHive::LocalMachine,
        r"SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall",
    ),
    (
        RegistryHive::LocalMachine,
        r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall",
    ),
    (
        RegistryHive::CurrentUser,
        r"Software\Microsoft\Windows\CurrentVersion\Uninstall",
    ),
];

/// Every program registered in the Windows "installed apps" list.
pub fn read_uninstall_entries() -> Vec<RegistryEntry> {
    UNINSTALL_KEYS
        .iter()
        .flat_map(|(hive, path)| read_registry_subkeys(*hive, path))
        .collect()
}

/// Reads all direct subkeys of `path` with their string values. Missing keys yield nothing.
#[cfg(target_os = "windows")]
pub fn read_registry_subkeys(hive: RegistryHive, path: &str) -> Vec<RegistryEntry> {
    use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};
    use winreg::RegKey;

    let root = RegKey::predef(match hive {
        RegistryHive::LocalMachine => HKEY_LOCAL_MACHINE,
        RegistryHive::CurrentUser => HKEY_CURRENT_USER,
    });
    let Ok(parent) = root.open_subkey(path) else {
        return Vec::new();
    };

    let mut entries = Vec::new();
    for subkey_name in parent.enum_keys().flatten() {
        let Ok(subkey) = parent.open_subkey(&subkey_name) else {
            continue;
        };
        let mut values = HashMap::new();
        for (value_name, _) in subkey.enum_values().flatten() {
            // Non-string values (DWORD sizes, dates...) are of no use to the scanners
            if let Ok(text) = subkey.get_value::<String, _>(&value_name) {
                values.insert(value_name.to_ascii_lowercase(), text);
            }
        }
        entries.push(RegistryEntry {
            subkey_name,
            values,
        });
    }
    entries
}

#[cfg(not(target_os = "windows"))]
pub fn read_registry_subkeys(_hive: RegistryHive, _path: &str) -> Vec<RegistryEntry> {
    Vec::new()
}

/// The names of the values directly under `path`, whatever they hold. A missing key yields
/// nothing.
#[cfg(target_os = "windows")]
pub fn read_registry_value_names(hive: RegistryHive, path: &str) -> Vec<String> {
    use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};
    use winreg::RegKey;

    let root = RegKey::predef(match hive {
        RegistryHive::LocalMachine => HKEY_LOCAL_MACHINE,
        RegistryHive::CurrentUser => HKEY_CURRENT_USER,
    });
    root.open_subkey(path)
        .map(|key| key.enum_values().flatten().map(|(name, _)| name).collect())
        .unwrap_or_default()
}

#[cfg(not(target_os = "windows"))]
pub fn read_registry_value_names(_hive: RegistryHive, _path: &str) -> Vec<String> {
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn value_lookup_is_case_insensitive_and_trimmed() {
        let entry = RegistryEntry::new("1207658924", &[("gameName", "  The Witcher 3  ")]);
        assert_eq!(entry.get("GAMENAME"), "The Witcher 3");
        assert_eq!(entry.get("missing"), "");
    }
}
