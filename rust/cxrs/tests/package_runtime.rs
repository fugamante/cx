use serde_json::Value;
use std::fs;
use std::os::unix::fs::{PermissionsExt, symlink};
use std::path::{Path, PathBuf};
use std::process::Command;

fn copy_file(from: &Path, to: &Path, mode: u32) {
    fs::create_dir_all(to.parent().expect("destination parent")).expect("create parent");
    fs::copy(from, to).expect("copy package file");
    let mut perms = fs::metadata(to).expect("package metadata").permissions();
    perms.set_mode(mode);
    fs::set_permissions(to, perms).expect("set package mode");
}

fn package_prefix() -> (tempfile::TempDir, PathBuf) {
    let temp = tempfile::tempdir().expect("package tempdir");
    let prefix = temp.path().join("prefix");
    let bin = prefix.join("bin");
    fs::create_dir_all(&bin).expect("create package bin");
    copy_file(
        Path::new(env!("CARGO_BIN_EXE_cxrs")),
        &bin.join("xshelf"),
        0o755,
    );
    symlink("xshelf", bin.join("xs")).expect("create xs alias");
    symlink("xshelf", bin.join("cx")).expect("create cx alias");

    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.cx/schemas");
    let schema_dir = prefix.join("share/xshelf/schemas");
    for name in [
        "commitjson.schema.json",
        "diffsum.schema.json",
        "fixrun.schema.json",
        "next.schema.json",
    ] {
        copy_file(&source.join(name), &schema_dir.join(name), 0o644);
    }
    (temp, prefix)
}

fn package_command(prefix: &Path, name: &str, home: &Path) -> Command {
    let mut cmd = Command::new(prefix.join("bin").join(name));
    cmd.env("HOME", home)
        .env("PATH", "/usr/bin:/bin")
        .env_remove("CX_CLI_NAME")
        .env_remove("CX_DATA_DIR")
        .env_remove("CX_REPO_ROOT")
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_INDEX_FILE");
    cmd
}

#[test]
fn package_alias_name() {
    let (_temp, prefix) = package_prefix();
    let home = prefix.join("clean-home");
    fs::create_dir_all(&home).expect("create clean home");
    let out = package_command(&prefix, "cx", &home)
        .args(["schema", "invalid"])
        .output()
        .expect("run cx alias");
    assert_eq!(out.status.code(), Some(2));
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("Usage: cx schema"),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn package_contract_embedded() {
    let (_temp, prefix) = package_prefix();
    let home = prefix.join("clean-home");
    fs::create_dir_all(&home).expect("create clean home");
    let out = package_command(&prefix, "xshelf", &home)
        .args(["contracts", "validate", "--profile", "eval-lab", "--json"])
        .output()
        .expect("validate embedded contract fixture");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let payload: Value = serde_json::from_slice(&out.stdout).expect("contract result json");
    assert_eq!(payload.get("ok").and_then(Value::as_bool), Some(true));
}

#[test]
fn package_schema_clean() {
    let (_temp, prefix) = package_prefix();
    let home = prefix.join("clean-home");
    let work = prefix.join("caller");
    fs::create_dir_all(&home).expect("create clean home");
    fs::create_dir_all(&work).expect("create caller repo");
    Command::new("git")
        .args(["init", "-q"])
        .current_dir(&work)
        .status()
        .expect("initialize caller repo");
    let out = package_command(&prefix, "xshelf", &home)
        .args(["schema", "list", "--json"])
        .current_dir(&work)
        .output()
        .expect("list packaged schemas");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let payload: Value = serde_json::from_slice(&out.stdout).expect("schema result json");
    assert_eq!(payload.get("file_count").and_then(Value::as_u64), Some(4));
    assert_eq!(
        payload.get("schema_dir").and_then(Value::as_str),
        prefix.join("share/xshelf/schemas").to_str()
    );
}

#[test]
fn package_schema_home() {
    let (_temp, prefix) = package_prefix();
    let home = prefix.join("custom-home");
    let schema_dir = home.join(".cx/schemas");
    let work = prefix.join("outside-git");
    fs::create_dir_all(&schema_dir).expect("create home schemas");
    fs::create_dir_all(&work).expect("create outside workdir");
    fs::write(
        schema_dir.join("custom.schema.json"),
        r#"{"$id":"cx://schemas/custom.v1","type":"object"}"#,
    )
    .expect("write custom schema");
    let out = package_command(&prefix, "xshelf", &home)
        .args(["schema", "list", "--json"])
        .current_dir(&work)
        .output()
        .expect("list home schemas");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let payload: Value = serde_json::from_slice(&out.stdout).expect("schema result json");
    assert_eq!(payload.get("file_count").and_then(Value::as_u64), Some(1));
    let actual = Path::new(
        payload
            .get("schema_dir")
            .and_then(Value::as_str)
            .expect("schema directory"),
    )
    .canonicalize()
    .expect("canonical actual schemas");
    assert_eq!(
        actual,
        schema_dir.canonicalize().expect("canonical home schemas")
    );
}

#[test]
fn package_version_caller() {
    let (_temp, prefix) = package_prefix();
    let home = prefix.join("clean-home");
    let work = prefix.join("unrelated-repo");
    fs::create_dir_all(&home).expect("create clean home");
    fs::create_dir_all(&work).expect("create caller repo");
    fs::write(work.join("VERSION"), "9999.99.99\n").expect("write unrelated version");
    Command::new("git")
        .args(["init", "-q"])
        .current_dir(&work)
        .status()
        .expect("initialize caller repo");
    let out = package_command(&prefix, "xshelf", &home)
        .args(["version", "--json"])
        .current_dir(&work)
        .output()
        .expect("run packaged version");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let payload: Value = serde_json::from_slice(&out.stdout).expect("version result json");
    assert_eq!(
        payload.get("version").and_then(Value::as_str),
        Some(include_str!("../../../VERSION").trim())
    );
}

#[test]
fn wrapper_schema_caller() {
    let temp = tempfile::tempdir().expect("wrapper caller tempdir");
    let caller = temp.path().join("caller");
    let home = temp.path().join("home");
    let schema_dir = caller.join(".cx/schemas");
    let target = temp.path().join("target/release");
    fs::create_dir_all(&schema_dir).expect("create caller schemas");
    fs::create_dir_all(&home).expect("create wrapper home");
    fs::create_dir_all(&target).expect("create target dir");
    fs::write(
        schema_dir.join("caller.schema.json"),
        r#"{"$id":"cx://schemas/caller.v1","type":"object"}"#,
    )
    .expect("write caller schema");
    Command::new("git")
        .args(["init", "-q"])
        .current_dir(&caller)
        .status()
        .expect("initialize caller repo");
    symlink(env!("CARGO_BIN_EXE_cxrs"), target.join("cxrs")).expect("link test runtime");

    let repo = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("resolve XSHELF checkout");
    let out = Command::new(repo.join("bin/xshelf"))
        .args(["schema", "list", "--json"])
        .env("HOME", &home)
        .env("CARGO_TARGET_DIR", temp.path().join("target"))
        .current_dir(&caller)
        .output()
        .expect("run checkout wrapper from caller repo");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let payload: Value = serde_json::from_slice(&out.stdout).expect("schema result json");
    assert_eq!(payload.get("file_count").and_then(Value::as_u64), Some(1));
    let actual = Path::new(
        payload
            .get("schema_dir")
            .and_then(Value::as_str)
            .expect("schema directory"),
    )
    .canonicalize()
    .expect("canonical actual schemas");
    assert_eq!(
        actual,
        schema_dir.canonicalize().expect("canonical caller schemas")
    );
}
