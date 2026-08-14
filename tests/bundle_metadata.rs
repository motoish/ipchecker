#[test]
fn info_plist_declares_agent_app_and_macos_13() {
    let value = plist::Value::from_file("resources/Info.plist").unwrap();
    let dict = value.as_dictionary().unwrap();

    assert_eq!(
        dict["CFBundleIdentifier"].as_string(),
        Some("com.tanishi.ipchecker")
    );
    assert_eq!(dict["LSUIElement"].as_boolean(), Some(true));
    assert_eq!(dict["LSMinimumSystemVersion"].as_string(), Some("13.0"));
    assert_eq!(dict["CFBundleExecutable"].as_string(), Some("ipchecker"));
    assert_eq!(dict["CFBundleIconFile"].as_string(), Some("AppIcon"));
    assert_eq!(dict["CFBundleIconName"].as_string(), Some("AppIcon"));
    assert!(
        std::path::Path::new("resources/AppIcon.icns").is_file(),
        "resources/AppIcon.icns must exist for Finder / .app icon"
    );
}
