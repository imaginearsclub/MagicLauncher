//! Version ranges, so pack-publisher can catch mods that need another Fabric Loader or
//! Minecraft version before players crash on start (Fabric refuses to start then).
//!
//! Two syntaxes:
//! - **Fabric** (`fabric.mod.json` `depends`, [`Range::parse_fabric`]): `>=0.16.0`, `<1.22`,
//!   `~1.21.10`, `^1.0`, `1.21.x`, `=1.21.10`, `1.21.10`, `*`; space-separated parts must all
//!   match, `||` (or a JSON array) means any of them. Unlike Maven, a bare version is exact.
//! - **Maven** (`mods.toml` `versionRange`, [`Range::parse`]): `[a,b]` inclusive, `(a,b)`
//!   exclusive, an empty side is unbounded, ranges joined with commas, and a bare version is
//!   only a *recommendation* that matches everything.

use std::cmp::Ordering;

/// A version split into numbers, plus whether it has a qualifier (`-beta`), which sorts it just
/// before the plain release (`21.0.0-beta` < `21.0.0`). Missing parts count as 0.
#[derive(Debug, Clone)]
pub struct Version {
    nums: Vec<u64>,
    pre: bool,
}

impl Version {
    pub fn parse(s: &str) -> Option<Version> {
        let s = s.trim();
        if s.is_empty() {
            return None;
        }
        let (core, pre) = match s.split_once(['-', '+']) {
            Some((c, q)) => (c, s.as_bytes()[c.len()] == b'-' && !q.is_empty()),
            None => (s, false),
        };
        let nums = core
            .split('.')
            .map(|p| p.parse::<u64>().ok())
            .collect::<Option<Vec<_>>>()?;
        Some(Version { nums, pre })
    }
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> Ordering {
        let n = self.nums.len().max(other.nums.len());
        for i in 0..n {
            let a = self.nums.get(i).copied().unwrap_or(0);
            let b = other.nums.get(i).copied().unwrap_or(0);
            match a.cmp(&b) {
                Ordering::Equal => continue,
                o => return o,
            }
        }
        // Release after pre-release.
        other.pre.cmp(&self.pre)
    }
}

impl PartialEq for Version {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

impl Eq for Version {}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[derive(Debug, Clone)]
struct Bound {
    v: Version,
    inclusive: bool,
}

#[derive(Debug, Clone)]
pub struct Interval {
    lo: Option<Bound>,
    hi: Option<Bound>,
}

impl Interval {
    fn contains(&self, v: &Version) -> bool {
        let lo_ok = self
            .lo
            .as_ref()
            .is_none_or(|b| if b.inclusive { v >= &b.v } else { v > &b.v });
        let hi_ok = self
            .hi
            .as_ref()
            .is_none_or(|b| if b.inclusive { v <= &b.v } else { v < &b.v });
        lo_ok && hi_ok
    }
}

/// A parsed range. `Any` = a bare (recommended) version, or `*`: matches everything.
#[derive(Debug, Clone)]
pub enum Range {
    Any,
    Intervals(Vec<Interval>),
}

impl Range {
    /// `None` = can't parse (e.g. an unexpanded `${neo_version_range}`); callers skip it.
    pub fn parse(spec: &str) -> Option<Range> {
        let spec: String = spec.chars().filter(|c| !c.is_whitespace()).collect();
        if spec.is_empty() || spec == "*" {
            return Some(Range::Any);
        }
        if !spec.starts_with(['[', '(']) {
            // Bare version: a recommendation, not enforced.
            return Version::parse(&spec).map(|_| Range::Any);
        }
        let mut out = Vec::new();
        let mut rest = spec.as_str();
        while !rest.is_empty() {
            let open = rest.chars().next()?;
            let close_at = rest.find([']', ')'])?;
            let close = rest.as_bytes()[close_at] as char;
            let inner = &rest[1..close_at];
            let (lo_inc, hi_inc) = (open == '[', close == ']');
            let iv = match inner.split_once(',') {
                Some((lo, hi)) => Interval {
                    lo: (!lo.is_empty())
                        .then(|| {
                            Version::parse(lo).map(|v| Bound {
                                v,
                                inclusive: lo_inc,
                            })
                        })
                        .map_or(Some(None), |b| b.map(Some))?,
                    hi: (!hi.is_empty())
                        .then(|| {
                            Version::parse(hi).map(|v| Bound {
                                v,
                                inclusive: hi_inc,
                            })
                        })
                        .map_or(Some(None), |b| b.map(Some))?,
                },
                // `[1.21.1]`: exactly this version.
                None => {
                    let v = Version::parse(inner)?;
                    Interval {
                        lo: Some(Bound {
                            v: v.clone(),
                            inclusive: true,
                        }),
                        hi: Some(Bound { v, inclusive: true }),
                    }
                }
            };
            out.push(iv);
            rest = rest[close_at + 1..].trim_start_matches(',');
        }
        Some(Range::Intervals(out))
    }

    /// Fabric version predicate (see the module docs). `None` = can't parse; callers skip it.
    pub fn parse_fabric(spec: &str) -> Option<Range> {
        let mut out = Vec::new();
        for alt in spec.split("||") {
            let alt = alt.trim();
            if alt.is_empty() || alt == "*" {
                return Some(Range::Any);
            }
            let mut iv = Interval { lo: None, hi: None };
            for part in alt.split_whitespace() {
                let p = fabric_part(part)?;
                if let Some(l) = p.lo {
                    if iv.lo.as_ref().is_none_or(|cur| l.v > cur.v) {
                        iv.lo = Some(l);
                    }
                }
                if let Some(h) = p.hi {
                    if iv.hi.as_ref().is_none_or(|cur| h.v < cur.v) {
                        iv.hi = Some(h);
                    }
                }
            }
            out.push(iv);
        }
        Some(Range::Intervals(out))
    }

    pub fn contains(&self, v: &Version) -> bool {
        match self {
            Range::Any => true,
            Range::Intervals(ivs) => ivs.iter().any(|i| i.contains(v)),
        }
    }

    /// Plain-English form for people, e.g. "1.21 up to (not including) 1.21.1",
    /// "1.21.1 or newer", "exactly 1.21.1".
    pub fn describe(&self) -> String {
        let Range::Intervals(ivs) = self else {
            return "any version".into();
        };
        let one = |i: &Interval| -> String {
            match (&i.lo, &i.hi) {
                (Some(l), Some(h)) if l.v == h.v => format!("exactly {}", l.v),
                (Some(l), Some(h)) => format!(
                    "{}{} up to {}{}",
                    l.v,
                    if l.inclusive {
                        ""
                    } else {
                        " (not including it)"
                    },
                    if h.inclusive {
                        "and including "
                    } else {
                        "(not including) "
                    },
                    h.v
                ),
                (Some(l), None) if l.inclusive => format!("{} or newer", l.v),
                (Some(l), None) => format!("newer than {}", l.v),
                (None, Some(h)) if h.inclusive => format!("{} or older", h.v),
                (None, Some(h)) => format!("older than {}", h.v),
                (None, None) => "any version".into(),
            }
        };
        ivs.iter().map(one).collect::<Vec<_>>().join(", or ")
    }

    /// The smallest lower bound, if any (e.g. "21.1.262" for "[21.1.262,)").
    pub fn min(&self) -> Option<&Version> {
        match self {
            Range::Any => None,
            Range::Intervals(ivs) => ivs.iter().filter_map(|i| i.lo.as_ref().map(|b| &b.v)).min(),
        }
    }
}

/// One Fabric predicate part (`>=1.2`, `~1.21.10`, `1.21.x`, ...) as an interval.
fn fabric_part(part: &str) -> Option<Interval> {
    let incl = |v: Version| Some(Bound { v, inclusive: true });
    let excl = |v: Version| {
        Some(Bound {
            v,
            inclusive: false,
        })
    };
    let (op, rest) = ["<=", ">=", "<", ">", "=", "~", "^"]
        .iter()
        .find_map(|op| part.strip_prefix(op).map(|r| (*op, r)))
        .unwrap_or(("", part));
    // `1.21.x` / `1.x` / `1.21.*`: everything with that prefix.
    if let Some(prefix) = rest
        .strip_suffix(".x")
        .or_else(|| rest.strip_suffix(".X"))
        .or_else(|| rest.strip_suffix(".*"))
    {
        let lo = Version::parse(prefix)?;
        return Some(Interval {
            hi: excl(lo.bump(lo.nums.len() - 1)),
            lo: incl(lo),
        });
    }
    let v = Version::parse(rest)?;
    Some(match op {
        ">=" => Interval {
            lo: incl(v),
            hi: None,
        },
        ">" => Interval {
            lo: excl(v),
            hi: None,
        },
        "<=" => Interval {
            lo: None,
            hi: incl(v),
        },
        "<" => Interval {
            lo: None,
            hi: excl(v),
        },
        // Same major.minor: `~1.21.10` = >=1.21.10 <1.22.
        "~" => Interval {
            hi: excl(v.bump(1)),
            lo: incl(v),
        },
        // Same major: `^1.2` = >=1.2 <2.
        "^" => Interval {
            hi: excl(v.bump(0)),
            lo: incl(v),
        },
        // "=" or bare: exactly this version.
        _ => Interval {
            lo: incl(v.clone()),
            hi: incl(v),
        },
    })
}

impl Version {
    /// The next version at position `i`, everything after it dropped: 1.21.10 bump(1) = 1.22.
    fn bump(&self, i: usize) -> Version {
        let mut nums: Vec<u64> = (0..=i)
            .map(|j| self.nums.get(j).copied().unwrap_or(0))
            .collect();
        nums[i] += 1;
        Version { nums, pre: false }
    }
}

impl std::fmt::Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s: Vec<String> = self.nums.iter().map(u64::to_string).collect();
        write!(f, "{}", s.join("."))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok(range: &str, v: &str) -> bool {
        Range::parse(range)
            .unwrap()
            .contains(&Version::parse(v).unwrap())
    }

    #[test]
    fn ranges_from_the_pack() {
        // The 1.2.0 crash: profile 21.1.252, mods need 262+.
        assert!(!ok("[21.1.262,)", "21.1.252"));
        assert!(ok("[21.1.262,)", "21.1.267"));
        assert!(ok("[21.1,)", "21.1.267"));
        assert!(ok("[21,)", "21.1.267"));
        assert!(ok("[21.0.0-beta,)", "21.1.267"));
        assert!(ok("[4,)", "21.1.267"));
        assert!(ok("[21.1.248,21.2)", "21.1.267"));
        assert!(!ok("[21.1.248,21.2)", "21.2.0"));
        assert!(ok("[1.21.1]", "1.21.1"));
        assert!(!ok("[1.21.1]", "1.21"));
        assert!(ok("[1.21, 1.22)", "1.21.1"));
        assert!(ok("[1.21,1.21.1]", "1.21.1"));
        assert!(!ok("[1.21,1.21.1)", "1.21.1"));
        assert!(!ok("(1.21.1,)", "1.21.1"));
        // Bare = recommendation: matches anything.
        assert!(ok("1.21.1", "1.20"));
        // Unions.
        assert!(ok("[1.20,1.20.6],[1.21,1.22)", "1.21.1"));
        assert!(!ok("[1.20,1.20.6],[1.21.2,1.22)", "1.21.1"));
    }

    #[test]
    fn plain_english_ranges() {
        let d = |s: &str| Range::parse(s).unwrap().describe();
        assert_eq!(d("[1.21, 1.21.1)"), "1.21 up to (not including) 1.21.1");
        assert_eq!(d("[1.21.1,)"), "1.21.1 or newer");
        assert_eq!(d("[1.21.1]"), "exactly 1.21.1");
        assert_eq!(d("[1.21,1.21.1]"), "1.21 up to and including 1.21.1");
        assert_eq!(d("1.21.1"), "any version");
    }

    #[test]
    fn versions_and_bad_input() {
        assert!(Version::parse("21.0.0-beta").unwrap() < Version::parse("21.0.0").unwrap());
        assert!(Version::parse("21.1").unwrap() == Version::parse("21.1.0").unwrap());
        assert!(Range::parse("${neo_version_range}").is_none());
        assert!(Range::parse("[21.1.262").is_none());
        assert_eq!(
            Range::parse("[21.1.262,)")
                .unwrap()
                .min()
                .unwrap()
                .to_string(),
            "21.1.262"
        );
    }
    #[test]
    fn fabric_predicates() {
        let ok = |spec: &str, v: &str| {
            Range::parse_fabric(spec)
                .unwrap()
                .contains(&Version::parse(v).unwrap())
        };
        assert!(ok("*", "0.1"));
        assert!(ok(">=0.16.0", "0.17.3"));
        assert!(!ok(">=0.17.4", "0.17.3"));
        assert!(ok("~1.21.10", "1.21.10"));
        assert!(ok("~1.21.10", "1.21.11"));
        assert!(!ok("~1.21.10", "1.21.9"));
        assert!(!ok("~1.21.10", "1.22"));
        assert!(ok("1.21.x", "1.21.10"));
        assert!(!ok("1.21.x", "1.22"));
        assert!(ok("1.21.10", "1.21.10"));
        assert!(!ok("1.21.10", "1.21.1"), "a bare Fabric version is exact");
        assert!(ok(">=1.21.9 <1.21.11", "1.21.10"));
        assert!(!ok(">=1.21.9 <1.21.10", "1.21.10"));
        assert!(ok("1.21.8 || 1.21.10", "1.21.10"));
        assert!(ok("^1.2", "1.9"));
        assert!(!ok("^1.2", "2.0"));
        assert!(Range::parse_fabric(">=banana").is_none());
        assert_eq!(
            Range::parse_fabric(">=0.17.0").unwrap().describe(),
            "0.17.0 or newer"
        );
    }
}
