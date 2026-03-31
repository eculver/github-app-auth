# github-app-auth

Authentication utilities for [GitHub Apps](https://docs.github.com/en/apps/creating-github-apps/about-creating-github-apps/about-creating-github-apps), in Rust.

Provides both a **CLI tool** (`github-app`) and a **library crate** (`github_app_auth`) for generating JWTs and short-lived installation access tokens for GitHub Apps.

## Why GitHub Apps?

Most CI/CD pipelines authenticate to GitHub with a personal access token tied to a machine user. This works, but it means every repository shares the same token, and if it leaks, everything is exposed.

GitHub Apps are better:

- **Scoped to a single repository** (or a handful) rather than an entire user account.
- **Short-lived tokens** that expire in 1 hour, so they're essentially single-use.
- **Fine-grained permissions** — grant only what the app needs (e.g., read-only access to contents).
- **Auditable** — every action is attributed to the app, not a person.

This project makes it simple to go from "I have a GitHub App" to "I have a token I can use."

## Quick start

### Prerequisites

1. **A GitHub App** — if you don't have one yet, see [Creating a GitHub App](#creating-a-github-app) below.
2. **A private key** (PEM file) for that app — see [Generating a private key](#generating-a-private-key) below.
3. **The App ID** — visible on the app's settings page under "General."
4. **An installation** — the app must be installed on at least one repository or organization.

### Install the CLI

```sh
# From source (requires Rust toolchain)
cargo install --git https://github.com/eculver/github-app-auth

# Or clone and build locally
git clone https://github.com/eculver/github-app-auth
cd github-app-auth
make install
```

### Generate an access token

The most common use case: generate a short-lived token for a specific installation.

```sh
# If you know the installation ID:
export GITHUB_TOKEN=$(github-app token \
  --id 12345 \
  --install-id 67890 \
  --private-key-file ./my-app.pem)

# If you know the repository (the tool looks up the installation for you):
export GITHUB_TOKEN=$(github-app token \
  --id 12345 \
  --owner my-org \
  --repository my-repo \
  --private-key-file ./my-app.pem)
```

Now use `$GITHUB_TOKEN` like any other GitHub token:

```sh
# Clone a private repo
git clone https://x-access-token:${GITHUB_TOKEN}@github.com/my-org/my-repo.git

# Use with the GitHub CLI
gh api repos/my-org/my-repo/releases --header "Authorization: Bearer ${GITHUB_TOKEN}"

# Use with curl
curl -H "Authorization: Bearer ${GITHUB_TOKEN}" \
  https://api.github.com/repos/my-org/my-repo/contents
```

## CLI reference

```
github-app [OPTIONS] <COMMAND>
```

### Global options

| Flag | Env var | Description |
|------|---------|-------------|
| `--debug` | `GITHUB_APP_DEBUG` | Print debug info (HTTP requests, JWT caching) to stderr |
| `--version` / `-V` | | Print version and exit |

### `github-app jwt`

Generate a signed JWT for a GitHub App. The JWT is valid for ~10 minutes and is used to authenticate as the app itself (e.g., to list installations).

```sh
github-app jwt \
  --id <APP_ID> \
  --private-key-file <PATH>
```

| Flag | Env var | Required | Description |
|------|---------|----------|-------------|
| `--id` | `GITHUB_APP_ID` | Yes | The GitHub App ID |
| `--private-key-file` | | One of these | Path to the PEM private key file |
| `--private-key` | | is required | The PEM private key as a string |

### `github-app token`

Generate a short-lived installation access token (typically valid for 1 hour). Provide either `--install-id` directly, or `--owner` and `--repository` to have the installation looked up automatically.

```sh
# With installation ID
github-app token \
  --id <APP_ID> \
  --install-id <INSTALL_ID> \
  --private-key-file <PATH>

# With owner/repository (looks up the installation for you)
github-app token \
  --id <APP_ID> \
  --owner <OWNER> \
  --repository <REPO> \
  --private-key-file <PATH>
```

| Flag | Env var | Required | Description |
|------|---------|----------|-------------|
| `--id` | `GITHUB_APP_ID` | Yes | The GitHub App ID |
| `--install-id` | `GITHUB_APP_INSTALL_ID` | Either this | The installation ID |
| `--owner` | `GITHUB_APP_OWNER` | or both | Repository owner (org or user) |
| `--repository` | `GITHUB_APP_REPOSITORY` | of these | Repository name |
| `--private-key-file` | | One of these | Path to the PEM private key file |
| `--private-key` | | is required | The PEM private key as a string |

### `github-app list-installs`

List all installations for a GitHub App. Prints a JSON array to stdout.

```sh
github-app list-installs \
  --id <APP_ID> \
  --private-key-file <PATH>
```

| Flag | Env var | Required | Description |
|------|---------|----------|-------------|
| `--id` | `GITHUB_APP_ID` | Yes | The GitHub App ID |
| `--private-key-file` | | One of these | Path to the PEM private key file |
| `--private-key` | | is required | The PEM private key as a string |

### `github-app get-install`

Get the installation details for a specific repository. Prints a JSON object to stdout.

```sh
github-app get-install \
  --id <APP_ID> \
  --owner <OWNER> \
  --repository <REPO> \
  --private-key-file <PATH>
```

| Flag | Env var | Required | Description |
|------|---------|----------|-------------|
| `--id` | `GITHUB_APP_ID` | Yes | The GitHub App ID |
| `--owner` | `GITHUB_APP_OWNER` | Yes | Repository owner (org or user) |
| `--repository` | `GITHUB_APP_REPOSITORY` | Yes | Repository name |
| `--private-key-file` | | One of these | Path to the PEM private key file |
| `--private-key` | | is required | The PEM private key as a string |

### `github-app version`

Print the version and exit. Equivalent to `github-app --version`.

## Using as a library

Add the crate to your project:

```toml
[dependencies]
github-app-auth = { git = "https://github.com/eculver/github-app-auth" }
```

Then use it:

```rust
use github_app_auth::App;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let private_key = std::fs::read("my-app.pem")?;
    let mut app = App::new("12345", private_key)?;

    // Generate a short-lived access token for an installation.
    let token = app.generate_access_token("67890")?;
    println!("Token: {}", token.token);
    println!("Expires: {}", token.expires_at);

    // Or list all installations to find the right one.
    let installs = app.list_installs()?;
    for install in &installs {
        println!("Installation {} (app {})", install.id, install.app_id);
    }

    // Or look up the installation for a specific repository.
    let install = app.get_install("my-org", "my-repo")?;
    let token = app.generate_access_token(&install.id.to_string())?;
    println!("Token for my-org/my-repo: {}", token.token);

    Ok(())
}
```

### Available types

| Type | Description |
|------|-------------|
| `App` | The main entry point. Holds the app ID, private key, and JWT cache. |
| `AccessToken` | A short-lived token with `token`, `expires_at`, and `permissions` fields. |
| `Install` | An installation record with `id`, `app_id`, `target_id`, `target_type`, `permissions`, and `events`. |
| `Error` | The crate's error type. Variants: `PrivateKey`, `Jwt`, `Http`, `Api`, `Json`, `Io`. |

## Setting up a GitHub App from scratch

If you're starting from zero, here's the full walkthrough.

### Creating a GitHub App

1. Go to **Settings > Developer settings > GitHub Apps** (or for an org: **Organization Settings > Developer settings > GitHub Apps**).
2. Click **New GitHub App**.
3. Fill in the basics:
   - **App name**: something descriptive, like `my-org-release-bot`.
   - **Homepage URL**: your repository URL is fine.
   - **Webhook**: uncheck "Active" if you don't need webhook events.
4. Under **Permissions**, grant only what you need. For example, to create releases: set **Contents** to **Read & write**.
5. Under **Where can this GitHub App be installed?**, choose **Only on this account** unless you need it on other accounts.
6. Click **Create GitHub App**.
7. Note the **App ID** shown on the next page.

### Generating a private key

1. On the app's settings page, scroll to **Private keys**.
2. Click **Generate a private key**.
3. A `.pem` file downloads automatically. **Store it securely** — GitHub only keeps the public half; you cannot download this again.

### Installing the app on a repository

1. On the app's settings page, click **Install App** in the left sidebar.
2. Choose the account (your user or an organization).
3. Select **Only select repositories** and pick the repositories you need.
4. Click **Install**.
5. Note the **installation ID** from the URL: `https://github.com/settings/installations/<INSTALL_ID>`.

### Putting it all together

Now you have all three pieces: the App ID, the private key file, and the installation ID (or the owner/repo to look it up). Generate a token:

```sh
github-app token \
  --id 12345 \
  --owner my-org \
  --repository my-repo \
  --private-key-file ./my-app.pem
```

The token printed to stdout is ready to use.

## How it works

GitHub App authentication is a two-step process:

```
Private Key + App ID
        |
        v
   [Generate JWT]  ──>  JWT (valid ~10 min)
        |
        v
   [POST /app/installations/{id}/access_tokens]
        |
        v
   Installation Access Token (valid ~1 hour)
```

1. **JWT generation**: The app's private key signs a JWT with the App ID as the issuer. This JWT authenticates the app itself and is valid for 10 minutes.
2. **Token exchange**: The JWT is sent to GitHub's API to mint a short-lived installation access token scoped to a specific installation (org/repo). This token is typically valid for 1 hour.

For full details, see [About authentication with a GitHub App](https://docs.github.com/en/apps/creating-github-apps/authenticating-with-a-github-app/about-authentication-with-a-github-app).

## Development

```sh
# Run tests (includes clippy lint)
make test

# Run tests with HTML coverage report (requires: cargo install cargo-llvm-cov)
make cover

# Build release binary
make build

# Install to ~/.cargo/bin/
make install
```

## License

MIT
