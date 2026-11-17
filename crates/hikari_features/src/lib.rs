//! Runtime feature flags for gating unfinished or experimental surfaces.

use enum_iterator::Sequence;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Sequence)]
pub enum FeatureFlag {
    AppServer,
}

/// Flags enabled when `HIKARI_DOGFOOD=1` is set.
pub const DOGFOOD_FLAGS: &[FeatureFlag] = &[];

/// Flags enabled unconditionally in release builds.
pub const RELEASE_FLAGS: &[FeatureFlag] = &[];

impl FeatureFlag {
    pub fn is_enabled(self) -> bool {
        self.is_enabled_with(|name| std::env::var(name).ok())
    }

    fn is_enabled_with(self, env: impl Fn(&str) -> Option<String>) -> bool {
        if RELEASE_FLAGS.contains(&self) {
            return true;
        }
        if DOGFOOD_FLAGS.contains(&self) && env("HIKARI_DOGFOOD").as_deref() == Some("1") {
            return true;
        }
        env(self.env_name()).as_deref() == Some("1")
    }

    fn env_name(self) -> &'static str {
        match self {
            Self::AppServer => "HIKARI_FEATURE_APP_SERVER",
        }
    }
}

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;
