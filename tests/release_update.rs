use std::{fs, path::Path, process::Command};

use serde_json::Value;

fn release_fixture() -> tempfile::TempDir {
    let directory = tempfile::tempdir().unwrap();
    fs::create_dir(directory.path().join("scripts")).unwrap();
    fs::create_dir(directory.path().join("resources")).unwrap();
    fs::write(
        directory.path().join("Cargo.toml"),
        r#"[package]
name = "ipchecker"
version = "2026.8.17-deadbeef"
"#,
    )
    .unwrap();
    fs::write(
        directory.path().join("Cargo.lock"),
        r#"version = 4

[[package]]
name = "ipchecker"
version = "2026.8.17-deadbeef"

[[package]]
name = "unrelated"
version = "1.0.0"
"#,
    )
    .unwrap();
    fs::write(
        directory.path().join("resources/Info.plist"),
        r#"<plist><dict>
<key>CFBundleVersion</key>
<string>17</string>
<key>CFBundleShortVersionString</key>
<string>2026.8.17</string>
</dict></plist>
"#,
    )
    .unwrap();

    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("scripts/prepare-release-version.sh");
    fs::copy(
        source,
        directory.path().join("scripts/prepare-release-version.sh"),
    )
    .unwrap();
    directory
}

fn run_prepare_version(directory: &Path, version: &str, build: &str) -> std::process::Output {
    Command::new("bash")
        .arg("scripts/prepare-release-version.sh")
        .arg(version)
        .arg(build)
        .current_dir(directory)
        .output()
        .unwrap()
}

fn run_identity(timezone: &str, sha: &str, epoch: &str) -> (std::process::Output, String) {
    let directory = tempfile::tempdir().unwrap();
    let github_output = directory.path().join("github_output");
    fs::write(&github_output, "").unwrap();
    let output = Command::new("bash")
        .arg("scripts/calver-identity.sh")
        .arg(timezone)
        .arg(sha)
        .arg(epoch)
        .env("GITHUB_OUTPUT", &github_output)
        .output()
        .unwrap();
    let written = fs::read_to_string(&github_output).unwrap();
    (output, written)
}

fn run_commit_count(directory: &Path) -> std::process::Output {
    Command::new("bash")
        .arg(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("scripts/commit-count.sh")
                .to_str()
                .unwrap(),
        )
        .current_dir(directory)
        .output()
        .unwrap()
}

fn git(directory: &Path, args: &[&str]) -> std::process::Output {
    Command::new("git")
        .args(args)
        .current_dir(directory)
        .output()
        .unwrap()
}

fn git_repo_with_commits(count: usize) -> tempfile::TempDir {
    let directory = tempfile::tempdir().unwrap();
    assert!(
        git(directory.path(), &["init", "-b", "main"])
            .status
            .success()
    );
    for index in 1..=count {
        fs::write(directory.path().join("file"), index.to_string()).unwrap();
        assert!(git(directory.path(), &["add", "file"]).status.success());
        assert!(
            git(
                directory.path(),
                &[
                    "-c",
                    "user.name=ipchecker",
                    "-c",
                    "user.email=ipchecker@example.test",
                    "commit",
                    "--no-gpg-sign",
                    "-m",
                    &format!("commit {index}"),
                ],
            )
            .status
            .success()
        );
    }
    directory
}

fn step_index(steps: &[&Value], name: &str) -> usize {
    steps
        .iter()
        .position(|step| step["name"] == name)
        .unwrap_or_else(|| panic!("workflow step {name:?} is missing"))
}

fn load_workflow(path: &str) -> Value {
    let output = Command::new("ruby")
        .arg("-ryaml")
        .arg("-rjson")
        .arg("-e")
        .arg("puts JSON.generate(YAML.load_file(ARGV.fetch(0)))")
        .arg(path)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "failed to parse {path}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

fn workflow_steps<'a>(workflow: &'a Value, job: &str) -> Vec<&'a Value> {
    workflow["jobs"][job]["steps"]
        .as_array()
        .unwrap()
        .iter()
        .collect()
}

fn step_with_id<'a>(steps: &'a [&Value], id: &str) -> &'a Value {
    steps
        .iter()
        .copied()
        .find(|step| step["id"] == id)
        .unwrap_or_else(|| panic!("workflow step with id {id:?} is missing"))
}

#[test]
fn release_version_preparation_updates_only_local_build_metadata() {
    let directory = release_fixture();

    let output = run_prepare_version(directory.path(), "2026.8.18-a1b2c3d4", "69");
    assert!(
        output.status.success(),
        "prepare script failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let manifest = fs::read_to_string(directory.path().join("Cargo.toml")).unwrap();
    let lock = fs::read_to_string(directory.path().join("Cargo.lock")).unwrap();
    let plist = fs::read_to_string(directory.path().join("resources/Info.plist")).unwrap();
    assert!(manifest.contains("version = \"2026.8.18-a1b2c3d4\""));
    assert!(lock.contains("name = \"ipchecker\"\nversion = \"2026.8.18-a1b2c3d4\""));
    assert!(lock.contains("name = \"unrelated\"\nversion = \"1.0.0\""));
    assert!(plist.contains("<key>CFBundleShortVersionString</key>\n<string>2026.8.18</string>"));
    assert!(plist.contains("<key>CFBundleVersion</key>\n<string>69</string>"));
}

#[test]
fn release_version_preparation_makes_numeric_hashes_cargo_safe() {
    let directory = release_fixture();

    let output = run_prepare_version(directory.path(), "2026.8.18-01234567", "70");
    assert!(output.status.success());

    let manifest = fs::read_to_string(directory.path().join("Cargo.toml")).unwrap();
    let lock = fs::read_to_string(directory.path().join("Cargo.lock")).unwrap();
    assert!(manifest.contains("version = \"2026.8.18-g01234567\""));
    assert!(lock.contains("name = \"ipchecker\"\nversion = \"2026.8.18-g01234567\""));
}

#[test]
fn release_version_preparation_rejects_invalid_version_or_build() {
    let directory = release_fixture();

    assert!(
        !run_prepare_version(directory.path(), "2026.08.18-not-a-hash", "69")
            .status
            .success()
    );
    assert!(
        !run_prepare_version(directory.path(), "2026.8.18-a1b2c3d4", "0")
            .status
            .success()
    );
}

#[test]
fn calver_identity_uses_the_timezone_calendar_date_without_leading_zeros() {
    let sha = "a1b2c3d4e5f6789012345678901234567890abcd";
    let (tokyo_next_day, next_day_output) = run_identity("Asia/Tokyo", sha, "1786894200");
    let (tokyo_same_day, same_day_output) = run_identity("Asia/Tokyo", sha, "1786890600");
    let expected_next_day =
        "version=2026.8.17-a1b2c3d4\nbuild_tag=v2026.8.17-a1b2c3d4\nepoch=1786894200\n";
    let expected_same_day =
        "version=2026.8.16-a1b2c3d4\nbuild_tag=v2026.8.16-a1b2c3d4\nepoch=1786890600\n";

    assert!(tokyo_next_day.status.success());
    assert!(tokyo_same_day.status.success());
    assert_eq!(
        String::from_utf8_lossy(&tokyo_next_day.stdout),
        expected_next_day
    );
    assert_eq!(
        String::from_utf8_lossy(&tokyo_same_day.stdout),
        expected_same_day
    );
    assert_eq!(next_day_output, expected_next_day);
    assert_eq!(same_day_output, expected_same_day);
}

#[test]
fn calver_identity_rejects_an_invalid_sha_or_timezone() {
    let sha = "a1b2c3d4e5f6789012345678901234567890abcd";
    assert!(
        !run_identity("Asia/Tokyo", "deadbeef", "1786894200")
            .0
            .status
            .success()
    );
    assert!(
        !run_identity("Not/A_Zone", sha, "1786894200")
            .0
            .status
            .success()
    );
}

#[test]
fn commit_count_reports_the_reachable_history_length() {
    let empty = tempfile::tempdir().unwrap();
    let one = git_repo_with_commits(1);
    let three = git_repo_with_commits(3);

    assert!(!run_commit_count(empty.path()).status.success());
    assert_eq!(
        String::from_utf8_lossy(&run_commit_count(one.path()).stdout).trim(),
        "1"
    );
    assert_eq!(
        String::from_utf8_lossy(&run_commit_count(three.path()).stdout).trim(),
        "3"
    );
}

#[test]
fn daily_workflow_delegates_release_channels_and_uploads_immutable_assets() {
    let workflow = load_workflow(".github/workflows/ci.yml");
    let steps = workflow_steps(&workflow, "release");

    assert!(steps.iter().all(|step| {
        !step["uses"]
            .as_str()
            .is_some_and(|uses| uses.starts_with("./.github/workflows/"))
    }));
    assert!(steps.iter().all(|step| step["name"] != "Verify app bundle"));

    let identity = step_with_id(&steps, "identity");
    let identity_run = identity["run"].as_str().unwrap();
    assert!(identity_run.contains("scripts/calver-identity.sh"));
    assert!(identity_run.contains("Asia/Tokyo"));
    assert!(identity_run.contains("date +%s"));

    let calver = step_with_id(&steps, "calver");
    assert_eq!(calver["uses"], "motoish/calver-release-action@v1");
    assert_eq!(calver["with"]["mode"], "daily");
    assert_eq!(calver["with"]["timezone"], "Asia/Tokyo");
    assert_eq!(calver["with"]["token"], "${{ github.token }}");
    assert_eq!(calver["with"]["now"], "${{ steps.identity.outputs.epoch }}");
    assert_eq!(
        calver["with"]["expected_version"],
        "${{ steps.identity.outputs.version }}"
    );

    let prepare_index = step_index(&steps, "Prepare app version");
    let bundle_index = step_index(&steps, "Bundle app");
    let zip_index = step_index(&steps, "Zip app");
    let manifest_index = step_index(&steps, "Generate update manifest");
    let calver_index = steps
        .iter()
        .position(|step| step["id"] == "calver")
        .unwrap();
    let upload_index = step_index(&steps, "Upload immutable release assets");
    assert!(
        steps
            .iter()
            .all(|step| step["name"] != "Confirm CalVer identity")
    );
    assert!(
        prepare_index < bundle_index
            && bundle_index < zip_index
            && zip_index < manifest_index
            && manifest_index < calver_index
            && calver_index < upload_index
    );

    let prepare = steps[prepare_index];
    let prepare_run = prepare["run"].as_str().unwrap();
    assert!(prepare_run.contains("scripts/commit-count.sh"));
    assert!(!prepare_run.contains("git rev-list"));
    assert!(prepare_run.contains("scripts/prepare-release-version.sh"));
    assert_eq!(
        prepare["env"]["VERSION"],
        "${{ steps.identity.outputs.version }}"
    );

    let manifest = steps[manifest_index];
    assert_eq!(
        manifest["env"]["VERSION"],
        "${{ steps.identity.outputs.version }}"
    );
    assert_eq!(
        manifest["env"]["BUILD_TAG"],
        "${{ steps.identity.outputs.build_tag }}"
    );

    let upload = steps[upload_index];
    assert_eq!(
        upload["env"]["BUILD_TAG"],
        "${{ steps.calver.outputs.build_tag }}"
    );
    let upload_run = upload["run"].as_str().unwrap();
    assert!(upload_run.contains("ipchecker.zip"));
    assert!(upload_run.contains("update.json"));
}

#[test]
fn promotion_workflow_copies_selected_build_assets_to_monthly_stable() {
    let workflow = load_workflow(".github/workflows/promote.yml");
    assert_eq!(workflow["permissions"]["contents"], "write");
    let steps = workflow_steps(&workflow, "promote");

    let download_index = steps
        .iter()
        .position(|step| step["name"] == "Download immutable assets")
        .expect("immutable asset download step");
    let calver_index = steps
        .iter()
        .position(|step| step["id"] == "calver")
        .expect("promotion Action step");
    let upload_index = steps
        .iter()
        .position(|step| step["name"] == "Upload stable assets")
        .expect("stable asset upload step");
    assert!(download_index < calver_index && calver_index < upload_index);

    let download_run = steps[download_index]["run"].as_str().unwrap();
    assert!(download_run.contains("ipchecker.zip"));
    assert!(download_run.contains("update.json"));

    let calver = steps[calver_index];
    assert_eq!(calver["uses"], "motoish/calver-release-action@v1");
    assert_eq!(calver["with"]["mode"], "promote");
    assert_eq!(calver["with"]["source_tag"], "${{ inputs.source_tag }}");
    assert_eq!(calver["with"]["token"], "${{ github.token }}");

    assert_eq!(
        steps[upload_index]["env"]["STABLE_TAG"],
        "${{ steps.calver.outputs.channel_tag }}"
    );
    let upload_run = steps[upload_index]["run"].as_str().unwrap();
    assert!(upload_run.contains("ipchecker.zip"));
    assert!(upload_run.contains("update.json"));
}

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
