mod common;

use common::*;
use serde_json::Value;
use std::fs;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::sync::{Mutex, MutexGuard, OnceLock};

fn suite_lock() -> MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
        .lock()
        .expect("http auth test lock")
}

#[test]
fn basic_auth_cov() {
    let _guard = suite_lock();
    if std::process::Command::new("curl")
        .arg("--version")
        .output()
        .is_err()
    {
        return;
    }
    let repo = TempRepo::new("cxrs-it");
    let (url, captured, handle) = run_fixture_http_server_once(r#"{"text":"basic-auth-ok"}"#);
    let out = repo.run_with_env(
        &["cxo", "echo", "http-basic"],
        &[
            ("CX_PROVIDER_ADAPTER", "http-curl"),
            ("CX_HTTP_PROVIDER_URL", &url),
            ("CX_HTTP_AUTH_PROFILE", "basic"),
            ("CX_HTTP_AUTH_USERNAME", "alice"),
            ("CX_HTTP_AUTH_PASSWORD", "secret"),
        ],
    );
    assert!(
        out.status.success(),
        "stdout={} stderr={}",
        stdout_str(&out),
        stderr_str(&out)
    );
    assert_eq!(stdout_str(&out).trim(), "basic-auth-ok");
    handle.join().expect("fixture join");

    let req = captured
        .lock()
        .expect("fixture lock")
        .clone()
        .expect("captured request");
    assert_eq!(req.authorization.as_deref(), Some("Basic YWxpY2U6c2VjcmV0"));
}

#[test]
fn header_auth_cov() {
    let _guard = suite_lock();
    if std::process::Command::new("curl")
        .arg("--version")
        .output()
        .is_err()
    {
        return;
    }
    let repo = TempRepo::new("cxrs-it");
    let (url, captured, handle) = run_fixture_http_server_once(r#"{"text":"header-auth-ok"}"#);
    let out = repo.run_with_env(
        &["cxo", "echo", "http-header"],
        &[
            ("CX_PROVIDER_ADAPTER", "http-curl"),
            ("CX_HTTP_PROVIDER_URL", &url),
            ("CX_HTTP_AUTH_PROFILE", "header"),
            ("CX_HTTP_AUTH_HEADER", "X-API-Key"),
            ("CX_HTTP_AUTH_VALUE", "key-123"),
        ],
    );
    assert!(
        out.status.success(),
        "stdout={} stderr={}",
        stdout_str(&out),
        stderr_str(&out)
    );
    assert_eq!(stdout_str(&out).trim(), "header-auth-ok");
    handle.join().expect("fixture join");

    let req = captured
        .lock()
        .expect("fixture lock")
        .clone()
        .expect("captured request");
    assert_eq!(
        req.headers.get("x-api-key").map(String::as_str),
        Some("key-123")
    );
}

#[test]
fn bearer_file_cov() {
    let _guard = suite_lock();
    if std::process::Command::new("curl")
        .arg("--version")
        .output()
        .is_err()
    {
        return;
    }
    let repo = TempRepo::new("cxrs-it");
    let secret = repo.root.join("token.secret");
    fs::write(&secret, "token-file-123\n").expect("write secret");
    #[cfg(unix)]
    {
        let mut perms = fs::metadata(&secret)
            .expect("secret metadata")
            .permissions();
        perms.set_mode(0o600);
        fs::set_permissions(&secret, perms).expect("set secret perms");
    }
    let (url, captured, handle) = run_fixture_http_server_once(r#"{"text":"bearer-file-ok"}"#);
    let out = repo.run_with_env(
        &["cxo", "echo", "http-bearer-file"],
        &[
            ("CX_PROVIDER_ADAPTER", "http-curl"),
            ("CX_HTTP_PROVIDER_URL", &url),
            (
                "CX_HTTP_PROVIDER_TOKEN_FILE",
                secret.to_str().expect("secret path"),
            ),
        ],
    );
    assert!(
        out.status.success(),
        "stdout={} stderr={}",
        stdout_str(&out),
        stderr_str(&out)
    );
    assert_eq!(stdout_str(&out).trim(), "bearer-file-ok");
    handle.join().expect("fixture join");

    let req = captured
        .lock()
        .expect("fixture lock")
        .clone()
        .expect("captured request");
    assert_eq!(req.authorization.as_deref(), Some("Bearer token-file-123"));
}

#[test]
fn secret_perm_cov() {
    let _guard = suite_lock();
    let repo = TempRepo::new("cxrs-it");
    let secret = repo.root.join("token-open.secret");
    fs::write(&secret, "token-open-123\n").expect("write secret");
    #[cfg(unix)]
    {
        let mut perms = fs::metadata(&secret)
            .expect("secret metadata")
            .permissions();
        perms.set_mode(0o644);
        fs::set_permissions(&secret, perms).expect("set secret perms");
    }
    let out = repo.run_with_env(
        &["cxo", "echo", "http-bearer-open"],
        &[
            ("CX_PROVIDER_ADAPTER", "http-curl"),
            ("CX_HTTP_PROVIDER_URL", "https://api.example.test/infer"),
            (
                "CX_HTTP_PROVIDER_TOKEN_FILE",
                secret.to_str().expect("secret path"),
            ),
        ],
    );
    #[cfg(unix)]
    {
        assert_eq!(out.status.code(), Some(1), "stderr={}", stderr_str(&out));
        assert!(
            stderr_str(&out).contains("must not be group/world readable or writable"),
            "stderr={}",
            stderr_str(&out)
        );
    }
    #[cfg(not(unix))]
    {
        let _ = out;
    }
}

#[test]
fn core_auth_mode() {
    let repo = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .find(|path| path.join(".cx").join("schemas").is_dir())
        .expect("repo root")
        .to_path_buf();
    let out = std::process::Command::new(repo.join("bin").join("cx"))
        .args(["core", "--json"])
        .env("CX_PROVIDER_ADAPTER", "http-curl")
        .env("CX_HTTP_PROVIDER_URL", "https://api.example.test/infer")
        .env("CX_HTTP_AUTH_PROFILE", "header")
        .env("CX_HTTP_AUTH_HEADER", "X-API-Key")
        .current_dir(&repo)
        .output()
        .expect("run core --json");

    assert!(
        out.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let payload: Value = serde_json::from_slice(&out.stdout).expect("parse core json");
    let provider = payload.get("provider").expect("provider object");
    assert_eq!(
        provider.get("http_auth_mode").and_then(Value::as_str),
        Some("header")
    );
    assert_eq!(
        provider.get("http_auth_header").and_then(Value::as_str),
        Some("X-API-Key")
    );
}

#[test]
fn core_auth_src() {
    let repo = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .find(|path| path.join(".cx").join("schemas").is_dir())
        .expect("repo root")
        .to_path_buf();
    let secret = repo.join(".tmp-http-token");
    fs::write(&secret, "secret-file-123\n").expect("write secret file");
    #[cfg(unix)]
    {
        let mut perms = fs::metadata(&secret)
            .expect("secret metadata")
            .permissions();
        perms.set_mode(0o600);
        fs::set_permissions(&secret, perms).expect("set secret perms");
    }
    let out = std::process::Command::new(repo.join("bin").join("cx"))
        .args(["core", "--json"])
        .env("CX_PROVIDER_ADAPTER", "http-curl")
        .env("CX_HTTP_PROVIDER_URL", "https://api.example.test/infer")
        .env(
            "CX_HTTP_PROVIDER_TOKEN_FILE",
            secret.to_str().expect("secret path"),
        )
        .current_dir(&repo)
        .output()
        .expect("run core --json");
    let _ = fs::remove_file(&secret);

    assert!(
        out.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let payload: Value = serde_json::from_slice(&out.stdout).expect("parse core json");
    let provider = payload.get("provider").expect("provider object");
    assert_eq!(
        provider
            .get("http_auth_secret_source")
            .and_then(Value::as_str),
        Some("file")
    );
}
