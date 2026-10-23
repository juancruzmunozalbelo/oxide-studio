//! Error types for project serialization, deserialization, and disk persistence.

use std::fmt;

/// Errors that can occur during DAW project file operations and schema parsing.
#[derive(Debug)]
pub enum DAWProjectError {
    /// File system or I/O failure.
    Io(std::io::Error),
    /// JSON syntax or serialization failure.
    Serialization(serde_json::Error),
    /// Schema validation or missing field error.
    Validation(String),
}

impl fmt::Display for DAWProjectError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DAWProjectError::Io(e) => write!(f, "I/O error: {}", e),
            DAWProjectError::Serialization(e) => write!(f, "JSON serialization error: {}", e),
            DAWProjectError::Validation(msg) => write!(f, "Project validation error: {}", msg),
        }
    }
}

impl std::error::Error for DAWProjectError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            DAWProjectError::Io(e) => Some(e),
            DAWProjectError::Serialization(e) => Some(e),
            DAWProjectError::Validation(_) => None,
        }
    }
}

impl From<std::io::Error> for DAWProjectError {
    fn from(err: std::io::Error) -> Self {
        DAWProjectError::Io(err)
    }
}

impl From<serde_json::Error> for DAWProjectError {
    fn from(err: serde_json::Error) -> Self {
        DAWProjectError::Serialization(err)
    }
}
