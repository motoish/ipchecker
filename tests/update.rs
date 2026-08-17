use std::{
    fs,
    io::{Read, Write},
    net::TcpListener,
    thread,
};

use ipchecker::update::{
    UpdateError, UpdateRelease, UpdateStatus, classify_update, fetch_update_status,
    is_trusted_asset_response_url, next_download_directory, parse_manifest, validate_extracted_app,
    verify_archive,
};

const MANIFEST: &str = r#"{
  "version": "2026.8.17-9f78eea9",
  "build": 18,
  "url": "https://github.com/motoish/ipchecker/releases/download/v2026.8.17-9f78eea9/ipchecker-2026.8.17-9f78eea9.zip",
  "size": 3,
  "sha256": "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
}"#;

fn serve_once(body: Vec<u8>) -> (String, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let handle = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut request = [0u8; 1024];
        let _ = stream.read(&mut request).unwrap();
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        )
        .unwrap();
        stream.write_all(&body).unwrap();
    });
    (format!("http://{address}/update.json"), handle)
}

#[test]
fn newer_numeric_build_is_available_even_on_the_same_calendar_day() {
    let manifest = parse_manifest(MANIFEST).expect("valid manifest");

    assert_eq!(
        classify_update(17, manifest),
        UpdateStatus::Available(UpdateRelease {
            version: "2026.8.17-9f78eea9".to_owned(),
            build: 18,
            url: "https://github.com/motoish/ipchecker/releases/download/v2026.8.17-9f78eea9/ipchecker-2026.8.17-9f78eea9.zip".to_owned(),
            size: 3,
            sha256: "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad".to_owned(),
        })
    );
}

#[test]
fn equal_or_older_build_is_not_offered_as_an_update() {
    let manifest = parse_manifest(MANIFEST).expect("valid manifest");

    assert_eq!(classify_update(18, manifest.clone()), UpdateStatus::Current);
    assert_eq!(classify_update(19, manifest), UpdateStatus::Current);
}

#[test]
fn manifest_rejects_downloads_outside_the_project_release_path() {
    let malicious = MANIFEST.replace(
        "https://github.com/motoish/ipchecker/releases/download/",
        "https://example.com/",
    );

    assert!(matches!(
        parse_manifest(&malicious),
        Err(UpdateError::UntrustedDownloadUrl(_))
    ));
}

#[test]
fn manifest_rejects_invalid_hash_and_zero_sized_archives() {
    let invalid_hash = MANIFEST.replace(
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
        "not-a-sha256",
    );
    assert!(matches!(
        parse_manifest(&invalid_hash),
        Err(UpdateError::InvalidManifest(_))
    ));

    let empty = MANIFEST.replace("\"size\": 3", "\"size\": 0");
    assert!(matches!(
        parse_manifest(&empty),
        Err(UpdateError::InvalidManifest(_))
    ));
}

#[test]
fn archive_size_and_sha256_must_both_match() {
    let directory = tempfile::tempdir().unwrap();
    let archive = directory.path().join("ipchecker.zip");
    fs::write(&archive, b"abc").unwrap();

    verify_archive(
        &archive,
        3,
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
    )
    .expect("known SHA-256 should match");
    assert!(matches!(
        verify_archive(
            &archive,
            4,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        ),
        Err(UpdateError::SizeMismatch { .. })
    ));
    assert!(matches!(
        verify_archive(
            &archive,
            3,
            "0000000000000000000000000000000000000000000000000000000000000000"
        ),
        Err(UpdateError::ChecksumMismatch)
    ));
}

#[test]
fn existing_download_directories_are_never_overwritten() {
    let downloads = tempfile::tempdir().unwrap();
    fs::create_dir(downloads.path().join("ipchecker-2026.8.17-9f78eea9")).unwrap();
    fs::create_dir(downloads.path().join("ipchecker-2026.8.17-9f78eea9-2")).unwrap();

    assert_eq!(
        next_download_directory(downloads.path(), "2026.8.17-9f78eea9"),
        downloads.path().join("ipchecker-2026.8.17-9f78eea9-3")
    );
}

#[test]
fn extracted_update_must_contain_only_the_ipchecker_app() {
    let extraction = tempfile::tempdir().unwrap();
    let app = extraction.path().join("ipchecker.app");
    fs::create_dir(&app).unwrap();

    assert_eq!(validate_extracted_app(extraction.path()).unwrap(), app);

    fs::write(extraction.path().join("unexpected.txt"), b"unexpected").unwrap();
    assert!(matches!(
        validate_extracted_app(extraction.path()),
        Err(UpdateError::InvalidArchiveLayout(_))
    ));
}

#[cfg(unix)]
#[test]
fn extracted_update_rejects_symlinks_that_escape_the_app() {
    use std::os::unix::fs::symlink;

    let extraction = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let app = extraction.path().join("ipchecker.app");
    fs::create_dir_all(app.join("Contents/Resources")).unwrap();
    symlink(outside.path(), app.join("Contents/Resources/escaped-link")).unwrap();

    assert!(matches!(
        validate_extracted_app(extraction.path()),
        Err(UpdateError::InvalidArchiveLayout(_))
    ));
}

#[test]
fn update_status_is_fetched_without_blocking_on_github_specific_api_shapes() {
    let (url, server) = serve_once(MANIFEST.as_bytes().to_vec());

    let status = fetch_update_status(&url, 17).expect("local manifest server should respond");
    server.join().unwrap();

    assert!(matches!(
        status,
        UpdateStatus::Available(UpdateRelease { build: 18, .. })
    ));
}

#[test]
fn oversized_manifest_is_rejected_before_json_parsing() {
    let (url, server) = serve_once(vec![b' '; 70 * 1024]);

    let result = fetch_update_status(&url, 17);
    server.join().unwrap();

    assert!(matches!(result, Err(UpdateError::ManifestTooLarge)));
}

#[test]
fn asset_redirects_are_limited_to_github_download_hosts() {
    assert!(is_trusted_asset_response_url(
        "https://github.com/motoish/ipchecker/releases/download/v1/ipchecker.zip"
    ));
    assert!(is_trusted_asset_response_url(
        "https://release-assets.githubusercontent.com/github-production-release-asset/file.zip"
    ));
    assert!(!is_trusted_asset_response_url(
        "https://example.com/ipchecker.zip"
    ));
    assert!(!is_trusted_asset_response_url(
        "http://release-assets.githubusercontent.com/ipchecker.zip"
    ));
}
