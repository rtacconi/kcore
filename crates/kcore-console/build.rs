//! Embeds an optional VCS revision when `KCORE_GIT_REV` is set in the build environment.
fn main() {
    if let Ok(rev) = std::env::var("KCORE_GIT_REV") {
        if !rev.trim().is_empty() {
            println!("cargo:rustc-env=KCORE_GIT_REV={rev}");
            return;
        }
    }
    if let Some(r) = std::process::Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
    {
        println!("cargo:rustc-env=KCORE_GIT_REV={r}");
    }
}
