pub const SUPPORTED_LOCALES: &[&str] =
    &["en", "zh-CN", "zh-TW", "ja", "ko", "fr", "ru", "es", "pt"];

pub fn resolve_locale<I, S>(preferred: I) -> String
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    for raw in preferred {
        if let Some(matched) = match_supported(raw.as_ref()) {
            return matched.to_owned();
        }
    }
    "en".to_owned()
}

pub fn init_from_system() {
    let preferred = sys_locale::get_locales().collect::<Vec<_>>();
    let preferred = if preferred.is_empty() {
        sys_locale::get_locale().into_iter().collect::<Vec<_>>()
    } else {
        preferred
    };
    let locale = resolve_locale(preferred);
    rust_i18n::set_locale(&locale);
    log::info!("locale set to {locale}");
}

pub fn current_ip_copied_title() -> String {
    t!("status.current_ip_copied").to_string()
}

fn match_supported(raw: &str) -> Option<&'static str> {
    let normalized = raw.replace('_', "-");
    let lower = normalized.to_ascii_lowercase();
    let primary = lower.split('-').next().unwrap_or(lower.as_str());

    if lower.starts_with("zh") {
        if lower.contains("hant")
            || lower.contains("tw")
            || lower.contains("hk")
            || lower.contains("mo")
        {
            return Some("zh-TW");
        }
        return Some("zh-CN");
    }

    let mapped = match primary {
        "en" => "en",
        "ja" => "ja",
        "ko" => "ko",
        "fr" => "fr",
        "ru" => "ru",
        "es" => "es",
        "pt" => "pt",
        _ => return None,
    };

    SUPPORTED_LOCALES
        .iter()
        .copied()
        .find(|locale| *locale == mapped)
}
