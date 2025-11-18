//! Configuration management for TransJLC
//!
//! This module handles CLI argument parsing and application settings.

use anyhow::{anyhow, Context, Result};
use clap::builder::styling;
use clap::{value_parser, Arg, ColorChoice, Command};
use std::path::PathBuf;
use tracing::info;

/// Build the CLI command
pub fn build_cli() -> Command {
    let styles = styling::Styles::styled()
        .header(styling::AnsiColor::Green.on_default() | styling::Effects::BOLD)
        .usage(styling::AnsiColor::Green.on_default() | styling::Effects::BOLD)
        .literal(styling::AnsiColor::Blue.on_default() | styling::Effects::BOLD)
        .placeholder(styling::AnsiColor::Cyan.on_default());

    Command::new("transjlc")
        .about("TransJLC - Convert EDA files for JLCPCB manufacturing")
        .author("HalfSweet <HalfSweet@HalfSweet.cn>")
        .color(ColorChoice::Auto)
        .styles(styles)
        .arg(
            Arg::new("eda")
                .short('e')
                .long("eda")
                .help("EDA software type (auto, kicad, jlc, protel)")
                .value_parser(["auto", "kicad", "jlc", "protel"])
                .default_value("auto"),
        )
        .arg(
            Arg::new("path")
                .short('p')
                .long("path")
                .help("Input file or directory path")
                .value_parser(value_parser!(String))
                .default_value("."),
        )
        .arg(
            Arg::new("output_path")
                .short('o')
                .long("output_path")
                .help("Output file or directory path")
                .value_parser(value_parser!(String))
                .default_value("./output"),
        )
        .arg(
            Arg::new("zip")
                .short('z')
                .long("zip")
                .help("Compress converted files into a ZIP archive")
                .action(clap::ArgAction::SetTrue),
        )
        .arg(
            Arg::new("zip_name")
                .short('n')
                .long("zip_name")
                .help("Name for the output ZIP archive")
                .value_parser(value_parser!(String))
                .default_value("Gerber"),
        )
        .arg(
            Arg::new("verbose")
                .short('v')
                .long("verbose")
                .help("Enable verbose logging output")
                .action(clap::ArgAction::SetTrue),
        )
        .arg(
            Arg::new("no_progress")
                .long("no-progress")
                .help("Disable progress indicators")
                .action(clap::ArgAction::SetTrue),
        )
}

#[derive(Debug, Clone)]
pub struct Config {
    /// EDA software type for input files
    pub eda: String,

    /// Input path (file or directory)
    pub path: PathBuf,

    /// Output directory path
    pub output_path: PathBuf,

    /// Create ZIP file for output
    pub zip: bool,

    /// Name for the output ZIP file
    pub zip_name: String,

    /// Enable verbose logging
    pub verbose: bool,

    /// Disable progress bars
    pub no_progress: bool,
}

impl Config {
    /// Parse arguments and apply initial configuration
    pub fn from_args() -> Result<Self> {
        let matches = build_cli().get_matches();

        let path = matches
            .get_one::<String>("path")
            .ok_or_else(|| anyhow!("Input path is required"))?
            .to_string();
        let path = PathBuf::from(path);

        let output_path = matches
            .get_one::<String>("output_path")
            .cloned()
            .unwrap_or_else(|| "./output".to_string());
        let output_path = PathBuf::from(output_path);

        let eda = matches
            .get_one::<String>("eda")
            .cloned()
            .unwrap_or_else(|| "auto".to_string());

        let zip = matches.get_flag("zip");

        let zip_name = matches
            .get_one::<String>("zip_name")
            .cloned()
            .unwrap_or_else(|| "Gerber".to_string());

        let verbose = matches.get_flag("verbose");
        let no_progress = matches.get_flag("no_progress");

        let config = Config {
            eda,
            path,
            output_path,
            zip,
            zip_name,
            verbose,
            no_progress,
        };

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
            return Err(anyhow::anyhow!(
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
