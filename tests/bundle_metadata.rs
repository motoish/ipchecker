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

    let short = dict["CFBundleShortVersionString"]
        .as_string()
        .expect("CFBundleShortVersionString");
    let segments: Vec<&str> = short.split('.').collect();
    assert_eq!(
        segments.len(),
        3,
        "CFBundleShortVersionString must be three numeric segments, got {short}"
    );
    assert!(
        segments
            .iter()
            .all(|segment| !segment.is_empty() && segment.chars().all(|ch| ch.is_ascii_digit())),
        "CFBundleShortVersionString must be digits only, got {short}"
    );
    let package = env!("CARGO_PKG_VERSION");
    let marketing = package
        .split_once('-')
        .map(|(head, _)| head)
        .unwrap_or(package);
    assert_eq!(short, marketing);

    let build = dict["CFBundleVersion"]
        .as_string()
        .expect("CFBundleVersion");
    assert!(
        !build.is_empty() && build.chars().all(|ch| ch.is_ascii_digit()),
        "CFBundleVersion must be a numeric build, got {build}"
    );
    assert!(
        std::path::Path::new("resources/AppIcon.icns").is_file(),
        "resources/AppIcon.icns must exist for Finder / .app icon"
    );
}
