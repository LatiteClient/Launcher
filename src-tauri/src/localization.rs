use serde_json::Value;

const AUTO_LOCALE_ID: &str = "auto";
const DEFAULT_LOCALE_ID: &str = "en_US";
const LANGUAGE_OPTION_ID: &str = "launcher_language";
const LANGUAGE_FALLBACKS: &[(&str, &str)] = &[("es", "es_ES"), ("fa", "fa_IR")];
const LOCALES: &[(&str, &str)] = &[
    ("cs_CZ", include_str!("../../src/locales/cs_CZ.json")),
    ("en_US", include_str!("../../src/locales/en_US.json")),
    ("es_ES", include_str!("../../src/locales/es_ES.json")),
    ("fa_IR", include_str!("../../src/locales/fa_IR.json")),
    ("fr_FR", include_str!("../../src/locales/fr_FR.json")),
    ("ja_JP", include_str!("../../src/locales/ja_JP.json")),
    ("nl_NL", include_str!("../../src/locales/nl_NL.json")),
    ("pl_PL", include_str!("../../src/locales/pl_PL.json")),
    ("pt_PT", include_str!("../../src/locales/pt_PT.json")),
];

pub fn get_translation(key: &str) -> Option<String> {
    let language_preference = get_language_preference();
    let system_locale = tauri::api::os::locale();
    let locale_id = resolve_locale(language_preference.as_deref(), system_locale.as_deref());

    get_translation_for_locale(locale_id, key)
}

fn get_language_preference() -> Option<String> {
    let options_path = crate::paths::get_options_path().ok()?;
    let options_file = std::fs::File::open(options_path).ok()?;
    let options: Value = serde_json::from_reader(options_file).ok()?;

    options
        .get(LANGUAGE_OPTION_ID)?
        .as_str()
        .map(str::to_string)
}

fn get_translation_for_locale(locale_id: &str, key: &str) -> Option<String> {
    get_locale_translation(locale_id, key).or_else(|| {
        if locale_id == DEFAULT_LOCALE_ID {
            None
        } else {
            get_locale_translation(DEFAULT_LOCALE_ID, key)
        }
    })
}

fn get_locale_translation(locale_id: &str, key: &str) -> Option<String> {
    let locale_json = LOCALES
        .iter()
        .find(|(registered_locale, _)| *registered_locale == locale_id)?
        .1;

    let value: Value = serde_json::from_str(locale_json).ok()?;
    let translations = value.get("translations")?;

    translations.get(key)?.as_str().map(str::to_string)
}

fn resolve_locale(preference: Option<&str>, system_locale: Option<&str>) -> &'static str {
    match preference.and_then(normalize_locale_id) {
        Some(locale_id) if locale_id != AUTO_LOCALE_ID => resolve_locale_candidate(&locale_id),
        _ => system_locale
            .map(resolve_locale_candidate)
            .unwrap_or(DEFAULT_LOCALE_ID),
    }
}

fn resolve_locale_candidate(locale_id: &str) -> &'static str {
    let Some(normalized_locale_id) = normalize_locale_id(locale_id) else {
        return DEFAULT_LOCALE_ID;
    };

    if let Some((registered_locale, _)) = LOCALES
        .iter()
        .find(|(registered_locale, _)| *registered_locale == normalized_locale_id.as_str())
    {
        return registered_locale;
    }

    let language_code = normalized_locale_id
        .split('_')
        .next()
        .unwrap_or(DEFAULT_LOCALE_ID);

    if let Some((_, fallback_locale)) = LANGUAGE_FALLBACKS
        .iter()
        .find(|(fallback_language, _)| *fallback_language == language_code)
    {
        return fallback_locale;
    }

    LOCALES
        .iter()
        .find(|(registered_locale, _)| registered_locale.split('_').next() == Some(language_code))
        .map(|(registered_locale, _)| *registered_locale)
        .unwrap_or(DEFAULT_LOCALE_ID)
}

fn normalize_locale_id(value: &str) -> Option<String> {
    let normalized = value.trim().replace('-', "_");
    let normalized = normalized.split('.').next().unwrap_or("").trim();

    if normalized.is_empty() {
        return None;
    }

    let mut parts = normalized.split('_');
    let language = parts.next()?.trim();

    if language.is_empty() {
        return None;
    }

    let mut locale_id = language.to_ascii_lowercase();

    if let Some(region) = parts.next() {
        let region = region.trim();

        if !region.is_empty() {
            locale_id.push('_');
            locale_id.push_str(&region.to_ascii_uppercase());
        }
    }

    for part in parts {
        let part = part.trim();

        if part.is_empty() {
            continue;
        }

        locale_id.push('_');
        locale_id.push_str(part);
    }

    Some(locale_id)
}
