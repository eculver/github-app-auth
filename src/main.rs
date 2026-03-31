// main.rs — The CLI entry point for the `github-app` binary.
//
// This file is intentionally thin: it only defines the CLI interface and
// delegates all real work to the library (`github_app_auth::App`). This
// separation means the same logic can be used as both a CLI tool and a
// library dependency.
//
// ## CLI framework: clap
//
// We use `clap` with its "derive" API, which is the standard way to build
// CLIs in Rust. You define your arguments as struct fields and annotate them
// with attributes. Clap generates the parser, help text, and validation.
//
// ## Conventions from clig.dev
//
// - Subcommands instead of separate binaries
// - `--help` on every subcommand (automatic with clap)
// - `--version` flag (automatic with clap)
// - Human-readable output by default
// - Errors to stderr, output to stdout
// - Non-zero exit code on failure

use std::fs;
use std::process;

use clap::{Args, Parser, Subcommand};

use github_app_auth::App;

/// Authentication utilities for GitHub Apps.
///
/// Generate JWTs, access tokens, and manage installations for GitHub Apps.
/// For more information about GitHub App authentication, see:
/// https://docs.github.com/en/apps/creating-github-apps/authenticating-with-a-github-app
#[derive(Parser)]
#[command(
    name = "github-app",
    version = env!("GITHUB_APP_AUTH_VERSION"),
    about = "Authentication utilities for GitHub Apps"
)]
struct Cli {
    /// Enable debug logging.
    ///
    /// When set, prints detailed information about HTTP requests, JWT
    /// generation, and caching to stderr.
    #[arg(long, global = true, env = "GITHUB_APP_DEBUG")]
    debug: bool,

    #[command(subcommand)]
    command: Commands,
}

/// The subcommands available in the CLI.
///
/// In clap, each enum variant becomes a subcommand. The `#[derive(Subcommand)]`
/// macro generates the dispatch logic. This is Rust's version of Go's
/// `switch` on a command name.
#[derive(Subcommand)]
enum Commands {
    /// Generate a JWT for a GitHub App.
    ///
    /// The JWT is signed with the app's private key and is valid for ~10
    /// minutes. Use it to authenticate as the app itself for API calls like
    /// listing installations.
    Jwt(JwtArgs),

    /// Generate an installation access token.
    ///
    /// Creates a short-lived token (typically 1 hour) scoped to a specific
    /// installation. Provide either --install-id directly, or --owner and
    /// --repository to look up the installation automatically.
    Token(TokenArgs),

    /// List all installations for a GitHub App.
    ///
    /// Prints a JSON array of every organization or user account where
    /// the app is installed.
    ListInstalls(ListInstallsArgs),

    /// Get the installation for a specific repository.
    ///
    /// Prints the JSON installation object for a given owner/repository
    /// combination.
    GetInstall(GetInstallArgs),

    /// Print the version and exit.
    Version,
}

// =============================================================================
// Shared argument groups
// =============================================================================
//
// In clap, `#[derive(Args)]` creates a reusable group of arguments that can
// be "flattened" into multiple subcommands with `#[command(flatten)]`. This
// avoids repeating the same flags across subcommands.
//
// This is conceptually similar to Go struct embedding.

/// Arguments for providing the private key.
///
/// Exactly one of `--private-key-file` or `--private-key` must be provided.
/// The `#[group(...)]` attribute enforces this constraint — clap will print
/// an error if neither or both are given.
#[derive(Args)]
#[group(required = true, multiple = false)]
struct PrivateKeyArgs {
    /// Path to the PEM-encoded private key file.
    #[arg(long)]
    private_key_file: Option<String>,

    /// The PEM-encoded private key as a string.
    ///
    /// Useful in CI environments where the key is stored in a secret and
    /// passed as a variable. Prefer --private-key-file when possible.
    #[arg(long)]
    private_key: Option<String>,
}

impl PrivateKeyArgs {
    /// Resolve the private key bytes from whichever source was provided.
    fn resolve(&self) -> Result<Vec<u8>, String> {
        if let Some(ref path) = self.private_key_file {
            fs::read(path).map_err(|e| format!("failed to read private key file '{path}': {e}"))
        } else if let Some(ref key) = self.private_key {
            Ok(key.as_bytes().to_vec())
        } else {
            // This shouldn't happen because clap enforces the group constraint,
            // but we handle it for completeness.
            Err("one of --private-key-file or --private-key is required".to_string())
        }
    }
}

// =============================================================================
// Subcommand argument structs
// =============================================================================

#[derive(Args)]
struct JwtArgs {
    /// The GitHub App ID.
    #[arg(long, env = "GITHUB_APP_ID")]
    id: String,

    #[command(flatten)]
    private_key: PrivateKeyArgs,
}

#[derive(Args)]
struct TokenArgs {
    /// The GitHub App ID.
    #[arg(long, env = "GITHUB_APP_ID")]
    id: String,

    /// The installation ID to generate a token for.
    ///
    /// If not provided, --owner and --repository must be given to look up
    /// the installation.
    #[arg(long, env = "GITHUB_APP_INSTALL_ID")]
    install_id: Option<String>,

    /// The repository owner (organization or user).
    ///
    /// Used together with --repository to look up the installation ID
    /// automatically. Ignored if --install-id is provided.
    #[arg(long, env = "GITHUB_APP_OWNER", requires = "repository")]
    owner: Option<String>,

    /// The repository name.
    ///
    /// Used together with --owner to look up the installation ID
    /// automatically. Ignored if --install-id is provided.
    #[arg(long, env = "GITHUB_APP_REPOSITORY", requires = "owner")]
    repository: Option<String>,

    #[command(flatten)]
    private_key: PrivateKeyArgs,
}

#[derive(Args)]
struct ListInstallsArgs {
    /// The GitHub App ID.
    #[arg(long, env = "GITHUB_APP_ID")]
    id: String,

    #[command(flatten)]
    private_key: PrivateKeyArgs,
}

#[derive(Args)]
struct GetInstallArgs {
    /// The GitHub App ID.
    #[arg(long, env = "GITHUB_APP_ID")]
    id: String,

    /// The repository owner (organization or user).
    #[arg(long, env = "GITHUB_APP_OWNER")]
    owner: String,

    /// The repository name.
    #[arg(long, env = "GITHUB_APP_REPOSITORY")]
    repository: String,

    #[command(flatten)]
    private_key: PrivateKeyArgs,
}

// =============================================================================
// Main
// =============================================================================

fn main() {
    let cli = Cli::parse();

    // Configure logging. When --debug is passed (or GITHUB_APP_DEBUG=true),
    // we enable debug-level output to stderr. The `env_logger` crate is the
    // conventional backend for Rust's `log` facade.
    //
    // Library code uses `log::debug!()` macros, and this is where we decide
    // whether those messages actually get printed.
    if cli.debug {
        env_logger::Builder::new()
            .filter_level(log::LevelFilter::Debug)
            .init();
    } else {
        env_logger::Builder::new()
            .filter_level(log::LevelFilter::Warn)
            .init();
    }

    // Dispatch to the appropriate subcommand. If anything fails, we print
    // the error to stderr and exit with code 1 — per clig.dev guidelines.
    if let Err(e) = run(cli.command) {
        eprintln!("error: {e}");
        process::exit(1);
    }
}

/// Run the selected subcommand. Separated from `main()` so we can use `?`
/// for error propagation (main() can't return Result with a custom exit code).
fn run(command: Commands) -> Result<(), Box<dyn std::error::Error>> {
    match command {
        Commands::Jwt(args) => {
            let key = args.private_key.resolve()?;
            let mut app = App::new(&args.id, key)?;
            let jwt = app.generate_jwt()?;
            println!("{jwt}");
        }

        Commands::Token(args) => {
            let key = args.private_key.resolve()?;
            let mut app = App::new(&args.id, key)?;

            // Determine the installation ID: either provided directly, or
            // looked up by owner/repository.
            let install_id = if let Some(id) = args.install_id {
                id
            } else if let (Some(owner), Some(repo)) = (args.owner, args.repository) {
                let install = app.get_install(&owner, &repo)?;
                install.id.to_string()
            } else {
                return Err(
                    "either --install-id or both --owner and --repository are required".into(),
                );
            };

            let token = app.generate_access_token(&install_id)?;
            println!("{}", token.token);
        }

        Commands::ListInstalls(args) => {
            let key = args.private_key.resolve()?;
            let mut app = App::new(&args.id, key)?;
            let installs = app.list_installs()?;
            let json = serde_json::to_string_pretty(&installs)?;
            println!("{json}");
        }

        Commands::GetInstall(args) => {
            let key = args.private_key.resolve()?;
            let mut app = App::new(&args.id, key)?;
            let install = app.get_install(&args.owner, &args.repository)?;
            let json = serde_json::to_string_pretty(&install)?;
            println!("{json}");
        }

        Commands::Version => {
            println!("github-app {}", env!("GITHUB_APP_AUTH_VERSION"));
        }
    }

    Ok(())
}
