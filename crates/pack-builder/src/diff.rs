//! What changed between the live pack and a freshly built one, and the SemVer
//! bump that implies (removed mods → MAJOR, added mods → MINOR, otherwise PATCH).

use launcher_core::pack::{PackFile, PackManifest};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Bump {
    Patch,
    Minor,
    Major,
}

#[derive(Debug, Default)]
pub struct Diff {
    pub mods_added: Vec<String>,
    pub mods_removed: Vec<String>,
    /// (old file, new file); the same file name when only its contents changed.
    pub mods_updated: Vec<(String, String)>,
    pub files_added: Vec<String>,
    pub files_removed: Vec<String>,
    pub files_changed: Vec<String>,
    pub minecraft: Option<(String, String)>,
    pub loader: Option<(String, String)>,
    /// Recommended memory (MiB) before → after.
    pub memory: Option<(Option<u32>, Option<u32>)>,
    /// Optional features / presets, or which files belong to them, changed.
    pub features_changed: bool,
}

impl Diff {
    pub fn is_empty(&self) -> bool {
        self.mods_added.is_empty()
            && self.mods_removed.is_empty()
            && self.mods_updated.is_empty()
            && self.files_added.is_empty()
            && self.files_removed.is_empty()
            && self.files_changed.is_empty()
            && self.minecraft.is_none()
            && self.loader.is_none()
            && self.memory.is_none()
            && !self.features_changed
    }

    pub fn bump(&self) -> Bump {
        if self.minecraft.is_some() || !self.mods_removed.is_empty() {
            Bump::Major
        } else if !self.mods_added.is_empty() {
            Bump::Minor
        } else {
            Bump::Patch
        }
    }
}

/// A player-friendly name for a mod or pack file: the file name without its extension.
pub fn display_name(path: &str) -> String {
    let name = file_name(path);
    name.strip_suffix(".jar")
        .or_else(|| name.strip_suffix(".zip"))
        .unwrap_or(name)
        .to_string()
}

impl Diff {
    /// The "what's new" block written into `pack.json` for the launcher to show.
    pub fn changes(
        &self,
        from: Option<semver::Version>,
        notes: Option<String>,
    ) -> launcher_core::pack::PackChanges {
        let names = |v: &[String]| v.iter().map(|p| display_name(p)).collect::<Vec<_>>();
        launcher_core::pack::PackChanges {
            from,
            notes: notes.filter(|n| !n.trim().is_empty()),
            added: names(&self.mods_added),
            removed: names(&self.mods_removed),
            updated: self
                .mods_updated
                .iter()
                .map(|(_, new)| display_name(new))
                .collect(),
            other_files: (self.files_added.len()
                + self.files_removed.len()
                + self.files_changed.len()) as u32,
        }
    }
}

pub fn next_version(live: &semver::Version, bump: Bump) -> semver::Version {
    match bump {
        Bump::Major => semver::Version::new(live.major + 1, 0, 0),
        Bump::Minor => semver::Version::new(live.major, live.minor + 1, 0),
        Bump::Patch => semver::Version::new(live.major, live.minor, live.patch + 1),
    }
}

/// Mods and content packs (resource/shader packs): adding one is a feature,
/// removing one can break things players rely on.
fn is_mod(f: &PackFile) -> bool {
    let content = ["mods/", "resourcepacks/", "shaderpacks/"];
    content.iter().any(|p| f.path.starts_with(p))
        && (f.path.ends_with(".jar") || f.path.ends_with(".zip"))
}

fn file_name(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

/// A file name with versions and loader tags removed, so
/// `sophisticatedbackpacks-1.21.1-3.26.21.2249.jar` → `sophisticatedbackpacks`.
pub fn mod_name_key(path: &str) -> String {
    const NOISE: &[&str] = &[
        "neoforge",
        "forge",
        "fabric",
        "quilt",
        "mc",
        "all",
        "universal",
    ];
    let name = file_name(path);
    let stem = name
        .strip_suffix(".jar")
        .or_else(|| name.strip_suffix(".zip"))
        .unwrap_or(name)
        .to_lowercase();
    stem.split(['-', '_', ' ', '+'])
        .filter(|t| !t.is_empty())
        .filter(|t| !t.starts_with(|c: char| c.is_ascii_digit()))
        .filter(|t| {
            // Version-like tokens: v1.2, ver1.2, mc1.21.10 (Fabric), r5.7.1 (shader packs).
            let rest = ["ver", "mc", "v", "r"]
                .iter()
                .find_map(|p| t.strip_prefix(p));
            !rest.is_some_and(|r| r.starts_with(|c: char| c.is_ascii_digit()))
        })
        .filter(|t| !NOISE.contains(t))
        .collect::<Vec<_>>()
        .join("-")
}

fn same_mod(a: &PackFile, b: &PackFile) -> bool {
    match (&a.mod_id, &b.mod_id) {
        (Some(x), Some(y)) => x == y,
        _ => {
            let k = mod_name_key(&a.path);
            !k.is_empty() && k == mod_name_key(&b.path)
        }
    }
}

pub fn diff(old: &PackManifest, new: &PackManifest) -> Diff {
    let mut d = Diff::default();
    if old.minecraft != new.minecraft {
        d.minecraft = Some((old.minecraft.clone(), new.minecraft.clone()));
    }
    let tags = |m: &PackManifest| {
        m.files
            .iter()
            .filter_map(|f| f.feature.as_ref().map(|t| (f.path.clone(), t.clone())))
            .collect::<BTreeMap<_, _>>()
    };
    d.features_changed =
        old.features != new.features || old.presets != new.presets || tags(old) != tags(new);
    if old.recommended_memory_mb != new.recommended_memory_mb {
        d.memory = Some((old.recommended_memory_mb, new.recommended_memory_mb));
    }
    if old.loader.version != new.loader.version {
        d.loader = Some((old.loader.version.clone(), new.loader.version.clone()));
    }

    let old_by_path: BTreeMap<&str, &PackFile> =
        old.files.iter().map(|f| (f.path.as_str(), f)).collect();
    let new_paths: BTreeSet<&str> = new.files.iter().map(|f| f.path.as_str()).collect();

    let mut new_mods_unmatched = Vec::new();
    for f in &new.files {
        match old_by_path.get(f.path.as_str()) {
            Some(o) if o.sha1 == f.sha1 => {}
            Some(_) if is_mod(f) => d.mods_updated.push((f.path.clone(), f.path.clone())),
            Some(_) => d.files_changed.push(f.path.clone()),
            None if is_mod(f) => new_mods_unmatched.push(f),
            None => d.files_added.push(f.path.clone()),
        }
    }
    let mut old_mods_unmatched: Vec<&PackFile> = Vec::new();
    for f in &old.files {
        if new_paths.contains(f.path.as_str()) {
            continue;
        }
        if is_mod(f) {
            old_mods_unmatched.push(f);
        } else {
            d.files_removed.push(f.path.clone());
        }
    }

    for n in new_mods_unmatched {
        match old_mods_unmatched.iter().position(|o| same_mod(o, n)) {
            Some(i) => {
                let o = old_mods_unmatched.remove(i);
                d.mods_updated.push((o.path.clone(), n.path.clone()));
            }
            None => d.mods_added.push(n.path.clone()),
        }
    }
    d.mods_removed = old_mods_unmatched
        .into_iter()
        .map(|o| o.path.clone())
        .collect();
    d
}

#[cfg(test)]
mod tests {
    use super::*;
    use launcher_core::pack::{FilePolicy, Loader, LoaderKind};

    fn f(path: &str, sha1: &str, mod_id: Option<&str>) -> PackFile {
        PackFile {
            path: path.into(),
            sha1: sha1.into(),
            size: 1,
            url: String::new(),
            policy: FilePolicy::Managed,
            mod_id: mod_id.map(Into::into),
            feature: None,
        }
    }

    fn m(files: Vec<PackFile>) -> PackManifest {
        PackManifest {
            format_version: 1,
            id: "t".into(),
            name: "t".into(),
            version: semver::Version::new(1, 0, 0),
            minecraft: "1.21.10".into(),
            loader: Loader {
                kind: LoaderKind::Fabric,
                version: "0.17.3".into(),
            },
            recommended_memory_mb: None,
            changes: None,
            features: vec![],
            presets: vec![],
            files,
        }
    }

    #[test]
    fn name_keys_strip_versions() {
        assert_eq!(
            mod_name_key("mods/sophisticatedbackpacks-1.21.1-3.26.21.2249.jar"),
            "sophisticatedbackpacks"
        );
        assert_eq!(mod_name_key("mods/interiors-0.6.1 v3.jar"), "interiors");
        assert_eq!(
            mod_name_key("mods/sodium-fabric-0.7.2+mc1.21.10.jar"),
            "sodium"
        );
        assert_eq!(
            mod_name_key("shaderpacks/ComplementaryReimagined_r5.7.1.zip"),
            "complementaryreimagined"
        );
    }

    #[test]
    fn renamed_jar_of_same_mod_is_an_update() {
        let old = m(vec![f("mods/foo-1.0.jar", "a", None)]);
        let new = m(vec![f("mods/foo-1.1.jar", "b", Some("foo"))]);
        let d = diff(&old, &new);
        assert_eq!(
            d.mods_updated,
            vec![("mods/foo-1.0.jar".into(), "mods/foo-1.1.jar".into())]
        );
        assert!(d.mods_added.is_empty() && d.mods_removed.is_empty());
        assert_eq!(d.bump(), Bump::Patch);
    }

    #[test]
    fn bumps_follow_semver_rules() {
        let base = m(vec![
            f("mods/a-1.jar", "1", Some("a")),
            f("config/x.toml", "1", None),
        ]);
        let added = m(vec![
            f("mods/a-1.jar", "1", Some("a")),
            f("mods/b-1.jar", "1", Some("b")),
            f("config/x.toml", "1", None),
        ]);
        assert_eq!(diff(&base, &added).bump(), Bump::Minor);
        assert_eq!(diff(&added, &base).bump(), Bump::Major);
        let cfg = m(vec![
            f("mods/a-1.jar", "1", Some("a")),
            f("config/x.toml", "2", None),
        ]);
        let d = diff(&base, &cfg);
        assert_eq!(d.files_changed, vec!["config/x.toml".to_string()]);
        assert_eq!(d.bump(), Bump::Patch);
        assert!(diff(&base, &base).is_empty());
    }

    #[test]
    fn changes_block_for_players() {
        let old = m(vec![
            f("mods/foo-1.0.jar", "a", None),
            f("mods/gone-2.jar", "a", Some("gone")),
            f("config/x.toml", "1", None),
        ]);
        let new = m(vec![
            f("mods/foo-1.1.jar", "b", Some("foo")),
            f("mods/new-3.jar", "c", Some("new")),
            f("config/x.toml", "2", None),
        ]);
        let c = diff(&old, &new).changes(Some(semver::Version::new(1, 0, 0)), Some("  ".into()));
        assert_eq!(c.added, ["new-3"]);
        assert_eq!(c.removed, ["gone-2"]);
        assert_eq!(c.updated, ["foo-1.1"]);
        assert_eq!(c.other_files, 1);
        assert_eq!(c.notes, None); // blank notes dropped
    }

    #[test]
    fn next_versions() {
        let v = semver::Version::new(1, 2, 3);
        assert_eq!(next_version(&v, Bump::Patch).to_string(), "1.2.4");
        assert_eq!(next_version(&v, Bump::Minor).to_string(), "1.3.0");
        assert_eq!(next_version(&v, Bump::Major).to_string(), "2.0.0");
    }
}
