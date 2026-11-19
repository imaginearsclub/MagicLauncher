//! Finding and reading pack instances on this computer.
//!
//! Supported (all only ever read):
//! - **Prism Launcher**: `mmc-pack.json` (+ `instance.cfg`), game files in `.minecraft/`
//!   (or `minecraft/`).
//! - **CurseForge**: `minecraftinstance.json`, game files in the instance folder; or a
//!   CurseForge export (`manifest.json` + `overrides/`).
//! - **Modrinth App** profiles and any other folder with a `mods/` folder. Versions come from
//!   a legacy `profile.json` when there is one; otherwise pass them (`--minecraft`, `--fabric`).

use serde::Deserialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Prism,
    CurseForge,
    /// Modrinth App profile or a plain folder.
    Folder,
}

impl Kind {
    pub fn label(self) -> &'static str {
        match self {
            Kind::Prism => "Prism Launcher",
            Kind::CurseForge => "CurseForge",
            Kind::Folder => "folder",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Instance {
    pub name: String,
    /// The folder the owner picked (instance root).
    pub path: PathBuf,
    /// Where the game files (mods/, config/, ...) are.
    pub game_dir: PathBuf,
    pub kind: Kind,
    /// Minecraft version, empty if unknown.
    pub minecraft: String,
    /// Fabric Loader version, if the instance says.
    pub fabric: Option<String>,
    /// Loader for display, e.g. "Fabric 0.17.3" or "neoforge-21.1.252".
    pub loader: Option<String>,
}

// ---------- Prism ----------

#[derive(Deserialize)]
struct MmcPack {
    components: Vec<MmcComponent>,
}

#[derive(Deserialize)]
struct MmcComponent {
    uid: String,
    #[serde(default)]
    version: Option<String>,
}

fn read_prism(path: &Path) -> Option<Instance> {
    let pack: MmcPack =
        serde_json::from_slice(&std::fs::read(path.join("mmc-pack.json")).ok()?).ok()?;
    let version_of = |uid: &str| {
        pack.components
            .iter()
            .find(|c| c.uid == uid)
            .and_then(|c| c.version.clone())
    };
    let fabric = version_of("net.fabricmc.fabric-loader");
    let other_loader = [
        "net.neoforged",
        "net.minecraftforge",
        "org.quiltmc.quilt-loader",
    ]
    .iter()
    .find_map(|uid| version_of(uid).map(|v| format!("{uid} {v}")));
    let name = std::fs::read_to_string(path.join("instance.cfg"))
        .ok()
        .and_then(|cfg| {
            cfg.lines()
                .find_map(|l| l.strip_prefix("name=").map(|n| n.trim().to_string()))
        })
        .filter(|n| !n.is_empty());
    let game_dir = [".minecraft", "minecraft"]
        .iter()
        .map(|d| path.join(d))
        .find(|d| d.is_dir())
        .unwrap_or_else(|| path.join(".minecraft"));
    Some(Instance {
        name: name.unwrap_or_else(|| folder_name(path)),
        path: path.to_path_buf(),
        game_dir,
        kind: Kind::Prism,
        minecraft: version_of("net.minecraft").unwrap_or_default(),
        loader: fabric
            .as_ref()
            .map(|v| format!("Fabric {v}"))
            .or(other_loader),
        fabric,
    })
}

// ---------- CurseForge ----------

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CfInstance {
    name: Option<String>,
    game_version: Option<String>,
    base_mod_loader: Option<CfLoader>,
}

#[derive(Deserialize)]
struct CfLoader {
    name: String,
}

#[derive(Deserialize)]
struct CfExport {
    name: Option<String>,
    minecraft: CfExportMc,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CfExportMc {
    version: String,
    #[serde(default)]
    mod_loaders: Vec<CfExportLoader>,
}

#[derive(Deserialize)]
struct CfExportLoader {
    id: String,
}

/// CurseForge loader names: `fabric-0.17.3-1.21.10` (instance) or `fabric-0.17.3` (export).
pub fn fabric_from_curseforge(name: &str) -> Option<String> {
    let rest = name.strip_prefix("fabric-")?;
    let v = rest.split('-').next()?;
    (!v.is_empty()).then(|| v.to_string())
}

fn read_curseforge(path: &Path) -> Option<Instance> {
    if let Some(i) = std::fs::read(path.join("minecraftinstance.json"))
        .ok()
        .and_then(|b| serde_json::from_slice::<CfInstance>(&b).ok())
    {
        let loader = i.base_mod_loader.map(|l| l.name);
        return Some(Instance {
            name: i.name.unwrap_or_else(|| folder_name(path)),
            path: path.to_path_buf(),
            game_dir: path.to_path_buf(),
            kind: Kind::CurseForge,
            minecraft: i.game_version.unwrap_or_default(),
            fabric: loader.as_deref().and_then(fabric_from_curseforge),
            loader,
        });
    }
    // Export: manifest.json + overrides/ (mods are only project IDs there).
    let e: CfExport =
        serde_json::from_slice(&std::fs::read(path.join("manifest.json")).ok()?).ok()?;
    let loader = e.minecraft.mod_loaders.first().map(|l| l.id.clone());
    Some(Instance {
        name: e.name.unwrap_or_else(|| folder_name(path)),
        path: path.to_path_buf(),
        game_dir: path.join("overrides"),
        kind: Kind::CurseForge,
        minecraft: e.minecraft.version,
        fabric: loader.as_deref().and_then(fabric_from_curseforge),
        loader,
    })
}

// ---------- Modrinth App / plain folder ----------

fn read_folder(path: &Path) -> Option<Instance> {
    if !path.join("mods").is_dir() {
        return None;
    }
    // Older Modrinth App versions kept a profile.json; newer ones keep it in their database.
    let profile: Option<serde_json::Value> = std::fs::read(path.join("profile.json"))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok());
    let get = |ptr: &str| {
        profile
            .as_ref()
            .and_then(|p| p.pointer(ptr))
            .and_then(|v| v.as_str())
            .map(String::from)
    };
    let is_fabric = get("/metadata/loader").is_some_and(|l| l.eq_ignore_ascii_case("fabric"));
    let fabric = is_fabric
        .then(|| get("/metadata/loader_version/id"))
        .flatten();
    Some(Instance {
        name: get("/metadata/name").unwrap_or_else(|| folder_name(path)),
        path: path.to_path_buf(),
        game_dir: path.to_path_buf(),
        kind: Kind::Folder,
        minecraft: get("/metadata/game_version").unwrap_or_default(),
        loader: fabric
            .as_ref()
            .map(|v| format!("Fabric {v}"))
            .or_else(|| get("/metadata/loader")),
        fabric,
    })
}

// ---------- public ----------

fn folder_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// Reads one instance folder; `None` if it is none of the supported kinds.
pub fn read(path: &Path) -> Option<Instance> {
    read_prism(path)
        .or_else(|| read_curseforge(path))
        .or_else(|| read_folder(path))
}

/// Where launchers keep their instances on this computer (only folders that exist).
pub fn default_dirs() -> Vec<PathBuf> {
    let env = |k: &str| std::env::var_os(k).map(PathBuf::from);
    let home = env("USERPROFILE").or_else(|| env("HOME"));
    // Per-user app data: %APPDATA%, ~/Library/Application Support, $XDG_DATA_HOME.
    let data = if cfg!(windows) {
        env("APPDATA")
    } else if cfg!(target_os = "macos") {
        home.as_ref().map(|h| h.join("Library/Application Support"))
    } else {
        env("XDG_DATA_HOME").or_else(|| home.as_ref().map(|h| h.join(".local/share")))
    };
    let mut dirs = Vec::new();
    if let Some(d) = &data {
        dirs.push(d.join("PrismLauncher").join("instances"));
        dirs.push(d.join("ModrinthApp").join("profiles"));
        dirs.push(d.join("com.modrinth.theseus").join("profiles"));
    }
    if let Some(h) = &home {
        dirs.push(h.join("curseforge").join("minecraft").join("Instances"));
    }
    dirs.retain(|d| d.is_dir());
    dirs
}

/// Every instance in `dir`, Fabric ones first, then by name.
pub fn list(dir: &Path) -> Vec<Instance> {
    let mut out: Vec<Instance> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .filter(|e| e.path().is_dir())
        .filter_map(|e| read(&e.path()))
        .collect();
    sort(&mut out);
    out
}

/// Every instance in every [`default_dirs`] folder, Fabric ones first, then by name.
pub fn list_default() -> Vec<Instance> {
    let mut out: Vec<Instance> = default_dirs().iter().flat_map(|d| list(d)).collect();
    sort(&mut out);
    out
}

fn sort(list: &mut [Instance]) {
    list.sort_by_key(|i| (i.fabric.is_none(), i.name.to_lowercase()));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("magic-instances-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn reads_prism_instance() {
        let d = tmp("prism");
        std::fs::write(
            d.join("mmc-pack.json"),
            r#"{"components":[{"uid":"org.lwjgl3","version":"3.3.3"},
                {"uid":"net.minecraft","version":"1.21.10"},
                {"uid":"net.fabricmc.intermediary","version":"1.21.10"},
                {"uid":"net.fabricmc.fabric-loader","version":"0.17.3"}],"formatVersion":1}"#,
        )
        .unwrap();
        std::fs::write(d.join("instance.cfg"), "[General]\nname=Magic Pack\n").unwrap();
        std::fs::create_dir_all(d.join(".minecraft/mods")).unwrap();
        let i = read(&d).unwrap();
        assert_eq!(i.kind, Kind::Prism);
        assert_eq!(i.name, "Magic Pack");
        assert_eq!(i.minecraft, "1.21.10");
        assert_eq!(i.fabric.as_deref(), Some("0.17.3"));
        assert_eq!(i.game_dir, d.join(".minecraft"));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn reads_curseforge_instance_and_loader_names() {
        let d = tmp("cf");
        std::fs::write(
            d.join("minecraftinstance.json"),
            r#"{"name":"Magic CF","gameVersion":"1.21.10","baseModLoader":{"name":"fabric-0.17.3-1.21.10"}}"#,
        )
        .unwrap();
        let i = read(&d).unwrap();
        assert_eq!(i.kind, Kind::CurseForge);
        assert_eq!(i.fabric.as_deref(), Some("0.17.3"));
        assert_eq!(i.game_dir, d);
        assert_eq!(
            fabric_from_curseforge("fabric-0.17.3"),
            Some("0.17.3".into())
        );
        assert_eq!(fabric_from_curseforge("neoforge-21.1.252"), None);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn plain_folder_needs_mods_and_reads_legacy_modrinth_profile() {
        let d = tmp("folder");
        assert!(read(&d).is_none(), "an empty folder is no instance");
        std::fs::create_dir_all(d.join("mods")).unwrap();
        let i = read(&d).unwrap();
        assert_eq!(i.kind, Kind::Folder);
        assert_eq!(i.minecraft, "");
        assert_eq!(i.fabric, None);
        std::fs::write(
            d.join("profile.json"),
            r#"{"metadata":{"name":"Magic MR","game_version":"1.21.10","loader":"fabric",
                "loader_version":{"id":"0.17.3"}}}"#,
        )
        .unwrap();
        let i = read(&d).unwrap();
        assert_eq!(i.name, "Magic MR");
        assert_eq!(i.minecraft, "1.21.10");
        assert_eq!(i.fabric.as_deref(), Some("0.17.3"));
        let _ = std::fs::remove_dir_all(&d);
    }
}
