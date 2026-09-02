use std::{fs, net::Ipv4Addr, path::Path, process::Command};

fn page_root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("docs/pages")
        .leak()
}

fn attribute_values<'a>(html: &'a str, attribute: &str) -> Vec<&'a str> {
    let marker = format!(r#"{attribute}=""#);
    html.split(&marker)
        .skip(1)
        .filter_map(|tail| tail.split_once('"').map(|(value, _)| value))
        .collect()
}

fn css_rule<'a>(css: &'a str, selector: &str) -> &'a str {
    css.split_once(selector)
        .and_then(|(_, tail)| tail.split_once('}'))
        .map(|(rule, _)| rule)
        .unwrap_or_else(|| panic!("missing CSS rule for {selector}"))
}

#[test]
fn pages_site_references_existing_local_assets() {
    let root = page_root();
    let html = fs::read_to_string(root.join("index.html"))
        .expect("docs/pages/index.html should be a deployable entrypoint");

    for reference in attribute_values(&html, "href")
        .into_iter()
        .chain(attribute_values(&html, "src"))
    {
        if let Some(fragment) = reference.strip_prefix('#') {
            assert!(
                html.contains(&format!(r#"id="{fragment}""#)),
                "missing target for fragment link {reference}"
            );
            continue;
        }

        if reference.starts_with("https://") || reference.starts_with("mailto:") {
            continue;
        }

        assert!(
            !reference.starts_with('/'),
            "local reference {reference} must work below /ipchecker/"
        );
        let path = reference
            .split_once(['?', '#'])
            .map_or(reference, |(path, _)| path);
        assert!(
            root.join(path).is_file(),
            "local reference {reference} does not exist"
        );
    }
}

#[test]
fn primary_download_targets_the_monthly_stable_release() {
    let html = fs::read_to_string(page_root().join("index.html"))
        .expect("docs/pages/index.html should be a deployable entrypoint");
    let download_links: Vec<_> = html
        .split("<a ")
        .filter_map(|tail| tail.split_once('>'))
        .map(|(tag, _)| tag)
        .filter(|tag| tag.contains("data-primary-download"))
        .collect();

    assert_eq!(download_links.len(), 1);
    assert!(
        download_links
            .iter()
            .all(|tag| tag
                .contains(r#"href="https://github.com/motoish/ipchecker/releases/latest""#))
    );
}

#[test]
fn product_preview_uses_a_documentation_ip_address() {
    let html = fs::read_to_string(page_root().join("index.html"))
        .expect("docs/pages/index.html should be a deployable entrypoint");
    let addresses: Vec<Ipv4Addr> = html
        .split(|character: char| !(character.is_ascii_digit() || character == '.'))
        .filter_map(|candidate| candidate.parse().ok())
        .collect();

    assert!(
        !addresses.is_empty(),
        "page should contain example addresses"
    );
    for address in addresses {
        assert!(
            matches!(
                address.octets(),
                [192, 0, 2, _] | [198, 51, 100, _] | [203, 0, 113, _]
            ),
            "page contains a non-documentation IPv4 address: {address}"
        );
    }
}

#[test]
fn root_repository_link_appears_once() {
    let html = fs::read_to_string(page_root().join("index.html"))
        .expect("docs/pages/index.html should be a deployable entrypoint");

    assert_eq!(
        html.matches(r#"href="https://github.com/motoish/ipchecker""#)
            .count(),
        1
    );
}

#[test]
fn license_is_linked_once_without_repeated_page_copy() {
    let html = fs::read_to_string(page_root().join("index.html"))
        .expect("docs/pages/index.html should be a deployable entrypoint");

    assert_eq!(html.matches("BSD 3-Clause").count(), 1);
    assert_eq!(
        html.matches(r#"href="https://github.com/motoish/ipchecker/blob/main/LICENSE""#)
            .count(),
        1
    );
}

#[test]
fn optional_history_uses_an_open_section_layout() {
    let css = fs::read_to_string(page_root().join("styles.css"))
        .expect("docs/pages/styles.css should define the site appearance");
    let log_panel_rules: Vec<_> = css
        .split(".log-panel {")
        .skip(1)
        .map(|tail| {
            tail.split_once('}')
                .map(|(rule, _)| rule)
                .expect("every log panel CSS rule should close")
        })
        .collect();

    assert!(
        log_panel_rules
            .iter()
            .any(|rule| rule.contains("background: transparent;"))
    );
    for rule in log_panel_rules {
        assert!(!rule.contains("border-radius:"));
        assert!(!rule.contains("box-shadow:"));
        assert!(!rule.contains("padding:"));
    }
    assert!(css_rule(&css, ".csv-preview {").contains("background: #ffffff;"));
}

#[test]
fn headings_and_sections_use_the_compact_scale() {
    let css = fs::read_to_string(page_root().join("styles.css"))
        .expect("docs/pages/styles.css should define the site appearance");

    assert!(css_rule(&css, "h1 {").contains("font-size: clamp(42px, 5vw, 58px);"));
    assert!(css_rule(&css, ".section h2 {").contains("font-size: clamp(28px, 3.5vw, 40px);"));
    assert!(css_rule(&css, ".hero {").contains("min-height: 0;"));
    assert!(css_rule(&css, ".section {").contains("padding: 56px 0;"));
    assert!(css_rule(&css, ".section-heading {").contains("margin-bottom: 22px;"));
}

#[test]
fn feature_cards_use_a_compact_content_driven_layout() {
    let css = fs::read_to_string(page_root().join("styles.css"))
        .expect("docs/pages/styles.css should define the site appearance");
    let feature_card_rules: Vec<_> = css
        .split(".feature-card {")
        .skip(1)
        .map(|tail| {
            tail.split_once('}')
                .map(|(rule, _)| rule)
                .expect("every feature card CSS rule should close")
        })
        .collect();

    assert!(
        feature_card_rules
            .iter()
            .any(|rule| rule.contains("grid-template-columns: 30px minmax(0, 1fr);"))
    );
    assert!(
        feature_card_rules
            .iter()
            .any(|rule| rule.contains("padding: 16px;"))
    );
    assert!(
        feature_card_rules
            .iter()
            .all(|rule| !rule.contains("min-height:"))
    );
}

#[test]
fn csv_preview_uses_a_macos_title_bar() {
    let html = fs::read_to_string(page_root().join("index.html"))
        .expect("docs/pages/index.html should be a deployable entrypoint");
    let filename = "ipchecker-daily-global-ip-log-2026-09.csv";

    assert_eq!(html.matches(filename).count(), 2);
    assert!(html.contains(r#"<div class="csv-lights" aria-hidden="true">"#));
    assert!(html.contains(r#"<span class="csv-light csv-light-close"></span>"#));
    assert!(html.contains(r#"<span class="csv-light csv-light-minimize"></span>"#));
    assert!(html.contains(r#"<span class="csv-light csv-light-zoom"></span>"#));
    assert!(html.contains(&format!(
        r#"<span class="csv-title" title="{filename}">{filename}</span>"#
    )));
}

#[test]
fn app_preview_uses_macos_menu_proportions() {
    let css = fs::read_to_string(page_root().join("styles.css"))
        .expect("docs/pages/styles.css should define the site appearance");
    let product_preview = css_rule(&css, ".product-preview {");
    let tray_row = css_rule(&css, ".tray-row {");
    let tray_widget = css_rule(&css, ".tray-widget {");
    let tray_status = css_rule(&css, ".tray-status {");
    let tray_speed_row = css_rule(&css, ".tray-speeds span {");
    let app_menu = css_rule(&css, ".app-menu {");
    let app_menu_info = css_rule(&css, ".app-menu-info {");
    let app_menu_item = css_rule(&css, ".app-menu-item {");
    let menu_separator = css_rule(&css, ".menu-separator {");

    assert!(product_preview.contains("max-width: 320px;"));
    assert!(tray_row.contains("min-height: 31px;"));
    assert!(tray_widget.contains("width: max-content;"));
    assert!(!tray_widget.contains("min-width:"));
    assert!(tray_widget.contains("grid-template-columns: 43px 17px auto;"));
    assert!(tray_widget.contains("gap: 4px;"));
    assert!(tray_widget.contains("font-size: 11px;"));
    assert!(tray_status.contains("width: 17px;"));
    assert!(tray_status.contains("height: 17px;"));
    assert!(tray_speed_row.contains("grid-template-columns: 10px auto;"));
    assert!(tray_speed_row.contains("column-gap: 2px;"));
    assert!(!tray_speed_row.contains("space-between"));
    assert!(app_menu.contains("max-width: 271px;"));
    assert!(app_menu.contains("margin-left: auto;"));
    assert!(app_menu.contains("font-size: 13px;"));
    assert!(app_menu.contains("border-radius: 10px;"));
    assert!(app_menu.contains("padding: 8px 0;"));
    assert!(app_menu_info.contains("padding: 2px 12px 2px 25px;"));
    assert!(app_menu_item.contains("min-height: 24px;"));
    assert!(app_menu_item.contains("padding: 3px 25px;"));
    assert!(menu_separator.contains("margin: 5px 16px;"));
    assert!(!css.contains("grid-template-columns: 46px 28px auto;"));
    assert!(!css.contains("min-height: 48px;"));
    assert!(!css.contains("min-height: 27px;"));
}

#[test]
fn tray_status_icon_uses_the_real_knockout_geometry() {
    let html = fs::read_to_string(page_root().join("index.html"))
        .expect("docs/pages/index.html should be a deployable entrypoint");
    let css = fs::read_to_string(page_root().join("styles.css"))
        .expect("docs/pages/styles.css should define the site appearance");
    let tray_status = css_rule(&css, ".tray-status {");

    assert!(!html.contains(r#"class="tray-status" aria-hidden="true">✓"#));
    assert!(html.contains(r#"<mask id="tray-check-mask">"#));
    assert!(html.contains(r#"<circle cx="17.5" cy="17.5" r="14.6" fill="white"></circle>"#));
    assert!(html.contains(r#"d="M 10 18 L 16 24 L 26 12""#));
    assert!(html.contains(r#"stroke-width="2.6""#));
    assert!(!tray_status.contains("box-shadow:"));
}

#[test]
fn product_preview_contains_the_real_menu_items() {
    let html = fs::read_to_string(page_root().join("index.html"))
        .expect("docs/pages/index.html should be a deployable entrypoint");

    for label in [
        "Set Expected IP...",
        "Use Current Public IP",
        "Check Interval",
        "Check Now",
        "Show Network Speed",
        "Show Network Latency",
        "Show Status Icon",
        "Record Daily Public IP Log",
        "Record VPN Addresses",
        "Change Log Output Folder...",
        "Mute for This Session",
        "Check for Updates...",
        "About ipchecker...",
        "Quit",
    ] {
        assert!(html.contains(label), "preview is missing {label}");
    }
}

#[test]
fn pages_workflow_validator_accepts_the_site() {
    let status = Command::new("python3")
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("scripts/validate-pages.py"))
        .arg(page_root())
        .status()
        .expect("python3 should run the Pages validator");

    assert!(status.success());
}
