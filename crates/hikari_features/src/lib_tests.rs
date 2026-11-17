use crate::{DOGFOOD_FLAGS, FeatureFlag, RELEASE_FLAGS};

fn no_env(_: &str) -> Option<String> {
    None
}

#[test]
fn unlisted_flag_is_disabled_by_default() {
    assert!(!RELEASE_FLAGS.contains(&FeatureFlag::AppServer));
    assert!(!DOGFOOD_FLAGS.contains(&FeatureFlag::AppServer));
    assert!(!FeatureFlag::AppServer.is_enabled_with(no_env));
}

#[test]
fn per_flag_env_enables() {
    let env = |name: &str| (name == "HIKARI_FEATURE_APP_SERVER").then(|| "1".to_string());
    assert!(FeatureFlag::AppServer.is_enabled_with(env));
}

#[test]
fn per_flag_env_requires_value_one() {
    let env = |name: &str| (name == "HIKARI_FEATURE_APP_SERVER").then(|| "0".to_string());
    assert!(!FeatureFlag::AppServer.is_enabled_with(env));
}

#[test]
fn env_name_matches_flag() {
    assert_eq!(
        FeatureFlag::AppServer.env_name(),
        "HIKARI_FEATURE_APP_SERVER"
    );
}

#[test]
fn every_flag_has_an_env_name() {
    for flag in enum_iterator::all::<FeatureFlag>() {
        assert!(flag.env_name().starts_with("HIKARI_FEATURE_"));
    }
}
