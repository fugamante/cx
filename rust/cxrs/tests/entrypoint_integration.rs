use std::path::PathBuf;
use std::process::Command;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
}

#[test]
fn bin_cx_version_reports_runtime() {
    let repo = repo_root();
    let out = Command::new(repo.join("bin").join("cx"))
        .arg("version")
        .current_dir(&repo)
        .output()
        .expect("run bin/cx version");

    assert!(
        out.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("execution_path:"), "{stdout}");
}

#[test]
fn lib_cx_sh_is_sourceable_and_exports_functions() {
    let repo = repo_root();
    let script = format!(
        "source '{}' >/dev/null 2>&1; declare -F cx >/dev/null && declare -F cxversion >/dev/null",
        repo.join("lib").join("cx.sh").display()
    );
    let out = Command::new("bash")
        .arg("-lc")
        .arg(script)
        .current_dir(&repo)
        .output()
        .expect("source lib/cx.sh");

    assert!(
        out.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}
