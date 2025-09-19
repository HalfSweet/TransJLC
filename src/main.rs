//! TransJLC - Convert Gerber files to JLCEDA format
//!
//! A modern Rust application for converting PCB Gerber files from various
//! EDA software (KiCad, Protel, etc.) to JLCEDA format.

#![allow(non_snake_case)]

use rust_i18n::t;
use tracing::{error, info};
use TransJLC::{config::Config, converter::Converter, error::Result};

rust_i18n::i18n!("i18n");

fn main() -> Result<()> {
    // Parse configuration and initialize logging
    let config = Config::from_args().unwrap_or_else(|e| {
        eprintln!("Configuration error: {}", e);
        std::process::exit(1);
    });

    // Language settings are already applied in from_args()
    info!("{}", t!("converter.starting"));
    if config.verbose {
        info!("Configuration: {:?}", config);
    }

    // Create and run converter
    let mut converter = Converter::new(config);

    match converter.run() {
        Ok(()) => {
            let stats = converter.get_conversion_stats();
            info!("{}", t!("converter.conversion_complete", time = 0));
            info!(
                "{}",
                t!(
                    "converter.files_processed",
                    count = stats.total_files_processed
                )
            );

            println!("{}", t!("converter.conversion_complete", time = 0));
            Ok(())
        }
        Err(e) => {
            error!("Conversion failed: {:#}", e);
            eprintln!("Error: {:#}", e);
            std::process::exit(1);
        }
    }
}
