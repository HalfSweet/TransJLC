//! Configuration management for TransJLC
//!
//! This module handles CLI argument parsing and application settings.

use anyhow::{anyhow, Context, Result};
use clap::{ColorChoice, Parser};
use std::path::PathBuf;
use tracing::info;

#[derive(Debug, Clone, Parser)]
#[command(
    name = "transjlc",
    about = "TransJLC - Convert EDA files for JLCPCB manufacturing",
    author = "HalfSweet <HalfSweet@HalfSweet.cn>",
    version,
    color = ColorChoice::Auto
)]
pub struct Config {
    /// EDA software type for input files
    #[arg(
        short = 'e',
        long = "eda",
        default_value = "auto",
        value_parser = ["auto", "kicad", "jlc", "protel"],
        help = "EDA software type (auto, kicad, jlc, protel)"
    )]
    pub eda: String,

    /// Input path (file or directory)
    #[arg(
        short = 'p',
        long = "path",
        default_value = ".",
        value_name = "PATH",
        help = "Input file or directory path"
    )]
    pub path: PathBuf,

    /// Output directory path
    #[arg(
        short = 'o',
        long = "output_path",
        default_value = "./output",
        value_name = "OUTPUT",
        help = "Output file or directory path"
    )]
    pub output_path: PathBuf,

    /// Create ZIP file for output
    #[arg(
        short = 'z',
        long = "zip",
        help = "Compress converted files into a ZIP archive"
    )]
    pub zip: bool,

    /// Name for the output ZIP file
    #[arg(
        short = 'n',
        long = "zip_name",
        default_value = "Gerber",
        help = "Name for the output ZIP archive"
    )]
    pub zip_name: String,

    /// Enable verbose logging
    #[arg(short = 'v', long = "verbose", help = "Enable verbose logging output")]
    pub verbose: bool,

    /// Disable progress bars
    #[arg(long = "no-progress", help = "Disable progress indicators")]
    pub no_progress: bool,
}

impl Config {
    /// Parse arguments and apply initial configuration
    pub fn from_args() -> Result<Self> {
        let config = Config::parse();

        // Set up tracing with environment variable support
        // RUST_LOG takes precedence over verbose flag
        let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("off"));

        tracing_subscriber::fmt().with_env_filter(env_filter).init();

        if config.verbose {
            info!("Configuration: {:?}", config);
        }

        Ok(config)
    }

    /// Get normalized EDA type
    pub fn get_eda_type(&self) -> EdaType {
        match self.eda.to_lowercase().as_str() {
            "auto" => EdaType::Auto,
            "kicad" => EdaType::KiCad,
            "protel" => EdaType::Protel,
            "jlc" => EdaType::Jlc,
            custom => EdaType::Custom(custom.to_string()),
        }
    }

    /// Validate configuration settings
    pub fn validate(&self) -> Result<()> {
        // Validate input path exists
        if !self.path.exists() {
            return Err(anyhow!(
                "Input path does not exist: {}",
                self.path.display()
            ));
        }

        // Create output directory if it doesn't exist
        if !self.output_path.exists() {
            std::fs::create_dir_all(&self.output_path).with_context(|| {
                format!(
                    "Failed to create output directory: {}",
                    self.output_path.display()
                )
            })?;
            info!("Created output directory: {}", self.output_path.display());
        }

        info!("Configuration validation completed successfully");
        Ok(())
    }
}

/// Supported EDA software types
#[derive(Debug, Clone, PartialEq)]
pub enum EdaType {
    Auto,
    KiCad,
    Protel,
    Jlc,
    Custom(String),
}

impl EdaType {
    pub fn as_str(&self) -> &str {
        match self {
            EdaType::Auto => "auto",
            EdaType::KiCad => "kicad",
            EdaType::Protel => "protel",
            EdaType::Jlc => "jlc",
            EdaType::Custom(name) => name,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_eda_type_conversion() {
        let config = Config {
            eda: "kicad".to_string(),
            path: PathBuf::from("."),
            output_path: PathBuf::from("./output"),
            zip: false,
            zip_name: "test".to_string(),
            verbose: false,
            no_progress: false,
        };

        assert_eq!(config.get_eda_type(), EdaType::KiCad);
    }

    #[test]
    fn test_custom_eda_type() {
        let config = Config {
            eda: "custom_eda".to_string(),
            path: PathBuf::from("."),
            output_path: PathBuf::from("./output"),
            zip: false,
            zip_name: "test".to_string(),
            verbose: false,
            no_progress: false,
        };

        assert_eq!(
            config.get_eda_type(),
            EdaType::Custom("custom_eda".to_string())
        );
    }
}
