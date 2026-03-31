// types.rs — Data types returned by the GitHub App API.
//
// In Rust, we define structs and derive `Serialize`/`Deserialize` from serde.
// This is similar to Go's struct tags (`json:"field_name"`), but serde is more
// powerful — it handles renaming, default values, custom formats, and more.
//
// The `#[serde(rename_all = "snake_case")]` attribute tells serde that the JSON
// field names use snake_case, which matches GitHub's API format.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// An access token for a specific GitHub App installation.
///
/// This token is short-lived (typically 1 hour) and scoped to the permissions
/// granted to the installation. Use it as a `Bearer` token in API requests.
///
/// See: <https://docs.github.com/en/apps/creating-github-apps/authenticating-with-a-github-app/generating-an-installation-access-token-for-a-github-app>
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessToken {
    /// The token string to use in `Authorization: Bearer <token>` headers.
    pub token: String,

    /// When this token expires. GitHub typically sets this to 1 hour from creation.
    pub expires_at: DateTime<Utc>,

    /// The permissions this token has (e.g., `{"contents": "read"}`).
    #[serde(default)]
    pub permissions: HashMap<String, String>,
}

/// A GitHub App installation — an instance of the app installed on an
/// organization or user account.
///
/// See: <https://docs.github.com/en/rest/apps/apps#list-installations-for-the-authenticated-app>
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Install {
    /// The unique installation ID. Used to generate access tokens.
    pub id: i64,

    /// The ID of the GitHub App this installation belongs to.
    pub app_id: i64,

    /// The ID of the organization or user account where the app is installed.
    pub target_id: i64,

    /// Either `"Organization"` or `"User"`.
    pub target_type: String,

    /// The permissions granted to this installation (e.g., `{"contents": "read"}`).
    #[serde(default)]
    pub permissions: HashMap<String, String>,

    /// The webhook events this installation is subscribed to.
    #[serde(default)]
    pub events: Vec<String>,
}
