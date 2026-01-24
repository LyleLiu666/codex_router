use std::env;
use std::process::Command;

#[test]
fn package_script_dry_run_prints_steps() {
    let repo_root = env::current_dir().expect("current dir");
    let script = repo_root.join("scripts").join("package-dmg.sh");

    let output = Command::new("bash")
        .arg(script)
        .env("DRY_RUN", "1")
        .env("SKIP_BUILD", "1")
        .output()
        .expect("run packaging script");

    assert!(
        output.status.success(),
        "script failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("create-dmg") || stdout.contains("hdiutil create"),
        "missing DMG step"
    );
}
