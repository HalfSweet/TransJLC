//! Configuration management for TransJLC
//!
//! This module handles CLI argument parsing and application settings.

use anyhow::{anyhow, Context, Result};
use clap::{ArgAction, ColorChoice, Parser};
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

    /// Optional colorful silkscreen image for top layer
    #[arg(
        long = "top_color_image",
        value_name = "PATH",
        help = "Path to colorful silkscreen image for the top layer"
    )]
    pub top_color_image: Option<PathBuf>,

    /// Optional colorful silkscreen image for bottom layer
    #[arg(
        long = "bottom_color_image",
        value_name = "PATH",
        help = "Path to colorful silkscreen image for the bottom layer"
    )]
    pub bottom_color_image: Option<PathBuf>,

    /// Force synthetic EasyEDA Pro header injection
    #[arg(
        long = "inject-header",
        action = ArgAction::SetTrue,
        conflicts_with = "no_inject_header",
        help = "Force synthetic EasyEDA Pro header injection"
    )]
    pub inject_header: bool,

    /// Disable synthetic EasyEDA Pro header injection
    #[arg(
        long = "no-inject-header",
        action = ArgAction::SetTrue,
        conflicts_with = "inject_header",
        help = "Disable synthetic EasyEDA Pro header injection"
    )]
    pub no_inject_header: bool,

    /// Keep unmatched files in the output
    #[arg(
        long = "passthrough",
        action = ArgAction::SetTrue,
        conflicts_with = "no_passthrough",
        help = "Keep files that do not match a known production layer"
    )]
    pub passthrough: bool,

    /// Drop unmatched files from the output
    #[arg(
        long = "no-passthrough",
        action = ArgAction::SetTrue,
        conflicts_with = "passthrough",
        help = "Drop files that do not match a known production layer"
    )]
    pub no_passthrough: bool,
}

impl Config {
    /// Parse CLI arguments without initializing logging
    pub fn parse() -> Self {
        <Self as Parser>::parse()
    }

    /// Parse arguments and apply initial configuration
    pub fn from_args() -> Result<Self> {
        let config = Self::parse();

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

    /// User override for synthetic EasyEDA Pro header injection
    pub fn inject_header_override(&self) -> Option<bool> {
        match (self.inject_header, self.no_inject_header) {
            (true, false) => Some(true),
            (false, true) => Some(false),
            _ => None,
        }
    }

    /// User override for unmatched-file pass-through
    pub fn pass_through_unmatched_override(&self) -> Option<bool> {
        match (self.passthrough, self.no_passthrough) {
            (true, false) => Some(true),
            (false, true) => Some(false),
            _ => None,
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

        // Validate optional colorful silkscreen images
        if let Some(path) = &self.top_color_image {
            if !path.exists() {
                return Err(anyhow!(
                    "Top layer colorful silkscreen not found: {}",
                    path.display()
                ));
            }
        }

        if let Some(path) = &self.bottom_color_image {
            if !path.exists() {
                return Err(anyhow!(
                    "Bottom layer colorful silkscreen not found: {}",
                    path.display()
                ));
            }
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
            top_color_image: None,
            bottom_color_image: None,
            inject_header: false,
            no_inject_header: false,
            passthrough: false,
            no_passthrough: false,
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
            top_color_image: None,
            bottom_color_image: None,
            inject_header: false,
            no_inject_header: false,
            passthrough: false,
            no_passthrough: false,
        };

        assert_eq!(
            config.get_eda_type(),
            EdaType::Custom("custom_eda".to_string())
        );
    }
}
