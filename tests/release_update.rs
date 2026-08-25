use std::{fs, os::unix::fs::PermissionsExt, path::Path, process::Command};

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

fn run_commit_release_metadata(
    directory: &Path,
    version: &str,
    expected_head: &str,
) -> std::process::Output {
    Command::new("bash")
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("scripts/commit-release-metadata.sh"))
        .arg(version)
        .arg(expected_head)
        .current_dir(directory)
        .output()
        .unwrap()
}

fn run_verify_app_archive(archive: &Path) -> std::process::Output {
    Command::new("bash")
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("scripts/verify-app-archive.sh"))
        .arg(archive)
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

fn release_metadata_repo() -> tempfile::TempDir {
    let directory = tempfile::tempdir().unwrap();
    let work = directory.path().join("work");
    let remote = directory.path().join("remote.git");
    fs::create_dir(&work).unwrap();
    fs::create_dir(&remote).unwrap();
    fs::create_dir(work.join("resources")).unwrap();
    fs::create_dir(work.join("scripts")).unwrap();

    assert!(
        git(&remote, &["init", "--bare", "-b", "main"])
            .status
            .success()
    );
    assert!(git(&work, &["init", "-b", "main"]).status.success());
    fs::write(work.join("Cargo.toml"), "old cargo manifest\n").unwrap();
    fs::write(work.join("Cargo.lock"), "old cargo lock\n").unwrap();
    fs::write(work.join("resources/Info.plist"), "old plist\n").unwrap();
    fs::write(work.join("CHANGELOG.md"), "old changelog\n").unwrap();
    fs::write(work.join("notes.txt"), "old notes\n").unwrap();
    assert!(git(&work, &["add", "."]).status.success());
    assert!(
        git(
            &work,
            &[
                "-c",
                "user.name=ipchecker",
                "-c",
                "user.email=ipchecker@example.test",
                "commit",
                "--no-gpg-sign",
                "-m",
                "initial",
            ],
        )
        .status
        .success()
    );
    assert!(
        git(
            &work,
            &["remote", "add", "origin", remote.to_str().unwrap()]
        )
        .status
        .success()
    );
    assert!(
        git(&work, &["push", "-u", "origin", "main"])
            .status
            .success()
    );

    fs::write(work.join("Cargo.toml"), "new cargo manifest\n").unwrap();
    fs::write(work.join("Cargo.lock"), "new cargo lock\n").unwrap();
    fs::write(work.join("resources/Info.plist"), "new plist\n").unwrap();
    fs::write(work.join("CHANGELOG.md"), "new changelog\n").unwrap();
    fs::write(work.join("notes.txt"), "unrelated local edit\n").unwrap();
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
fn release_metadata_commit_pushes_exactly_the_generated_files() {
    let directory = release_metadata_repo();
    let work = directory.path().join("work");
    let remote = directory.path().join("remote.git");
    let before = git(&work, &["rev-parse", "HEAD"]);
    let expected_head = String::from_utf8_lossy(&before.stdout);

    let output = run_commit_release_metadata(&work, "2026.8.18-a1b2c3d4", expected_head.trim());
    assert!(
        output.status.success(),
        "metadata commit failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let subject = git(&work, &["log", "-1", "--pretty=%s"]);
    assert_eq!(
        String::from_utf8_lossy(&subject.stdout).trim(),
        "chore(release): bump version to 2026.8.18-a1b2c3d4 [skip ci]"
    );
    let changed = git(
        &work,
        &["diff-tree", "--no-commit-id", "--name-only", "-r", "HEAD"],
    );
    assert_eq!(
        String::from_utf8_lossy(&changed.stdout),
        "CHANGELOG.md\nCargo.lock\nCargo.toml\nresources/Info.plist\n"
    );
    let remote_head = git(&remote, &["rev-parse", "refs/heads/main"]);
    let local_head = git(&work, &["rev-parse", "HEAD"]);
    assert_eq!(remote_head.stdout, local_head.stdout);
    let status = git(&work, &["status", "--short"]);
    assert_eq!(String::from_utf8_lossy(&status.stdout), " M notes.txt\n");
}

#[test]
fn release_metadata_commit_rejects_invalid_versions_without_committing() {
    let directory = release_metadata_repo();
    let work = directory.path().join("work");
    let before = git(&work, &["rev-parse", "HEAD"]);
    let expected_head = String::from_utf8_lossy(&before.stdout);

    let output = run_commit_release_metadata(&work, "2026.08.18-latest", expected_head.trim());
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("invalid release version"));
    let after = git(&work, &["rev-parse", "HEAD"]);
    assert_eq!(before.stdout, after.stdout);
}

#[test]
fn release_metadata_commit_refuses_a_dirty_index_without_staging_more_files() {
    let directory = release_metadata_repo();
    let work = directory.path().join("work");
    let before = git(&work, &["rev-parse", "HEAD"]);
    let expected_head = String::from_utf8_lossy(&before.stdout);
    assert!(git(&work, &["add", "notes.txt"]).status.success());

    let output = run_commit_release_metadata(&work, "2026.8.18-a1b2c3d4", expected_head.trim());
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("index must be clean"));
    let staged = git(&work, &["diff", "--cached", "--name-only"]);
    assert_eq!(String::from_utf8_lossy(&staged.stdout), "notes.txt\n");
}

#[test]
fn release_metadata_commit_pushes_only_the_files_that_changed() {
    let directory = release_metadata_repo();
    let work = directory.path().join("work");
    fs::write(work.join("CHANGELOG.md"), "old changelog\n").unwrap();
    let before = git(&work, &["rev-parse", "HEAD"]);
    let expected_head = String::from_utf8_lossy(&before.stdout);

    let output = run_commit_release_metadata(&work, "2026.8.18-a1b2c3d4", expected_head.trim());
    assert!(
        output.status.success(),
        "metadata commit failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let changed = git(
        &work,
        &["diff-tree", "--no-commit-id", "--name-only", "-r", "HEAD"],
    );
    assert_eq!(
        String::from_utf8_lossy(&changed.stdout),
        "Cargo.lock\nCargo.toml\nresources/Info.plist\n"
    );
    let status = git(&work, &["status", "--short"]);
    assert_eq!(String::from_utf8_lossy(&status.stdout), " M notes.txt\n");
}

#[test]
fn release_metadata_commit_skips_when_main_already_has_the_same_files() {
    let directory = release_metadata_repo();
    let work = directory.path().join("work");
    let remote = directory.path().join("remote.git");
    let original = git(&work, &["rev-parse", "HEAD"]);
    let expected_head = String::from_utf8_lossy(&original.stdout).trim().to_string();

    let first = run_commit_release_metadata(&work, "2026.8.18-a1b2c3d4", &expected_head);
    assert!(
        first.status.success(),
        "first metadata commit failed: {}",
        String::from_utf8_lossy(&first.stderr)
    );
    let bumped = git(&work, &["rev-parse", "HEAD"]);

    assert!(
        git(&work, &["reset", "--hard", &expected_head])
            .status
            .success()
    );
    fs::write(work.join("Cargo.toml"), "new cargo manifest\n").unwrap();
    fs::write(work.join("Cargo.lock"), "new cargo lock\n").unwrap();
    fs::write(work.join("resources/Info.plist"), "new plist\n").unwrap();
    fs::write(work.join("CHANGELOG.md"), "new changelog\n").unwrap();
    fs::write(work.join("notes.txt"), "unrelated local edit\n").unwrap();

    let second = run_commit_release_metadata(&work, "2026.8.18-a1b2c3d4", &expected_head);
    assert!(
        second.status.success(),
        "retry metadata commit failed: {}",
        String::from_utf8_lossy(&second.stderr)
    );
    assert!(String::from_utf8_lossy(&second.stderr).contains("release metadata already on main"));
    let remote_head = git(&remote, &["rev-parse", "refs/heads/main"]);
    assert_eq!(remote_head.stdout, bumped.stdout);
    let status = git(&work, &["status", "--short"]);
    assert_eq!(
        String::from_utf8_lossy(&status.stdout),
        " M CHANGELOG.md\n M Cargo.lock\n M Cargo.toml\n M notes.txt\n M resources/Info.plist\n"
    );
}

#[test]
fn release_metadata_commit_refuses_when_main_moved_with_different_files() {
    let directory = release_metadata_repo();
    let work = directory.path().join("work");
    let other = directory.path().join("other");
    let original = git(&work, &["rev-parse", "HEAD"]);
    let expected_head = String::from_utf8_lossy(&original.stdout).trim().to_string();

    let clone = git(
        directory.path(),
        &[
            "clone",
            "-b",
            "main",
            directory.path().join("remote.git").to_str().unwrap(),
            other.to_str().unwrap(),
        ],
    );
    assert!(
        clone.status.success(),
        "clone failed: {}",
        String::from_utf8_lossy(&clone.stderr)
    );
    fs::write(other.join("Cargo.toml"), "unrelated main update\n").unwrap();
    assert!(git(&other, &["add", "Cargo.toml"]).status.success());
    assert!(
        git(
            &other,
            &[
                "-c",
                "user.name=ipchecker",
                "-c",
                "user.email=ipchecker@example.test",
                "commit",
                "--no-gpg-sign",
                "-m",
                "unrelated",
            ],
        )
        .status
        .success()
    );
    let push = git(&other, &["push", "origin", "main"]);
    assert!(
        push.status.success(),
        "push failed: {}",
        String::from_utf8_lossy(&push.stderr)
    );

    let output = run_commit_release_metadata(&work, "2026.8.18-a1b2c3d4", &expected_head);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("main changed during release"));
    let after = git(&work, &["rev-parse", "HEAD"]);
    assert_eq!(original.stdout, after.stdout);
}

#[test]
fn app_archive_verification_requires_the_bundled_executable_and_plist() {
    let directory = tempfile::tempdir().unwrap();
    let app = directory.path().join("ipchecker.app/Contents");
    fs::create_dir_all(app.join("MacOS")).unwrap();
    fs::write(app.join("MacOS/ipchecker"), b"binary").unwrap();
    fs::set_permissions(
        app.join("MacOS/ipchecker"),
        fs::Permissions::from_mode(0o755),
    )
    .unwrap();
    fs::write(
        app.join("Info.plist"),
        br#"<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0"><dict><key>CFBundleIdentifier</key><string>com.tanishi.ipchecker</string></dict></plist>
"#,
    )
    .unwrap();
    let valid = directory.path().join("valid.zip");
    let non_executable = directory.path().join("non-executable.zip");
    let invalid = directory.path().join("invalid.zip");
    assert!(
        Command::new("zip")
            .args(["-q", "-r"])
            .arg(&valid)
            .arg("ipchecker.app")
            .current_dir(directory.path())
            .status()
            .unwrap()
            .success()
    );
    fs::set_permissions(
        app.join("MacOS/ipchecker"),
        fs::Permissions::from_mode(0o644),
    )
    .unwrap();
    assert!(
        Command::new("zip")
            .args(["-q", "-r"])
            .arg(&non_executable)
            .arg("ipchecker.app")
            .current_dir(directory.path())
            .status()
            .unwrap()
            .success()
    );
    fs::write(directory.path().join("readme.txt"), b"missing app").unwrap();
    assert!(
        Command::new("zip")
            .arg("-q")
            .arg(&invalid)
            .arg("readme.txt")
            .current_dir(directory.path())
            .status()
            .unwrap()
            .success()
    );

    assert!(run_verify_app_archive(&valid).status.success());
    assert!(!run_verify_app_archive(&non_executable).status.success());
    assert!(!run_verify_app_archive(&invalid).status.success());
}

#[test]
fn daily_workflow_delegates_release_channels_and_uploads_immutable_assets() {
    let workflow = load_workflow(".github/workflows/ci.yml");
    let steps = workflow_steps(&workflow, "release");

    let release_condition = workflow["jobs"]["release"]["if"].as_str().unwrap();
    assert!(release_condition.contains("github.event_name == 'push'"));
    assert!(release_condition.contains("github.ref == 'refs/heads/main'"));
    assert!(release_condition.contains("!startsWith(github.event.head_commit.message"));
    assert!(release_condition.contains("'chore(release):'"));

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
    let archive_check_index = step_index(&steps, "Verify app archive");
    let manifest_index = step_index(&steps, "Generate update manifest");
    let changelog_index = step_index(&steps, "Generate CHANGELOG.md");
    let release_range_index = step_index(&steps, "Resolve release notes range");
    let release_notes_index = step_index(&steps, "Generate release notes");
    let release_notes_check_index = step_index(&steps, "Verify release notes scope");
    let commit_index = step_index(&steps, "Commit release metadata");
    let calver_index = steps
        .iter()
        .position(|step| step["id"] == "calver")
        .unwrap();
    let update_notes_index = step_index(&steps, "Update immutable release notes");
    let upload_index = step_index(&steps, "Upload immutable release assets");
    assert!(steps.iter().all(|step| step["name"] != "Save Rust cache"));
    assert!(
        steps
            .iter()
            .all(|step| step["name"] != "Confirm main is unchanged")
    );
    assert!(
        steps
            .iter()
            .all(|step| step["name"] != "Confirm CalVer identity")
    );
    assert!(
        prepare_index < bundle_index
            && bundle_index < zip_index
            && zip_index < archive_check_index
            && archive_check_index < manifest_index
            && manifest_index < changelog_index
            && changelog_index < release_range_index
            && release_range_index < release_notes_index
            && release_notes_index < release_notes_check_index
            && release_notes_check_index < commit_index
            && commit_index < calver_index
            && calver_index < update_notes_index
            && update_notes_index < upload_index
    );

    assert_eq!(
        steps[archive_check_index]["run"],
        "bash scripts/verify-app-archive.sh ipchecker.zip"
    );

    let changelog = steps[changelog_index];
    assert_eq!(changelog["uses"], "orhun/git-cliff-action@v4");
    assert_eq!(changelog["env"]["OUTPUT"], "CHANGELOG.md");
    assert_eq!(
        changelog["with"]["args"],
        "--tag ${{ steps.identity.outputs.build_tag }}"
    );

    let release_range = steps[release_range_index];
    assert_eq!(release_range["id"], "release_range");
    let release_range_run = release_range["run"].as_str().unwrap();
    assert!(release_range_run.contains("git describe --tags"));
    assert!(release_range_run.contains("$GITHUB_SHA^"));
    assert!(release_range_run.contains("range=\"$previous_tag..$GITHUB_SHA\""));
    assert!(release_range_run.contains("range=\"$GITHUB_SHA\""));
    assert!(release_range_run.contains("$GITHUB_OUTPUT"));

    let release_notes = steps[release_notes_index];
    assert_eq!(release_notes["uses"], "orhun/git-cliff-action@v4");
    assert_eq!(release_notes["env"]["OUTPUT"], "release-notes.md");
    assert_eq!(
        release_notes["with"]["args"],
        "--tag ${{ steps.identity.outputs.build_tag }} ${{ steps.release_range.outputs.range }}"
    );

    let release_notes_check = steps[release_notes_check_index];
    assert_eq!(
        release_notes_check["env"]["VERSION"],
        "${{ steps.identity.outputs.version }}"
    );
    let release_notes_check_run = release_notes_check["run"].as_str().unwrap();
    assert!(release_notes_check_run.contains("test -s release-notes.md"));
    assert!(release_notes_check_run.contains("grep -c '^## ' release-notes.md"));
    assert!(release_notes_check_run.contains("grep -Fqx \"## $VERSION\" release-notes.md"));

    let commit = steps[commit_index];
    assert_eq!(
        commit["env"]["VERSION"],
        "${{ steps.identity.outputs.version }}"
    );
    assert_eq!(commit["env"]["EXPECTED_HEAD"], "${{ github.sha }}");
    assert_eq!(
        commit["run"],
        "bash scripts/commit-release-metadata.sh \"$VERSION\" \"$EXPECTED_HEAD\""
    );

    let update_notes = steps[update_notes_index];
    assert_eq!(
        update_notes["env"]["BUILD_TAG"],
        "${{ steps.calver.outputs.build_tag }}"
    );
    let update_notes_run = update_notes["run"].as_str().unwrap();
    assert!(update_notes_run.contains("gh release edit"));
    assert!(update_notes_run.contains("--repo \"$GITHUB_REPOSITORY\""));
    assert!(update_notes_run.contains("--notes-file release-notes.md"));

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

    let checkout_index = steps
        .iter()
        .position(|step| step["name"] == "Checkout repository")
        .expect("checkout step");

    let download_index = steps
        .iter()
        .position(|step| step["name"] == "Download immutable assets")
        .expect("immutable asset download step");
    let calver_index = steps
        .iter()
        .position(|step| step["id"] == "calver")
        .expect("promotion Action step");
    let preflight_index = steps
        .iter()
        .position(|step| step["name"] == "Verify immutable assets")
        .expect("immutable asset verification step");
    let stable_range_index = steps
        .iter()
        .position(|step| step["name"] == "Resolve stable release notes range")
        .expect("stable Release Notes range step");
    let stable_notes_index = steps
        .iter()
        .position(|step| step["name"] == "Generate stable release notes")
        .expect("stable Release Notes generation step");
    let stable_notes_check_index = steps
        .iter()
        .position(|step| step["name"] == "Verify stable release notes scope")
        .expect("stable Release Notes verification step");
    let upload_index = steps
        .iter()
        .position(|step| step["name"] == "Upload stable assets")
        .expect("stable asset upload step");
    let notes_update_index = steps
        .iter()
        .position(|step| step["name"] == "Update stable release notes")
        .expect("stable Release Notes update step");
    let verify_index = steps
        .iter()
        .position(|step| step["name"] == "Verify stable assets")
        .expect("stable asset verification step");
    assert!(
        checkout_index < download_index
            && download_index < preflight_index
            && preflight_index < calver_index
            && calver_index < stable_range_index
            && stable_range_index < stable_notes_index
            && stable_notes_index < stable_notes_check_index
            && stable_notes_check_index < notes_update_index
            && notes_update_index < upload_index
            && upload_index < verify_index
    );

    let checkout = steps[checkout_index];
    assert_eq!(checkout["with"]["fetch-depth"], 0);
    assert_eq!(checkout["with"]["fetch-tags"], true);

    let download_run = steps[download_index]["run"].as_str().unwrap();
    assert!(download_run.contains("ipchecker.zip"));
    assert!(download_run.contains("update.json"));
    assert!(download_run.contains("--repo \"$GITHUB_REPOSITORY\""));

    let preflight_run = steps[preflight_index]["run"].as_str().unwrap();
    assert!(preflight_run.contains("test -s promoted/ipchecker.zip"));
    assert!(preflight_run.contains("test -s promoted/update.json"));
    assert!(preflight_run.contains("scripts/verify-app-archive.sh"));
    assert!(preflight_run.contains("jq -e"));

    let calver = steps[calver_index];
    assert_eq!(calver["uses"], "motoish/calver-release-action@v1");
    assert_eq!(calver["with"]["mode"], "promote");
    assert_eq!(calver["with"]["source_tag"], "${{ inputs.source_tag }}");
    assert_eq!(calver["with"]["token"], "${{ github.token }}");

    let stable_range = steps[stable_range_index];
    assert_eq!(stable_range["id"], "stable_range");
    assert_eq!(
        stable_range["env"]["SOURCE_TAG"],
        "${{ inputs.source_tag }}"
    );
    let stable_range_run = stable_range["run"].as_str().unwrap();
    assert!(stable_range_run.contains("git rev-list -n 1 \"$SOURCE_TAG\""));
    assert!(stable_range_run.contains("git tag --merged \"$source_commit^\""));
    assert!(stable_range_run.contains("^v20[0-9]{2}\\.([1-9]|1[0-2])$"));
    assert!(stable_range_run.contains("range=\"$previous_stable..$source_commit\""));
    assert!(stable_range_run.contains("range=\"$source_commit\""));
    assert!(stable_range_run.contains("$GITHUB_OUTPUT"));

    let stable_notes = steps[stable_notes_index];
    assert_eq!(stable_notes["uses"], "orhun/git-cliff-action@v4");
    assert_eq!(stable_notes["with"]["config"], "cliff.toml");
    let stable_notes_args = stable_notes["with"]["args"].as_str().unwrap();
    assert!(stable_notes_args.contains("--ignore-tags '.*'"));
    assert!(stable_notes_args.contains("--tag ${{ steps.calver.outputs.channel_tag }}"));
    assert!(stable_notes_args.contains("${{ steps.stable_range.outputs.range }}"));
    assert_eq!(stable_notes["env"]["OUTPUT"], "promoted/release-notes.md");

    let stable_notes_check = steps[stable_notes_check_index];
    assert_eq!(
        stable_notes_check["env"]["STABLE_TAG"],
        "${{ steps.calver.outputs.channel_tag }}"
    );
    let stable_notes_check_run = stable_notes_check["run"].as_str().unwrap();
    assert!(stable_notes_check_run.contains("test -s promoted/release-notes.md"));
    assert!(stable_notes_check_run.contains("grep -c '^## '"));
    assert!(stable_notes_check_run.contains("expected_heading=\"## ${STABLE_TAG#v}\""));

    let notes_update = steps[notes_update_index];
    assert_eq!(
        notes_update["env"]["STABLE_TAG"],
        "${{ steps.calver.outputs.channel_tag }}"
    );
    let notes_update_run = notes_update["run"].as_str().unwrap();
    assert!(notes_update_run.contains("gh release edit"));
    assert!(notes_update_run.contains("--repo \"$GITHUB_REPOSITORY\""));
    assert!(notes_update_run.contains("--notes-file promoted/release-notes.md"));

    assert_eq!(
        steps[upload_index]["env"]["STABLE_TAG"],
        "${{ steps.calver.outputs.channel_tag }}"
    );
    let upload_run = steps[upload_index]["run"].as_str().unwrap();
    assert!(upload_run.contains("ipchecker.zip"));
    assert!(upload_run.contains("update.json"));
    assert!(upload_run.contains("--repo \"$GITHUB_REPOSITORY\""));

    let verify = steps[verify_index];
    assert_eq!(
        verify["env"]["STABLE_TAG"],
        "${{ steps.calver.outputs.channel_tag }}"
    );
    let verify_run = verify["run"].as_str().unwrap();
    assert!(verify_run.contains("gh release view"));
    assert!(verify_run.contains("ipchecker.zip"));
    assert!(verify_run.contains("update.json"));
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
