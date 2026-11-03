//! Persisted Settings: defaults, validation, migration.
use crate::types::*;

/// Defaults for a platform (output dir left empty = platform default resolved by the app).
pub fn default_settings(platform: Platform) -> Settings {
    todo!()
}

/// Validate ranges (fps ∈ {24,30,60}, size_pct 10–50, template renders, …).
pub fn validate(settings: &Settings) -> crate::Result<()> {
    todo!()
}

/// Load from JSON text: unknown/missing fields get defaults, `version` migrated to SETTINGS_VERSION.
pub fn from_json(json: &str, platform: Platform) -> crate::Result<Settings> {
    todo!()
}

pub fn to_json(settings: &Settings) -> crate::Result<String> {
    todo!()
}
