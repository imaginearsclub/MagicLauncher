//! Optional pack features and presets (see `magic-pack.toml`).
//!
//! The default config is compiled in; a `magic-pack.toml` in the instance root overrides it,
//! so the owner can change groups without a new pack-publisher.

use launcher_core::pack::{Feature, Preset};
use serde::Deserialize;
use std::path::Path;

pub const DEFAULT_CONFIG: &str = include_str!("../magic-pack.toml");
pub const FILE_NAME: &str = "magic-pack.toml";

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Config {
    #[serde(default, rename = "feature")]
    pub features: Vec<FeatureDef>,
    #[serde(default, rename = "preset")]
    pub presets: Vec<Preset>,
    /// Mod ids never suggested as optional (`pack-publisher --suggest-optional`): mods we
    /// checked and decided players need.
    #[serde(default)]
    pub keep: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FeatureDef {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default = "yes")]
    pub default: bool,
    #[serde(default)]
    pub include: Vec<String>,
}

fn yes() -> bool {
    true
}

impl Config {
    pub fn parse(text: &str) -> Result<Config, String> {
        let c: Config = toml::from_str(text).map_err(|e| format!("{FILE_NAME}: {e}"))?;
        for p in &c.presets {
            for id in &p.disable {
                if !c.features.iter().any(|f| &f.id == id) {
                    return Err(format!(
                        "{FILE_NAME}: preset '{}' disables unknown feature '{id}'",
                        p.id
                    ));
                }
            }
        }
        Ok(c)
    }

    /// The instance's own `magic-pack.toml`, or the built-in default.
    pub fn for_instance(root: &Path) -> Result<(Config, &'static str), String> {
        match std::fs::read_to_string(root.join(FILE_NAME)) {
            Ok(t) => Ok((Config::parse(&t)?, "instance")),
            Err(_) => Ok((Config::parse(DEFAULT_CONFIG)?, "built-in")),
        }
    }

    /// The feature a pack path belongs to, if any (first match wins).
    pub fn feature_for(&self, rel: &str) -> Option<String> {
        let rel = rel.to_ascii_lowercase();
        self.features
            .iter()
            .find(|f| {
                f.include
                    .iter()
                    .any(|p| glob(&p.to_ascii_lowercase(), &rel))
            })
            .map(|f| f.id.clone())
    }

    pub fn manifest_features(&self) -> Vec<Feature> {
        self.features
            .iter()
            .map(|f| Feature {
                id: f.id.clone(),
                name: f.name.clone(),
                description: f.description.clone(),
                default: f.default,
            })
            .collect()
    }
}

/// `*` matches any run of characters (including `/`); everything else is literal.
pub fn glob(pattern: &str, text: &str) -> bool {
    let parts: Vec<&str> = pattern.split('*').collect();
    if parts.len() == 1 {
        return pattern == text;
    }
    let (first, last) = (parts[0], parts[parts.len() - 1]);
    if !text.starts_with(first) || !text[first.len()..].ends_with(last) {
        return false;
    }
    let mut rest = &text[first.len()..text.len() - last.len()];
    for mid in &parts[1..parts.len() - 1] {
        match rest.find(mid) {
            Some(i) => rest = &rest[i + mid.len()..],
            None => return false,
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn globs() {
        assert!(glob("mods/iris-*", "mods/iris-fabric-1.9.6+mc1.21.10.jar"));
        assert!(glob("shaderpacks/*", "shaderpacks/complementary.zip"));
        assert!(glob("mods/bobby*", "mods/bobby_reneoforged-5.2.4.jar"));
        assert!(!glob("mods/iris-*", "mods/irisflw.jar"));
        assert!(glob("a*b*c", "a-x-b-y-c"));
        assert!(!glob("a*b*c", "a-x-c"));
    }

    #[test]
    fn default_config_is_valid_and_maps_mods() {
        let c = Config::parse(DEFAULT_CONFIG).unwrap();
        assert_eq!(c.presets.len(), 3);
        assert_eq!(
            c.feature_for("mods/iris-fabric-1.9.6+mc1.21.10.jar")
                .as_deref(),
            Some("shaders")
        );
        assert_eq!(
            c.feature_for("shaderpacks/ComplementaryReimagined_r5.7.1.zip")
                .as_deref(),
            Some("shaders")
        );
        assert_eq!(
            c.feature_for("mods/lambdynamiclights-4.8.0+1.21.10.jar")
                .as_deref(),
            Some("dynamic_lights")
        );
        // Required mods are never optional.
        assert_eq!(
            c.feature_for("mods/sodium-fabric-0.7.2+mc1.21.10.jar"),
            None
        );
        assert_eq!(c.feature_for("mods/smoothcoasters-1.21.10-v1.jar"), None);
    }

    #[test]
    fn rejects_preset_with_unknown_feature() {
        let bad = "[[preset]]\nid = \"x\"\nname = \"X\"\ndisable = [\"nope\"]\n";
        assert!(Config::parse(bad).is_err());
    }
}
