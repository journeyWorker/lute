//! dsl 0.27.0 §5: seasons — named state tiers opened by a live condition.
//!
//! A schema MAY declare `seasons: { harvest: { live: "@harvestLive" } }`.
//! Each season owns the state paths `season.<name>.<field>` (declared under
//! `state:` like any other), the read-only mirror `prev.season.<name>.*`,
//! beats with `once: season:<name>` and quests with `tier="season:<name>"`.
//! When `live` goes false→true the season opens: its state resets to the
//! declared defaults (the old values move to `prev.season.<name>.*`), its
//! `once` spends clear and its quests return to `unset`.
//!
//! Pure data and path arithmetic, shared by the checker, the compiler and
//! the runtime.

use serde::{Deserialize, Serialize};

/// The state root every season's paths live under.
pub const SEASON_ROOT: &str = "season";

/// The `once:` / `tier=` spelling prefix naming a season (`season:harvest`).
pub const SEASON_PREFIX: &str = "season:";

/// One season as declared (`{ live: "<condition>" }`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SeasonDecl {
    /// The condition that holds while the season's window is open.
    pub live: String,
}

/// The season a `season:<name>` spelling names (`None` for any other).
pub fn season_ref(raw: &str) -> Option<&str> {
    raw.strip_prefix(SEASON_PREFIX)
}

/// The season whose tier `path` belongs to: `season.<name>.<field…>`.
pub fn season_of_path(path: &str) -> Option<&str> {
    let rest = path.strip_prefix("season.")?;
    let (name, field) = rest.split_once('.')?;
    (!name.is_empty() && !field.is_empty()).then_some(name)
}

/// `prev.season.<name>.<field>` for a `season.<name>.<field>` path.
pub fn prev_season_path(path: &str) -> Option<String> {
    season_of_path(path).map(|_| format!("prev.{path}"))
}

/// `true` for a `prev.season.*` mirror path.
pub fn is_prev_season_path(path: &str) -> bool {
    path.strip_prefix("prev.")
        .is_some_and(|p| season_of_path(p).is_some())
}

/// `true` for a season name: an identifier `[A-Za-z][A-Za-z0-9_]*`.
pub fn is_season_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars.next().is_some_and(|c| c.is_ascii_alphabetic())
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn season_paths() {
        assert_eq!(season_of_path("season.harvest.tokens"), Some("harvest"));
        assert_eq!(season_of_path("season.harvest"), None);
        assert_eq!(season_of_path("run.x"), None);
        assert_eq!(
            prev_season_path("season.harvest.tokens").as_deref(),
            Some("prev.season.harvest.tokens")
        );
        assert!(is_prev_season_path("prev.season.harvest.tokens"));
        assert!(!is_prev_season_path("prev.run.x"));
        assert_eq!(season_ref("season:harvest"), Some("harvest"));
        assert!(is_season_name("harvest_2") && !is_season_name("2x") && !is_season_name(""));
    }
}
