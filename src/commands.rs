use clap::Args;
use std::path::PathBuf;

/// Error type for API manifest inspection execution.
#[derive(Debug, thiserror::Error)]
pub enum ApiManifestError {
    /// Target path was not found on disk.
    #[error("Target path '{0}' was not found")]
    PathNotFound(PathBuf),

    /// I/O error during API manifest operation.
    #[error("I/O error during API inspection: {0}")]
    Io(#[from] std::io::Error),
}

/// Command-line arguments for `api-manifest`.
#[derive(Args, Debug, Clone, Default, PartialEq, Eq)]
pub struct ApiManifestCommand {
    /// Path to target crate or workspace directory
    #[arg(long)]
    path: Option<PathBuf>,

    /// Silence non-essential logging output
    #[arg(short, long)]
    quiet: bool,
}

impl ApiManifestCommand {
    /// Creates a new `ApiManifestCommand`.
    pub fn new(path: Option<PathBuf>, quiet: bool) -> Self {
        Self { path, quiet }
    }

    /// Executes the command.
    pub fn run(self) -> Result<(), ApiManifestError> {
        let target_dir = self.path.unwrap_or_else(|| PathBuf::from("."));
        if !target_dir.exists() {
            return Err(ApiManifestError::PathNotFound(target_dir));
        }
        Ok(())
    }
}
