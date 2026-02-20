use std::fmt;

#[derive(Debug)]
pub enum ShipItError {
    Git(git2::Error),
    Gitlab(gitlab::GitlabError),
    Http(reqwest::Error),
    Error(String),
}

impl fmt::Display for ShipItError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Git(e) => write!(f, "Git operation failed with: {}", e),
            Self::Gitlab(e) => write!(f, "Gitlab operation failed with: {}", e),
            Self::Http(e) => write!(f, "The HTTP request failed with: {}", e),
            Self::Error(e) => write!(f, "{}", e),
        }
    }
}

impl std::error::Error for ShipItError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ShipItError::Git(e) => Some(e),
            ShipItError::Gitlab(e) => Some(e),
            ShipItError::Http(e) => Some(e),
            _ => None,
        }
    }
}
