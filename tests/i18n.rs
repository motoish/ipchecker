use ipchecker::i18n::{SUPPORTED_LOCALES, resolve_locale};

#[test]
fn resolves_exact_and_prefix_locales_to_supported_codes() {
    assert_eq!(resolve_locale(["zh-CN"]), "zh-CN");
    assert_eq!(resolve_locale(["zh-Hans-CN"]), "zh-CN");
    assert_eq!(resolve_locale(["zh-TW"]), "zh-TW");
    assert_eq!(resolve_locale(["zh-Hant-HK"]), "zh-TW");
    assert_eq!(resolve_locale(["en-US"]), "en");
    assert_eq!(resolve_locale(["ja-JP"]), "ja");
    assert_eq!(resolve_locale(["pt-BR"]), "pt");
}

#[test]
fn falls_back_through_preferred_list_then_english() {
    assert_eq!(resolve_locale(["fa-IR", "fr-FR"]), "fr");
    assert_eq!(resolve_locale(["fa-IR", "xx-YY"]), "en");
}

#[test]
fn supported_locales_cover_the_documented_set() {
    for locale in ["en", "zh-CN", "zh-TW", "ja", "ko", "fr", "ru", "es", "pt"] {
        assert!(
            SUPPORTED_LOCALES.contains(&locale),
            "missing supported locale {locale}"
        );
    }
}
