//! Error handling for TransJLC
//!
//! This module provides unified error handling using anyhow for better error propagation
//! and context information throughout the application.

use anyhow::Context;
use std::path::Path;

pub type Result<T> = anyhow::Result<T>;

/// Extension trait for Results to add context with file paths
pub trait ResultExt<T> {
    /// Add context with file path information
    fn with_path_context<P: AsRef<Path>>(self, operation: &str, path: P) -> Result<T>;

    /// Add context with EDA type information
    fn with_eda_context(self, eda_name: &str) -> Result<T>;

    /// Add context with conversion operation
    fn with_conversion_context(self, from: &str, to: &str) -> Result<T>;
}

impl<T, E> ResultExt<T> for std::result::Result<T, E>
where
    E: Into<anyhow::Error> + Send + Sync + 'static,
{
    fn with_path_context<P: AsRef<Path>>(self, operation: &str, path: P) -> Result<T> {
        self.map_err(|e| e.into())
            .with_context(|| format!("Failed to {} file: {}", operation, path.as_ref().display()))
    }

    fn with_eda_context(self, eda_name: &str) -> Result<T> {
        self.map_err(|e| e.into())
            .with_context(|| format!("Error processing {} format", eda_name))
    }

    fn with_conversion_context(self, from: &str, to: &str) -> Result<T> {
        self.map_err(|e| e.into())
            .with_context(|| format!("Error converting from {} to {}", from, to))
    }
}

/// Specific error types for TransJLC operations
#[derive(Debug, thiserror::Error)]
pub enum TransJlcError {
    #[error("No matching EDA pattern found for files in directory")]
    NoMatchingPattern,

    #[error("Unsupported EDA format: {format}")]
    UnsupportedEda { format: String },

    #[error("Invalid Gerber file format: {reason}")]
    InvalidGerberFormat { reason: String },

    #[error("File not found in expected location: {path}")]
    FileNotFound { path: String },

    #[error("ZIP extraction failed: {reason}")]
    ZipExtractionFailed { reason: String },

    #[error("Hash aperture generation failed: {reason}")]
    HashApertureError { reason: String },
}
