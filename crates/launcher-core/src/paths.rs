//! On-disk layout. Mirrors the vanilla launcher layout (versions/<id>/<id>.json etc.).
//! Never uses the player's `.minecraft`.
//!
//! <root>/
//!   libraries/  versions/  assets/  runtimes/   shared game files
//!   instance/                                    Release game directory (mods, config, saves…)
//!   instance-beta/  instance-alpha/              test channels' game directories
//!   cache/                                       installers and temp downloads
//!   settings.json  account.json

use crate::channel::Channel;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct Paths {
    pub root: PathBuf,
}

impl Paths {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }
    pub fn libraries(&self) -> PathBuf {
        self.root.join("libraries")
    }
    pub fn versions(&self) -> PathBuf {
        self.root.join("versions")
    }
    pub fn version_dir(&self, id: &str) -> PathBuf {
        self.versions().join(id)
    }
    pub fn version_json(&self, id: &str) -> PathBuf {
        self.version_dir(id).join(format!("{id}.json"))
    }
    pub fn version_jar(&self, id: &str) -> PathBuf {
        self.version_dir(id).join(format!("{id}.jar"))
    }
    pub fn natives(&self, id: &str) -> PathBuf {
        self.version_dir(id).join("natives")
    }
    pub fn assets(&self) -> PathBuf {
        self.root.join("assets")
    }
    pub fn runtimes(&self) -> PathBuf {
        self.root.join("runtimes")
    }
    /// The game directory for a channel: `instance` (Release), `instance-beta`, `instance-alpha`.
    pub fn instance(&self, channel: Channel) -> PathBuf {
        self.root.join(channel.instance_dir())
    }
    pub fn cache(&self) -> PathBuf {
        self.root.join("cache")
    }
    pub fn settings(&self) -> PathBuf {
        self.root.join("settings.json")
    }
    pub fn account(&self) -> PathBuf {
        self.root.join("account.json")
    }
    /// Signed-in accounts (public profiles) and the active one.
    pub fn accounts(&self) -> PathBuf {
        self.root.join("accounts.json")
    }
}

/// Join a relative path from an untrusted manifest onto `base`, rejecting
/// absolute paths, `..`, drive prefixes and anything else that could escape.
pub fn safe_join(base: &Path, rel: &str) -> crate::Result<PathBuf> {
    use std::path::Component;
    if rel.is_empty() || rel.contains('\\') || rel.contains(':') {
        return Err(crate::Error::UnsafePath(rel.to_string()));
    }
    let mut out = base.to_path_buf();
    for c in Path::new(rel).components() {
        match c {
            Component::Normal(p) => out.push(p),
            _ => return Err(crate::Error::UnsafePath(rel.to_string())),
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn safe_join_accepts_normal_paths() {
        let base = Path::new("/base");
        assert_eq!(
            safe_join(base, "mods/a.jar").unwrap(),
            base.join("mods").join("a.jar")
        );
    }

    #[test]
    fn safe_join_rejects_escapes() {
        let base = Path::new("/base");
        for bad in ["../x", "mods/../../x", "/etc/passwd", "C:/x", "a\\b", ""] {
            assert!(safe_join(base, bad).is_err(), "{bad} should be rejected");
        }
    }
}
