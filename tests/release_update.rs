use std::{fs, process::Command};

use serde_json::Value;

#[test]
fn release_script_generates_an_immutable_verified_update_manifest() {
    let directory = tempfile::tempdir().unwrap();
    let asset = directory.path().join("ipchecker-2026.8.17-9f78eea9.zip");
    let output = directory.path().join("update.json");
    fs::write(&asset, b"abc").unwrap();

    let status = Command::new("bash")
        .arg("scripts/generate-update-manifest.sh")
        .arg(&asset)
        .arg("2026.8.17-9f78eea9")
        .arg("18")
        .arg("v2026.8.17-9f78eea9")
        .arg("motoish/ipchecker")
        .arg(&output)
        .status()
        .unwrap();
    assert!(status.success());

    let manifest: Value = serde_json::from_slice(&fs::read(output).unwrap()).unwrap();
    assert_eq!(manifest["version"], "2026.8.17-9f78eea9");
    assert_eq!(manifest["build"], 18);
    assert_eq!(
        manifest["url"],
        "https://github.com/motoish/ipchecker/releases/download/v2026.8.17-9f78eea9/ipchecker-2026.8.17-9f78eea9.zip"
    );
    assert_eq!(manifest["size"], 3);
    assert_eq!(
        manifest["sha256"],
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
}
