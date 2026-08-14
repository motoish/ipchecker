use ipchecker::about::{about_informative_text, about_message_title};

#[test]
fn about_copy_includes_package_version_and_product_name() {
    rust_i18n::set_locale("zh-CN");
    let version = env!("CARGO_PKG_VERSION");
    assert_eq!(about_message_title(), "ipchecker");
    assert!(
        about_informative_text().contains(&format!("版本 {version}")),
        "about text should include version, got {}",
        about_informative_text()
    );
    assert!(about_informative_text().contains("macOS 菜单栏"));
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
