// build.rs — A build script that runs before compilation.
//
// In Rust, build.rs is a convention: Cargo automatically compiles and runs this
// file before building the rest of the crate. It's commonly used to:
//   - Generate code
//   - Set compile-time environment variables
//   - Link native libraries
//
// Here we use it to bake the version string (from `git describe`) into the
// binary at compile time, so `github-app --version` shows the git tag.

fn main() {
    // Run `git describe` to get a version like "v0.1.0" or "v0.1.0-3-gabcdef"
    // or "v0.1.0-dev" if the working tree is dirty.
    let version = std::process::Command::new("git")
        .args(["describe", "--tags", "--always", "--dirty=-dev"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        // Fall back to the version in Cargo.toml if git isn't available
        // (e.g., when building from a published crate without git history).
        .unwrap_or_else(|| env!("CARGO_PKG_VERSION").to_string());

    // This makes the version available as env!("GITHUB_APP_AUTH_VERSION")
    // in the Rust source code at compile time.
    println!("cargo:rustc-env=GITHUB_APP_AUTH_VERSION={version}");

    // Only re-run this script if the git HEAD changes (new commits/tags).
    println!("cargo:rerun-if-changed=.git/HEAD");
}
