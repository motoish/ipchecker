use ipchecker::about::{
    about_description, about_informative_text, about_message_title, marketing_version,
};

#[test]
fn marketing_version_strips_prerelease_hash() {
    let package = env!("CARGO_PKG_VERSION");
    let marketing = marketing_version();
    assert!(
        package == marketing || package.starts_with(&format!("{marketing}-")),
        "marketing version {marketing} should be the date prefix of {package}"
    );
    assert!(
        !marketing.contains('-'),
        "About version should not include the commit hash, got {marketing}"
    );
}

#[test]
fn about_copy_uses_cursor_style_version_line() {
    rust_i18n::set_locale("zh-CN");
    assert_eq!(about_message_title(), "ipchecker");
    assert_eq!(
        about_informative_text(),
        format!("版本 {}", marketing_version())
    );
    assert_eq!(about_description(), "监控公网 IPv4 的 macOS 菜单栏应用。");

    rust_i18n::set_locale("en");
    assert_eq!(
        about_informative_text(),
        format!("Version {}", marketing_version())
    );
    assert_eq!(
        about_description(),
        "A macOS menu bar app that watches your public IPv4."
    );
}

#[cfg(target_os = "macos")]
#[test]
fn about_build_time_line_includes_relative_time() {
    let line = ipchecker::about::about_build_time_line();
    assert!(
        line.contains('(') && line.ends_with(')'),
        "build time should include a relative suffix, got {line}"
    );
}

#[cfg(target_os = "macos")]
#[test]
fn exposes_native_about_dialog_with_the_runtime_contract() {
    use ipchecker::about::{apply_application_icon, show_about};

    let _show: fn() = show_about;
    let _apply: fn() = apply_application_icon;
}

#[cfg(target_os = "macos")]
#[test]
fn embeds_about_icon_png_asset() {
    let bytes = include_bytes!("../resources/about-icon.png");
    assert!(
        bytes.starts_with(&[0x89, b'P', b'N', b'G']),
        "about icon must be a PNG"
    );
    assert!(bytes.len() > 1_000, "about icon should not be empty");
}
