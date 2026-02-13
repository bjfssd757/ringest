
#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("File system error: {0}")]
    FileSystemError(#[from] FileSystemError),

    #[error("IO Error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Operation timed out")]
    Timeout,

    #[error("Internal error: {0}")]
    Internal(String),
}

#[derive(thiserror::Error, Debug)]
pub enum FileSystemError {
    #[error("Path not found: {0}")]
    PathNotFound(std::path::PathBuf),

    #[error("Regex error: {0}")]
    #[cfg(feature = "regex")]
    RegexError(#[from] regex::Error),

    #[error("Content not found: {0}")]
    SearchError(String),

    #[error("File `{name}` closed or inaccessible")]
    FileClosed {
        name: String,
    },

    #[error("UTF-8 error: {0}")]
    InvalidUtf8(#[from] std::string::FromUtf8Error),

    #[error("Exceeded max recursive depth: {0}")]
    MaxDepthExceeded(u64),

    #[error("Access denied")]
    PermissionDenied,
}

pub type Result<T, E = Error> = std::result::Result<T, E>;

impl From<tokio::time::error::Elapsed> for Error {
    fn from(_value: tokio::time::error::Elapsed) -> Self {
        Error::Timeout
    }
}