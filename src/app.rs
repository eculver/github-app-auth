// app.rs — Core GitHub App authentication logic.
//
// This module provides the `App` struct, which is the main entry point for
// library users. It mirrors the Go `pkg/app` package.
//
// ## Design patterns used here
//
// **Builder-style construction** (the `with_*` methods):
//   In Go, the original code used the "functional options" pattern:
//     app.New(id, key, app.WithBaseURL(url))
//
//   In Rust, the idiomatic equivalent is the "consuming builder" pattern:
//     App::new(id, key)?.with_base_url(url)
//
//   Each `with_*` method takes `self` by value (consuming it) and returns `Self`,
//   enabling method chaining. This is safe because Rust's ownership system
//   ensures only one reference exists at a time.
//
// **Mutable self for caching** (`&mut self`):
//   Methods like `generate_jwt()` take `&mut self` because they cache the JWT
//   on the struct. In Go, this was possible with a pointer receiver. In Rust,
//   `&mut self` is the equivalent — it gives exclusive mutable access.

use chrono::{DateTime, Duration, Utc};
use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
use log::debug;
use serde::Serialize;

use crate::error::Error;
use crate::types::{AccessToken, Install};

/// Default GitHub API base URL.
const DEFAULT_BASE_URL: &str = "https://api.github.com";

/// The Accept header value for GitHub App API endpoints.
const GITHUB_ACCEPT_HEADER: &str = "application/vnd.github+json";

/// The `User-Agent` header value sent with every GitHub API request.
///
/// GitHub requires a `User-Agent` header on all API requests and rejects
/// requests without one. See:
/// <https://docs.github.com/en/rest/using-the-rest-api/getting-started-with-the-rest-api#user-agent>
const USER_AGENT: &str = concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION"));

/// JWT claims for GitHub App authentication.
///
/// These are the standard claims required by GitHub when authenticating as a
/// GitHub App. The `iss` (issuer) claim is the App ID.
///
/// See: <https://docs.github.com/en/apps/creating-github-apps/authenticating-with-a-github-app/generating-a-json-web-token-jwt-for-a-github-app>
#[derive(Debug, Serialize)]
struct Claims {
    /// Issued-at time (Unix timestamp).
    iat: i64,
    /// Expiration time (Unix timestamp).
    exp: i64,
    /// Issuer — the GitHub App ID.
    iss: String,
}

/// A GitHub App that can generate JWTs and access tokens.
///
/// This is the primary type for interacting with GitHub App authentication.
/// Create one with [`App::new`], then call methods to generate JWTs, access
/// tokens, or query installations.
///
/// # Example
///
/// ```no_run
/// use github_app_auth::App;
///
/// let key = std::fs::read("private-key.pem").unwrap();
/// let mut app = App::new("12345", key).unwrap();
///
/// // Generate a short-lived access token for an installation.
/// let token = app.generate_access_token("67890").unwrap();
/// println!("Token: {}", token.token);
/// ```
pub struct App {
    /// The GitHub App ID.
    id: String,

    /// The PEM-encoded RSA private key, pre-parsed into an encoding key.
    encoding_key: EncodingKey,

    /// The GitHub API base URL (default: `https://api.github.com`).
    base_url: String,

    /// The HTTP client used for API requests.
    client: reqwest::blocking::Client,

    /// Cached JWT string.
    jwt: Option<String>,

    /// Expiration time of the cached JWT.
    jwt_exp: Option<DateTime<Utc>>,
}

impl App {
    /// Create a new `App` with the given GitHub App ID and PEM-encoded private key.
    ///
    /// The private key is parsed immediately, so any PEM format errors are
    /// caught upfront rather than at JWT generation time.
    ///
    /// # Errors
    ///
    /// Returns [`Error::PrivateKey`] if the PEM data cannot be parsed as an
    /// RSA private key.
    pub fn new(id: impl Into<String>, private_key: impl AsRef<[u8]>) -> Result<Self, Error> {
        // EncodingKey::from_rsa_pem parses the PEM-encoded key and validates
        // its structure. We do this once at construction time.
        let encoding_key = EncodingKey::from_rsa_pem(private_key.as_ref())
            .map_err(|e| Error::PrivateKey(e.to_string()))?;

        Ok(Self {
            id: id.into(),
            encoding_key,
            base_url: DEFAULT_BASE_URL.to_string(),
            client: reqwest::blocking::Client::builder()
                .user_agent(USER_AGENT)
                .build()?,
            jwt: None,
            jwt_exp: None,
        })
    }

    /// Override the GitHub API base URL.
    ///
    /// This is primarily useful for testing (pointing at a mock server) or
    /// for GitHub Enterprise Server installations.
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use github_app_auth::App;
    /// # let key = std::fs::read("key.pem").unwrap();
    /// let app = App::new("123", key)?
    ///     .with_base_url("https://github.example.com/api/v3");
    /// # Ok::<(), github_app_auth::Error>(())
    /// ```
    pub fn with_base_url(mut self, url: impl Into<String>) -> Self {
        self.base_url = url.into();
        self
    }

    /// Pre-set a JWT and its expiration time.
    ///
    /// This is primarily useful for testing, where you want deterministic
    /// tokens rather than generating real signed JWTs.
    pub fn with_jwt(mut self, jwt: impl Into<String>, exp: DateTime<Utc>) -> Self {
        self.jwt = Some(jwt.into());
        self.jwt_exp = Some(exp);
        self
    }

    /// Returns the app ID.
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Generate a signed JWT for this GitHub App.
    ///
    /// The JWT is signed with RS256 using the app's private key and is valid
    /// for 10 minutes (minus a 30-second buffer for clock skew). The result is
    /// cached — subsequent calls return the cached JWT until it expires.
    ///
    /// This JWT is used as a `Bearer` token to authenticate as the app itself
    /// (not as an installation). It's required for operations like listing
    /// installations or generating installation access tokens.
    ///
    /// See: <https://docs.github.com/en/apps/creating-github-apps/authenticating-with-a-github-app/generating-a-json-web-token-jwt-for-a-github-app>
    ///
    /// # Errors
    ///
    /// Returns [`Error::Jwt`] if signing fails.
    pub fn generate_jwt(&mut self) -> Result<String, Error> {
        let now = Utc::now();

        // Return the cached JWT if it's still valid.
        if let (Some(ref jwt), Some(exp)) = (&self.jwt, self.jwt_exp) {
            if exp > now {
                debug!("using cached JWT (expires at {})", exp);
                return Ok(jwt.clone());
            }
            debug!("cached JWT expired at {}, generating new one", exp);
        }

        // Max expiration is 10 minutes. Subtract 30 seconds to account for
        // clock skew between our machine and GitHub's servers.
        let exp = now + Duration::minutes(10) - Duration::seconds(30);

        let claims = Claims {
            iat: now.timestamp(),
            exp: exp.timestamp(),
            iss: self.id.clone(),
        };

        let header = Header::new(Algorithm::RS256);
        let token = encode(&header, &claims, &self.encoding_key)?;

        debug!("generated new JWT (expires at {})", exp);

        // Cache the JWT for subsequent calls.
        self.jwt = Some(token.clone());
        self.jwt_exp = Some(exp);

        Ok(token)
    }

    /// Generate a short-lived access token for a specific installation.
    ///
    /// This calls the GitHub API to exchange a JWT for an installation access
    /// token. The returned token is typically valid for 1 hour and scoped to
    /// the permissions of the installation.
    ///
    /// See: <https://docs.github.com/en/apps/creating-github-apps/authenticating-with-a-github-app/generating-an-installation-access-token-for-a-github-app>
    ///
    /// # Errors
    ///
    /// Returns an error if JWT generation fails, the HTTP request fails, or
    /// the GitHub API returns a non-2xx response.
    pub fn generate_access_token(
        &mut self,
        installation_id: &str,
    ) -> Result<AccessToken, Error> {
        let jwt = self.generate_jwt()?;

        let url = format!(
            "{}/app/installations/{}/access_tokens",
            self.base_url, installation_id
        );

        debug!("POST {}", url);

        let resp = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {jwt}"))
            .header("Accept", GITHUB_ACCEPT_HEADER)
            .send()?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().unwrap_or_default();
            return Err(Error::Api {
                status: status.as_u16(),
                body,
            });
        }

        let token: AccessToken = resp.json()?;
        debug!(
            "generated access token (expires at {})",
            token.expires_at
        );

        Ok(token)
    }

    /// List all installations for this GitHub App.
    ///
    /// Returns every organization or user account where the app is installed.
    ///
    /// See: <https://docs.github.com/en/rest/apps/apps#list-installations-for-the-authenticated-app>
    ///
    /// # Errors
    ///
    /// Returns an error if JWT generation fails, the HTTP request fails, or
    /// the GitHub API returns a non-2xx response.
    pub fn list_installs(&mut self) -> Result<Vec<Install>, Error> {
        let jwt = self.generate_jwt()?;

        let url = format!("{}/app/installations", self.base_url);

        debug!("GET {}", url);

        let resp = self
            .client
            .get(&url)
            .header("Authorization", format!("Bearer {jwt}"))
            .header("Accept", GITHUB_ACCEPT_HEADER)
            .send()?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().unwrap_or_default();
            return Err(Error::Api {
                status: status.as_u16(),
                body,
            });
        }

        let installs: Vec<Install> = resp.json()?;
        debug!("found {} installations", installs.len());

        Ok(installs)
    }

    /// Get the installation for a specific repository.
    ///
    /// Looks up the app's installation on a particular `owner/repository`
    /// combination.
    ///
    /// See: <https://docs.github.com/en/rest/apps/apps#get-a-repository-installation-for-the-authenticated-app>
    ///
    /// # Errors
    ///
    /// Returns an error if JWT generation fails, the HTTP request fails, or
    /// the GitHub API returns a non-2xx response (e.g., 404 if the app is not
    /// installed on that repository).
    pub fn get_install(&mut self, owner: &str, repo: &str) -> Result<Install, Error> {
        let jwt = self.generate_jwt()?;

        let url = format!(
            "{}/repos/{}/{}/installation",
            self.base_url, owner, repo
        );

        debug!("GET {}", url);

        let resp = self
            .client
            .get(&url)
            .header("Authorization", format!("Bearer {jwt}"))
            .header("Accept", GITHUB_ACCEPT_HEADER)
            .send()?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().unwrap_or_default();
            return Err(Error::Api {
                status: status.as_u16(),
                body,
            });
        }

        let install: Install = resp.json()?;
        debug!("found installation {} for {}/{}", install.id, owner, repo);

        Ok(install)
    }
}

// =============================================================================
// Tests
// =============================================================================
//
// In Rust, unit tests live in the same file as the code they test, inside a
// `#[cfg(test)] mod tests { ... }` block. The `#[cfg(test)]` attribute means
// this module is only compiled when running `cargo test` — it's completely
// stripped from release builds.
//
// This is different from Go, where tests live in separate `_test.go` files.
// The Rust convention keeps tests close to the code, which makes it easy to
// test private functions and keeps related code together.

#[cfg(test)]
mod tests {
    use super::*;

    /// Load the test private key from the fixtures directory.
    fn test_private_key() -> Vec<u8> {
        std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/test_private_key.pem"
        ))
        .expect("failed to read test private key")
    }

    // -------------------------------------------------------------------------
    // generate_jwt tests
    // -------------------------------------------------------------------------

    #[test]
    fn generate_jwt_with_valid_key() {
        let mut app = App::new("1234", test_private_key()).unwrap();
        let jwt = app.generate_jwt().unwrap();

        assert!(!jwt.is_empty(), "JWT should not be empty");

        // JWTs have three base64-encoded parts separated by dots.
        let parts: Vec<&str> = jwt.split('.').collect();
        assert_eq!(parts.len(), 3, "JWT should have 3 parts");
    }

    #[test]
    fn generate_jwt_caches_result() {
        let mut app = App::new("1234", test_private_key()).unwrap();

        let jwt1 = app.generate_jwt().unwrap();
        let jwt2 = app.generate_jwt().unwrap();

        assert_eq!(jwt1, jwt2, "second call should return the cached JWT");
    }

    #[test]
    fn generate_jwt_returns_preset_jwt() {
        let exp = Utc::now() + Duration::minutes(5);
        let mut app = App::new("1234", test_private_key())
            .unwrap()
            .with_jwt("preset-jwt-token", exp);

        let jwt = app.generate_jwt().unwrap();
        assert_eq!(jwt, "preset-jwt-token");
    }

    #[test]
    fn generate_jwt_regenerates_when_expired() {
        // Set a JWT that's already expired.
        let exp = Utc::now() - Duration::seconds(30);
        let mut app = App::new("1234", test_private_key())
            .unwrap()
            .with_jwt("expired-jwt", exp);

        let jwt = app.generate_jwt().unwrap();

        assert_ne!(jwt, "expired-jwt", "should generate a new JWT, not return the expired one");
        assert!(!jwt.is_empty());
    }

    #[test]
    fn generate_jwt_fails_with_invalid_key() {
        let result = App::new("1234", b"not a valid PEM key");
        assert!(result.is_err(), "should fail with an invalid private key");
    }

    // -------------------------------------------------------------------------
    // generate_access_token tests
    // -------------------------------------------------------------------------

    #[test]
    fn generate_access_token_success() {
        let mut server = mockito::Server::new();
        let mock = server
            .mock("POST", "/app/installations/5678/access_tokens")
            .match_header("Authorization", "Bearer test-jwt")
            .match_header("Accept", GITHUB_ACCEPT_HEADER)
            .match_header("User-Agent", USER_AGENT)
            .with_status(201)
            .with_header("Content-Type", "application/json")
            .with_body(
                r#"{
                    "token": "ghs_test_token_123",
                    "expires_at": "2001-09-09T01:46:40Z",
                    "permissions": {"contents": "read"}
                }"#,
            )
            .create();

        let exp = Utc::now() + Duration::minutes(5);
        let mut app = App::new("1234", test_private_key())
            .unwrap()
            .with_base_url(server.url())
            .with_jwt("test-jwt", exp);

        let token = app.generate_access_token("5678").unwrap();

        assert_eq!(token.token, "ghs_test_token_123");
        assert_eq!(token.permissions.get("contents").unwrap(), "read");
        mock.assert();
    }

    #[test]
    fn generate_access_token_non_2xx_response() {
        let mut server = mockito::Server::new();
        let mock = server
            .mock("POST", "/app/installations/5678/access_tokens")
            .with_status(401)
            .with_body(r#"{"message": "Bad credentials"}"#)
            .create();

        let exp = Utc::now() + Duration::minutes(5);
        let mut app = App::new("1234", test_private_key())
            .unwrap()
            .with_base_url(server.url())
            .with_jwt("test-jwt", exp);

        let result = app.generate_access_token("5678");

        assert!(result.is_err());
        let err = result.unwrap_err();
        match &err {
            Error::Api { status, body } => {
                assert_eq!(*status, 401);
                assert!(body.contains("Bad credentials"));
            }
            _ => panic!("expected Error::Api, got: {err:?}"),
        }
        mock.assert();
    }

    #[test]
    fn generate_access_token_invalid_json() {
        let mut server = mockito::Server::new();
        let mock = server
            .mock("POST", "/app/installations/5678/access_tokens")
            .with_status(200)
            .with_body("not json")
            .create();

        let exp = Utc::now() + Duration::minutes(5);
        let mut app = App::new("1234", test_private_key())
            .unwrap()
            .with_base_url(server.url())
            .with_jwt("test-jwt", exp);

        let result = app.generate_access_token("5678");
        assert!(result.is_err());
        mock.assert();
    }

    // -------------------------------------------------------------------------
    // list_installs tests
    // -------------------------------------------------------------------------

    #[test]
    fn list_installs_success() {
        let mut server = mockito::Server::new();
        let mock = server
            .mock("GET", "/app/installations")
            .match_header("Authorization", "Bearer test-jwt")
            .match_header("Accept", GITHUB_ACCEPT_HEADER)
            .match_header("User-Agent", USER_AGENT)
            .with_status(200)
            .with_header("Content-Type", "application/json")
            .with_body(
                r#"[
                    {"id": 1, "app_id": 2, "target_id": 10, "target_type": "Organization"},
                    {"id": 3, "app_id": 4, "target_id": 20, "target_type": "User"}
                ]"#,
            )
            .create();

        let exp = Utc::now() + Duration::minutes(5);
        let mut app = App::new("1234", test_private_key())
            .unwrap()
            .with_base_url(server.url())
            .with_jwt("test-jwt", exp);

        let installs = app.list_installs().unwrap();

        assert_eq!(installs.len(), 2);
        assert_eq!(installs[0].id, 1);
        assert_eq!(installs[0].app_id, 2);
        assert_eq!(installs[1].id, 3);
        assert_eq!(installs[1].app_id, 4);
        mock.assert();
    }

    #[test]
    fn list_installs_non_2xx_response() {
        let mut server = mockito::Server::new();
        let mock = server
            .mock("GET", "/app/installations")
            .with_status(401)
            .with_body(r#"{"message": "Bad credentials"}"#)
            .create();

        let exp = Utc::now() + Duration::minutes(5);
        let mut app = App::new("1234", test_private_key())
            .unwrap()
            .with_base_url(server.url())
            .with_jwt("test-jwt", exp);

        let result = app.list_installs();
        assert!(result.is_err());
        match result.unwrap_err() {
            Error::Api { status, .. } => assert_eq!(status, 401),
            e => panic!("expected Error::Api, got: {e:?}"),
        }
        mock.assert();
    }

    #[test]
    fn list_installs_invalid_json() {
        let mut server = mockito::Server::new();
        let mock = server
            .mock("GET", "/app/installations")
            .with_status(200)
            .with_body("not json")
            .create();

        let exp = Utc::now() + Duration::minutes(5);
        let mut app = App::new("1234", test_private_key())
            .unwrap()
            .with_base_url(server.url())
            .with_jwt("test-jwt", exp);

        let result = app.list_installs();
        assert!(result.is_err());
        mock.assert();
    }

    // -------------------------------------------------------------------------
    // get_install tests
    // -------------------------------------------------------------------------

    #[test]
    fn get_install_success() {
        let mut server = mockito::Server::new();
        let mock = server
            .mock("GET", "/repos/octocat/hello-world/installation")
            .match_header("Authorization", "Bearer test-jwt")
            .match_header("Accept", GITHUB_ACCEPT_HEADER)
            .match_header("User-Agent", USER_AGENT)
            .with_status(200)
            .with_header("Content-Type", "application/json")
            .with_body(
                r#"{
                    "id": 42,
                    "app_id": 1,
                    "target_id": 100,
                    "target_type": "Organization"
                }"#,
            )
            .create();

        let exp = Utc::now() + Duration::minutes(5);
        let mut app = App::new("1234", test_private_key())
            .unwrap()
            .with_base_url(server.url())
            .with_jwt("test-jwt", exp);

        let install = app.get_install("octocat", "hello-world").unwrap();

        assert_eq!(install.id, 42);
        assert_eq!(install.app_id, 1);
        assert_eq!(install.target_type, "Organization");
        mock.assert();
    }

    #[test]
    fn get_install_non_2xx_response() {
        let mut server = mockito::Server::new();
        let mock = server
            .mock("GET", "/repos/octocat/hello-world/installation")
            .with_status(404)
            .with_body(r#"{"message": "Not Found"}"#)
            .create();

        let exp = Utc::now() + Duration::minutes(5);
        let mut app = App::new("1234", test_private_key())
            .unwrap()
            .with_base_url(server.url())
            .with_jwt("test-jwt", exp);

        let result = app.get_install("octocat", "hello-world");
        assert!(result.is_err());
        match result.unwrap_err() {
            Error::Api { status, .. } => assert_eq!(status, 404),
            e => panic!("expected Error::Api, got: {e:?}"),
        }
        mock.assert();
    }

    #[test]
    fn get_install_invalid_json() {
        let mut server = mockito::Server::new();
        let mock = server
            .mock("GET", "/repos/octocat/hello-world/installation")
            .with_status(200)
            .with_body("not json")
            .create();

        let exp = Utc::now() + Duration::minutes(5);
        let mut app = App::new("1234", test_private_key())
            .unwrap()
            .with_base_url(server.url())
            .with_jwt("test-jwt", exp);

        let result = app.get_install("octocat", "hello-world");
        assert!(result.is_err());
        mock.assert();
    }
}
