// error.rs — Defines the crate's error types.
//
// In Rust, the conventional way to handle errors in a library is:
//   1. Define an enum with a variant for each kind of error.
//   2. Derive `thiserror::Error` to auto-generate `Display` and `std::error::Error` impls.
//   3. Use `#[from]` to enable automatic conversion from underlying errors via the `?` operator.
//
// This is analogous to Go's `fmt.Errorf("...: %w", err)` wrapping pattern, but
// the Rust compiler enforces that every error is handled — you can't accidentally
// ignore an error like you can in Go.
//
// Users of this library can match on specific error variants:
//
//   match result {
//       Err(github_app_auth::Error::Api { status, .. }) if status == 401 => { ... }
//       Err(e) => { ... }
//       Ok(token) => { ... }
//   }

/// The error type for all operations in this crate.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Failed to parse the PEM-encoded private key.
    #[error("failed to parse private key: {0}")]
    PrivateKey(String),

    /// Failed to create or sign a JWT.
    #[error("failed to generate JWT: {0}")]
    Jwt(#[from] jsonwebtoken::errors::Error),

    /// An HTTP-level error (connection refused, timeout, DNS failure, etc.).
    #[error("HTTP request failed: {0}")]
    Http(#[from] reqwest::Error),

    /// The GitHub API returned a non-2xx status code.
    #[error("GitHub API error ({status}): {body}")]
    Api {
        /// The HTTP status code returned by GitHub.
        status: u16,
        /// The response body, which usually contains a JSON error message.
        body: String,
    },

    /// Failed to parse the JSON response from GitHub.
    #[error("failed to parse response: {0}")]
    Json(#[from] serde_json::Error),

    /// Failed to read a file (e.g., the private key file).
    #[error("failed to read file: {0}")]
    Io(#[from] std::io::Error),
}
