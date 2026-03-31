// lib.rs — The crate's library root.
//
// In Rust, `lib.rs` serves two purposes:
//   1. It declares which modules exist (via `mod` statements).
//   2. It re-exports public items so users have a clean API surface.
//
// This is conceptually similar to a Go package's exported symbols, but more
// explicit — you choose exactly what to expose at the crate level.
//
// ## How other crates use this
//
// Other Rust projects add this crate as a dependency:
//
//   [dependencies]
//   github-app-auth = { git = "https://github.com/eculver/github-app-auth" }
//
// And then import types from it:
//
//   use github_app_auth::{App, AccessToken, Install, Error};
//
// Note: Cargo converts hyphens to underscores in `use` statements, so the
// crate `github-app-auth` is imported as `github_app_auth`.

// Declare the modules. Each corresponds to a file: `src/app.rs`, etc.
// The `mod` keyword is similar to Go's package imports but for internal
// organization — it tells the compiler these files are part of this crate.
mod app;
pub mod error;
pub mod types;

// Re-export the main types at the crate root for convenience.
// This lets users write `use github_app_auth::App` instead of
// `use github_app_auth::app::App`.
pub use app::App;
pub use error::Error;
pub use types::{AccessToken, Install};
