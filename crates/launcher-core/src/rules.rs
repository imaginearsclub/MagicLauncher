//! Mojang "rules" evaluation used by libraries and arguments.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RuleAction {
    Allow,
    Disallow,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OsRule {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arch: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Rule {
    pub action: RuleAction,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub os: Option<OsRule>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub features: Option<HashMap<String, bool>>,
}

/// The environment rules are evaluated against.
#[derive(Debug, Clone)]
pub struct Env {
    pub os: String,
    pub arch: String,
    pub features: HashMap<String, bool>,
}

impl Env {
    pub fn current() -> Self {
        Self {
            os: current_os().to_string(),
            arch: current_arch().to_string(),
            features: HashMap::new(),
        }
    }

    pub fn with_feature(mut self, name: &str, on: bool) -> Self {
        self.features.insert(name.to_string(), on);
        self
    }
}

/// Mojang's OS names.
pub fn current_os() -> &'static str {
    match std::env::consts::OS {
        "windows" => "windows",
        "macos" => "osx",
        _ => "linux",
    }
}

/// Mojang's arch names (only "x86" for 32-bit is used in practice).
pub fn current_arch() -> &'static str {
    match std::env::consts::ARCH {
        "x86" => "x86",
        "aarch64" => "arm64",
        other => other,
    }
}

impl Rule {
    fn matches(&self, env: &Env) -> bool {
        if let Some(os) = &self.os {
            if os.name.as_deref().is_some_and(|n| n != env.os) {
                return false;
            }
            if os.arch.as_deref().is_some_and(|a| a != env.arch) {
                return false;
            }
            // `version` is a regex on the OS version, only used by ancient versions. Ignored.
        }
        if let Some(features) = &self.features {
            for (k, want) in features {
                if env.features.get(k).copied().unwrap_or(false) != *want {
                    return false;
                }
            }
        }
        true
    }
}

/// No rules = allowed. Otherwise start disallowed and let the last matching rule win.
pub fn allowed(rules: &[Rule], env: &Env) -> bool {
    if rules.is_empty() {
        return true;
    }
    let mut result = false;
    for r in rules {
        if r.matches(env) {
            result = r.action == RuleAction::Allow;
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env(os: &str, arch: &str) -> Env {
        Env {
            os: os.into(),
            arch: arch.into(),
            features: HashMap::new(),
        }
    }

    fn rules(json: &str) -> Vec<Rule> {
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn empty_rules_allow() {
        assert!(allowed(&[], &env("linux", "x86_64")));
    }

    #[test]
    fn os_allow_rule() {
        let r = rules(r#"[{"action":"allow","os":{"name":"osx"}}]"#);
        assert!(allowed(&r, &env("osx", "arm64")));
        assert!(!allowed(&r, &env("windows", "x86_64")));
    }

    #[test]
    fn allow_then_disallow_last_wins() {
        let r = rules(r#"[{"action":"allow"},{"action":"disallow","os":{"name":"osx"}}]"#);
        assert!(allowed(&r, &env("linux", "x86_64")));
        assert!(!allowed(&r, &env("osx", "x86_64")));
    }

    #[test]
    fn arch_rule() {
        let r = rules(r#"[{"action":"allow","os":{"arch":"x86"}}]"#);
        assert!(allowed(&r, &env("windows", "x86")));
        assert!(!allowed(&r, &env("windows", "x86_64")));
    }

    #[test]
    fn feature_rules() {
        let r = rules(r#"[{"action":"allow","features":{"is_quick_play_multiplayer":true}}]"#);
        let e = env("linux", "x86_64");
        assert!(!allowed(&r, &e));
        assert!(allowed(
            &r,
            &e.with_feature("is_quick_play_multiplayer", true)
        ));
    }
}
