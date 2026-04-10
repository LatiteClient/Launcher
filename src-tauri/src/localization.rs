use serde_json::Value;

// Simple compile-time embedded default locale lookup (en_US.json).
// This minimal helper returns the translation string for a key from the
// embedded English locale. It's intentionally small — for full localization
// support you'd load the user's selected locale and fallbacks.
pub fn get_translation(key: &str) -> Option<String> {
    // Path is relative to this file: src-tauri/src -> ../../src/locales/en_US.json
    const EN_US_JSON: &str = include_str!("../../src/locales/en_US.json");

    let v: Value = serde_json::from_str(EN_US_JSON).ok()?;
    let translations = v.get("translations")?;
    translations.get(key)?.as_str().map(|s| s.to_string())
}
