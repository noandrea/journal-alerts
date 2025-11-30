use std::process::Command;

fn main() {
    // Get the git commit hash
    let git_hash = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        .ok()
        .and_then(|output| {
            if output.status.success() {
                String::from_utf8(output.stdout).ok()
            } else {
                None
            }
        })
        .unwrap_or_else(|| String::from("unknown"));

    let git_hash = git_hash.trim();

    // Set GIT_HASH environment variable for use in the binary
    println!("cargo:rustc-env=GIT_HASH={}", git_hash);

    // Rerun build script if git HEAD changes
    println!("cargo:rerun-if-changed=.git/HEAD");
    println!("cargo:rerun-if-changed=.git/refs/heads");
}
