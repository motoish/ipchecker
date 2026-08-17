use std::env;
use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

fn main() {
    println!("cargo:rerun-if-env-changed=SOURCE_DATE_EPOCH");
    // These inputs are compiled or bundled into the app. Without them, Cargo
    // reuses a stale IPCHECKER_BUILD_UNIX_SECS after incremental rebuilds.
    println!("cargo:rerun-if-changed=src");
    println!("cargo:rerun-if-changed=resources");
    println!("cargo:rerun-if-changed=locales");
    println!("cargo:rerun-if-changed=Cargo.toml");
    println!("cargo:rerun-if-changed=Cargo.lock");

    let secs = env::var("SOURCE_DATE_EPOCH")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or_else(|| {
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|duration| duration.as_secs())
                .unwrap_or(0)
        });
    println!("cargo:rustc-env=IPCHECKER_BUILD_UNIX_SECS={secs}");

    let info_plist = fs::read_to_string("resources/Info.plist")
        .expect("resources/Info.plist must be readable at build time");
    let build = plist_string(&info_plist, "CFBundleVersion")
        .expect("CFBundleVersion must exist in resources/Info.plist");
    build
        .parse::<u64>()
        .expect("CFBundleVersion must be a positive integer");
    println!("cargo:rustc-env=IPCHECKER_BUILD_NUMBER={build}");
}

fn plist_string<'a>(plist: &'a str, key: &str) -> Option<&'a str> {
    let marker = format!("<key>{key}</key>");
    let after_key = plist.split_once(&marker)?.1;
    let after_open = after_key.split_once("<string>")?.1;
    Some(after_open.split_once("</string>")?.0.trim())
}
