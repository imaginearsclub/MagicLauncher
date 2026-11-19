//! Suggest mods that could become optional features (`pack-publisher --suggest-optional`).
//!
//! Only a suggestion: a person decides what goes into `magic-pack.toml`. A mod is a
//! candidate when it is client-only (the server doesn't need it) and no other mod in the pack
//! requires it. Candidates that look like UI, libraries, compat glue or performance mods are
//! listed separately as "probably keep": switching those off tends to break screens or make
//! the game slower, which the automatic checks can't see.

use crate::features::Config;
use std::collections::{BTreeMap, BTreeSet};
use std::io::Read;
use std::path::Path;

/// What we read from one mod jar.
#[derive(Debug, Clone, Default)]
pub struct ModMeta {
    /// File name, e.g. "iris-fabric-1.9.6+mc1.21.10.jar".
    pub file: String,
    /// Mod ids declared by the jar (most declare one).
    pub ids: Vec<String>,
    pub name: Option<String>,
    /// Declares itself client-side only.
    pub client_only: bool,
    /// Mod ids this jar requires (hard dependencies).
    pub requires: Vec<String>,
    /// Required Fabric version predicates for "fabricloader" and "minecraft", as written in
    /// `fabric.mod.json` (alternatives joined with `||`).
    pub loader_ranges: Vec<(String, String)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    /// Client-only, nothing requires it, doesn't look like UI/library/performance.
    Candidate,
    /// Passes the checks but its name suggests UI, a library, compat or performance.
    ProbablyKeep(&'static str),
}

#[derive(Debug, Clone)]
pub struct Suggestion {
    pub file: String,
    pub id: String,
    pub name: String,
    pub verdict: Verdict,
}

/// Words in a mod's id or name that mean "players need this": UI, libraries, glue, speed.
const KEEP_HINTS: &[(&str, &str)] = &[
    ("lib", "library"),
    ("api", "library"),
    ("core", "library"),
    ("config", "settings screen / library"),
    ("compat", "compatibility glue"),
    ("fix", "bug fix"),
    ("patch", "bug fix"),
    ("menu", "menus / UI"),
    ("screen", "menus / UI"),
    ("gui", "menus / UI"),
    ("hud", "HUD / UI"),
    ("tooltip", "UI"),
    ("toast", "UI"),
    ("title", "menus / UI"),
    ("jei", "recipe viewer (UI)"),
    ("emi", "recipe viewer (UI)"),
    ("rei", "recipe viewer (UI)"),
    ("key", "key binds"),
    ("controlling", "key binds"),
    ("catalogue", "mod list (UI)"),
    ("font", "text rendering"),
    ("language", "translations"),
    ("loading", "loading screen"),
    ("sodium", "performance"),
    ("embeddium", "performance"),
    ("lithium", "performance"),
    ("ferrite", "performance"),
    ("modernfix", "performance"),
    ("immediatelyfast", "performance"),
    ("cull", "performance"),
    ("fps", "performance"),
    ("memory", "performance"),
    ("starlight", "performance"),
    ("optimi", "performance"),
    ("xaero", "map / minimap (UI)"),
    ("map", "map / minimap (UI)"),
    ("voice", "voice chat"),
];

fn keep_hint(id: &str, name: &str) -> Option<&'static str> {
    let hay = format!("{} {}", id, name).to_ascii_lowercase();
    KEEP_HINTS
        .iter()
        .find(|(word, _)| hay.contains(word))
        .map(|(_, why)| *why)
}

/// Decide which mods could be optional. Mods already in a feature, or listed in the config's
/// `keep`, are skipped.
pub fn suggest(mods: &[ModMeta], cfg: &Config) -> Vec<Suggestion> {
    let required: BTreeSet<&str> = mods
        .iter()
        .flat_map(|m| m.requires.iter().map(String::as_str))
        .collect();
    let mut out = Vec::new();
    for m in mods {
        let Some(id) = m.ids.first() else { continue };
        if !m.client_only
            || m.ids.iter().any(|i| required.contains(i.as_str()))
            || cfg.feature_for(&format!("mods/{}", m.file)).is_some()
            || m.ids.iter().any(|i| cfg.keep.iter().any(|k| k == i))
        {
            continue;
        }
        let name = m.name.clone().unwrap_or_else(|| id.clone());
        let verdict = match keep_hint(id, &name) {
            Some(why) => Verdict::ProbablyKeep(why),
            None => Verdict::Candidate,
        };
        out.push(Suggestion {
            file: m.file.clone(),
            id: id.clone(),
            name,
            verdict,
        });
    }
    out.sort_by_key(|s| s.name.to_ascii_lowercase());
    out
}

/// Problems that would crash players: a mod requires a mod that can be switched off, but
/// isn't itself switched off with it (i.e. it's always on, or in a different feature).
pub fn check_optional_deps(mods: &[ModMeta], cfg: &Config) -> Vec<String> {
    let feature_of_file = |file: &str| cfg.feature_for(&format!("mods/{file}"));
    let feature_of_id: BTreeMap<&str, String> = mods
        .iter()
        .filter_map(|m| feature_of_file(&m.file).map(|f| (m, f)))
        .flat_map(|(m, f)| m.ids.iter().map(move |i| (i.as_str(), f.clone())))
        .collect();
    let mut problems = Vec::new();
    for m in mods {
        let mine = feature_of_file(&m.file);
        for r in &m.requires {
            if let Some(theirs) = feature_of_id.get(r.as_str()) {
                if mine.as_ref() != Some(theirs) {
                    problems.push(format!(
                        "{} requires '{r}', which is in the optional feature '{theirs}'. Add {} to '{theirs}' in {} too (or make '{r}' always on).",
                        m.file,
                        m.file,
                        crate::features::FILE_NAME
                    ));
                }
            }
        }
    }
    problems
}

/// Result of [`check_loader_versions`].
#[derive(Debug, Default)]
pub struct LoaderCheck {
    /// Mods need another Fabric Loader or Minecraft version. Fabric refuses to start then,
    /// so this blocks publishing.
    pub error: Option<String>,
}

/// Compare every mod's required Fabric Loader / Minecraft versions with the pack's.
pub fn check_loader_versions(mods: &[ModMeta], fabric: &str, minecraft: &str) -> LoaderCheck {
    use crate::versions::{Range, Version};
    let mut out = LoaderCheck::default();
    let (Some(have_loader), Some(have_mc)) = (Version::parse(fabric), Version::parse(minecraft))
    else {
        return out;
    };
    let mut bad = Vec::new();
    let mut need_loader: Option<Version> = None;
    for m in mods {
        for (dep, spec) in &m.loader_ranges {
            let Some(range) = Range::parse_fabric(spec) else {
                continue;
            };
            let name = m.name.as_deref().unwrap_or(&m.file);
            if dep == "fabricloader" && !range.contains(&have_loader) {
                bad.push(format!(
                    "{name} ({}) needs Fabric Loader {}",
                    m.file,
                    range.describe()
                ));
                if let Some(min) = range.min() {
                    if need_loader.as_ref().is_none_or(|n| min > n) {
                        need_loader = Some(min.clone());
                    }
                }
            } else if dep == "minecraft" && !range.contains(&have_mc) {
                bad.push(format!(
                    "{name} ({}) is for Minecraft {}, not {minecraft}",
                    m.file,
                    range.describe()
                ));
            }
        }
    }
    if !bad.is_empty() {
        let mut msg = format!(
            "mods don't fit the pack's versions, so Fabric would refuse to start.\n  \
             The pack uses Minecraft {minecraft} with Fabric Loader {fabric}, but:\n"
        );
        for b in &bad {
            msg.push_str(&format!("    - {b}\n"));
        }
        if let Some(v) = need_loader.filter(|v| v > &have_loader) {
            msg.push_str(&format!(
                "  Fix: update Fabric Loader in the instance to {v} or newer (or publish with \
                 --fabric <{v} or newer>), and swap mods made for another Minecraft version."
            ));
        } else {
            msg.push_str("  Fix: swap these mods for versions made for this Minecraft version.");
        }
        out.error = Some(msg);
    }
    out
}

/// Read every jar in `<instance>/mods`.
pub fn scan_mods(instance: &Path) -> Vec<ModMeta> {
    let Ok(dir) = std::fs::read_dir(instance.join("mods")) else {
        return Vec::new();
    };
    let mut mods: Vec<ModMeta> = dir
        .flatten()
        .filter(|e| {
            e.path()
                .extension()
                .is_some_and(|x| x.eq_ignore_ascii_case("jar"))
        })
        .filter_map(|e| read_meta(&e.path()))
        .collect();
    mods.sort_by(|a, b| a.file.cmp(&b.file));
    mods
}

fn read_meta(jar: &Path) -> Option<ModMeta> {
    let file = jar.file_name()?.to_string_lossy().into_owned();
    let mut zip = zip::ZipArchive::new(std::fs::File::open(jar).ok()?).ok()?;
    let mut read = |name: &str| -> Option<String> {
        let mut s = String::new();
        zip.by_name(name).ok()?.read_to_string(&mut s).ok()?;
        Some(s)
    };
    for t in ["META-INF/neoforge.mods.toml", "META-INF/mods.toml"] {
        if let Some(m) = read(t).and_then(|text| parse_mods_toml(&file, &text)) {
            return Some(m);
        }
    }
    if let Some(m) = read("fabric.mod.json").and_then(|text| parse_fabric_json(&file, &text)) {
        return Some(m);
    }
    None
}

/// NeoForge/Forge `mods.toml`. Client-only = its dependency on Minecraft or the loader is
/// declared `side = "CLIENT"` (that's how client mods say so), or every dependency is.
pub fn parse_mods_toml(file: &str, text: &str) -> Option<ModMeta> {
    let v: toml::Value = toml::from_str(text).ok()?;
    let mods = v.get("mods")?.as_array()?;
    let ids: Vec<String> = mods
        .iter()
        .filter_map(|m| m.get("modId")?.as_str().map(String::from))
        .collect();
    if ids.is_empty() {
        return None;
    }
    let name = mods
        .first()
        .and_then(|m| m.get("displayName")?.as_str().map(String::from));
    let mut requires = Vec::new();
    let mut sides = Vec::new();
    let mut base_side = None;
    if let Some(deps) = v.get("dependencies").and_then(|d| d.as_table()) {
        for list in deps.values().filter_map(|l| l.as_array()) {
            for d in list {
                let Some(dep) = d.get("modId").and_then(|x| x.as_str()) else {
                    continue;
                };
                let side = d
                    .get("side")
                    .and_then(|x| x.as_str())
                    .unwrap_or("BOTH")
                    .to_ascii_uppercase();
                let hard = d.get("type").and_then(|x| x.as_str()).map_or_else(
                    || {
                        d.get("mandatory")
                            .and_then(|x| x.as_bool())
                            .unwrap_or(false)
                    },
                    |t| t.eq_ignore_ascii_case("required"),
                );
                if matches!(dep, "minecraft" | "neoforge" | "forge") {
                    base_side.get_or_insert(side.clone());
                } else if hard {
                    requires.push(dep.to_string());
                }
                sides.push(side);
            }
        }
    }
    let client_only = match base_side {
        Some(s) => s == "CLIENT",
        None => !sides.is_empty() && sides.iter().all(|s| s == "CLIENT"),
    };
    Some(ModMeta {
        file: file.to_string(),
        ids,
        name,
        client_only,
        requires,
        loader_ranges: Vec::new(),
    })
}

/// Fabric `fabric.mod.json`. `depends` values are a version predicate string or an array of
/// alternatives.
pub fn parse_fabric_json(file: &str, text: &str) -> Option<ModMeta> {
    let v: serde_json::Value = serde_json::from_str(text).ok()?;
    let id = v.get("id")?.as_str()?.to_string();
    let requires = v
        .get("depends")
        .and_then(|d| d.as_object())
        .map(|o| {
            o.keys()
                .filter(|k| !matches!(k.as_str(), "minecraft" | "fabricloader" | "java"))
                .cloned()
                .collect()
        })
        .unwrap_or_default();
    let loader_ranges = ["fabricloader", "minecraft"]
        .iter()
        .filter_map(|dep| {
            let spec = match v.pointer(&format!("/depends/{dep}"))? {
                serde_json::Value::String(s) => s.clone(),
                serde_json::Value::Array(a) => a
                    .iter()
                    .filter_map(|x| x.as_str())
                    .collect::<Vec<_>>()
                    .join(" || "),
                _ => return None,
            };
            Some((dep.to_string(), spec))
        })
        .collect();
    Some(ModMeta {
        file: file.to_string(),
        ids: vec![id],
        name: v.get("name").and_then(|n| n.as_str()).map(String::from),
        client_only: v.get("environment").and_then(|e| e.as_str()) == Some("client"),
        requires,
        loader_ranges,
    })
}

/// Group suggestions for printing: candidates first, then "probably keep" by reason.
pub fn report(s: &[Suggestion]) -> String {
    let mut out = String::new();
    let cands: Vec<_> = s
        .iter()
        .filter(|x| x.verdict == Verdict::Candidate)
        .collect();
    out.push_str(&format!(
        "Could be optional ({}): client-only, nothing requires them.\n\
         Test with them switched off before adding them to magic-pack.toml.\n",
        cands.len()
    ));
    for c in &cands {
        out.push_str(&format!("  {:<38} {:<28} {}\n", c.name, c.id, c.file));
    }
    let mut keep: BTreeMap<&str, Vec<&Suggestion>> = BTreeMap::new();
    for x in s {
        if let Verdict::ProbablyKeep(why) = x.verdict {
            keep.entry(why).or_default().push(x);
        }
    }
    let n: usize = keep.values().map(Vec::len).sum();
    out.push_str(&format!(
        "\nProbably keep ({n}): also client-only, but look like UI, libraries or performance mods.\n"
    ));
    for (why, list) in keep {
        out.push_str(&format!("  [{why}]\n"));
        for x in list {
            out.push_str(&format!("    {:<36} {:<28} {}\n", x.name, x.id, x.file));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> Config {
        Config::parse(crate::features::DEFAULT_CONFIG).unwrap()
    }

    #[test]
    fn reads_client_side_from_mods_toml() {
        let client = r#"
            [[mods]]
            modId = "fancyclouds"
            displayName = "Fancy Clouds"
            [[dependencies.fancyclouds]]
            modId = "neoforge"
            type = "required"
            side = "CLIENT"
            [[dependencies.fancyclouds]]
            modId = "somelib"
            type = "required"
            side = "CLIENT"
        "#;
        let m = parse_mods_toml("fancyclouds.jar", client).unwrap();
        assert!(m.client_only);
        assert_eq!(m.requires, vec!["somelib"]);

        let both = r#"
            [[mods]]
            modId = "itemphysic"
            [[dependencies.itemphysic]]
            modId = "neoforge"
            type = "required"
            side = "BOTH"
        "#;
        assert!(!parse_mods_toml("ip.jar", both).unwrap().client_only);
    }

    #[test]
    fn required_ui_and_existing_features_are_not_candidates() {
        let m = |file: &str, id: &str, client: bool, req: &[&str]| ModMeta {
            file: file.into(),
            ids: vec![id.into()],
            name: None,
            client_only: client,
            requires: req.iter().map(|s| s.to_string()).collect(),
            loader_ranges: vec![],
        };
        let mods = vec![
            m("fancyclouds.jar", "fancyclouds", true, &[]),
            m("cinematic.jar", "cinematic", true, &[]),
            m("cinecreate.jar", "cinecreate", false, &["cinematic"]),
            m("jei-1.0.jar", "jei", true, &[]),
            m("server-thing.jar", "serverthing", false, &[]),
            m("iris-fabric-1.9.6.jar", "iris", true, &[]),
        ];
        let s = suggest(&mods, &cfg());
        let ids: Vec<_> = s
            .iter()
            .map(|x| (x.id.as_str(), x.verdict.clone()))
            .collect();
        assert_eq!(
            ids,
            vec![
                ("fancyclouds", Verdict::Candidate),
                ("jei", Verdict::ProbablyKeep("recipe viewer (UI)")),
            ]
        );
    }

    #[test]
    fn flags_always_on_mod_requiring_optional_one() {
        let m = |file: &str, id: &str, req: &[&str]| ModMeta {
            file: file.into(),
            ids: vec![id.into()],
            name: None,
            client_only: true,
            requires: req.iter().map(|s| s.to_string()).collect(),
            loader_ranges: vec![],
        };
        // An Iris add-on requires iris; iris is in "shaders", the add-on isn't -> problem.
        let mods = vec![
            m("iris-fabric-1.9.6+mc1.21.10.jar", "iris", &[]),
            m("irisaddon-1.0.jar", "irisaddon", &["iris"]),
        ];
        let p = check_optional_deps(&mods, &cfg());
        assert_eq!(p.len(), 1, "{p:?}");
        assert!(p[0].starts_with("irisaddon-1.0.jar requires 'iris'"));
        // With the add-on in the same group it's fine.
        let mut cfg = cfg();
        cfg.features
            .iter_mut()
            .find(|f| f.id == "shaders")
            .unwrap()
            .include
            .push("mods/irisaddon-*".into());
        assert!(check_optional_deps(&mods, &cfg).is_empty());
    }

    #[test]
    fn loader_version_check() {
        let mods = vec![
            parse_fabric_json(
                "sodium-fabric-0.7.2+mc1.21.10.jar",
                r#"{"id":"sodium","name":"Sodium","environment":"client",
                    "depends":{"fabricloader":">=0.17.0","minecraft":["1.21.9","1.21.10"]}}"#,
            )
            .unwrap(),
            parse_fabric_json(
                "smoothcoasters-1.21.10.jar",
                r#"{"id":"smoothcoasters","depends":{"fabricloader":"*","minecraft":"~1.21.10"}}"#,
            )
            .unwrap(),
        ];
        assert!(check_loader_versions(&mods, "0.17.3", "1.21.10")
            .error
            .is_none());
        // Loader too old.
        let msg = check_loader_versions(&mods, "0.16.14", "1.21.10")
            .error
            .unwrap();
        assert!(
            msg.contains(
                "Sodium (sodium-fabric-0.7.2+mc1.21.10.jar) needs Fabric Loader 0.17.0 or newer"
            ),
            "{msg}"
        );
        assert!(msg.contains("to 0.17.0 or newer"), "{msg}");
        // Wrong Minecraft: Fabric enforces it, so it's an error too.
        let msg = check_loader_versions(&mods, "0.17.3", "1.21.1")
            .error
            .unwrap();
        assert!(msg.contains("is for Minecraft"), "{msg}");
        assert!(msg.contains("smoothcoasters-1.21.10.jar"), "{msg}");
    }

    #[test]
    fn fabric_environment() {
        let j = r#"{"id":"zoomify","name":"Zoomify","environment":"client","depends":{"fabricloader":"*","yet_another_config_lib_v3":"*"}}"#;
        let m = parse_fabric_json("zoomify.jar", j).unwrap();
        assert!(m.client_only);
        assert_eq!(m.requires, vec!["yet_another_config_lib_v3"]);
    }
}
