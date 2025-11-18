//! TransJLC - Convert Gerber files to JLCEDA format
//!
//! A modern Rust application for converting PCB Gerber files from various
//! EDA software (KiCad, Protel, etc.) to JLCEDA format.

#![allow(non_snake_case)]

use tracing::{error, info};
use TransJLC::{config::Config, converter::Converter, error::Result};

fn main() -> Result<()> {
    // Parse configuration and initialize logging
    let config = Config::from_args().unwrap_or_else(|e| {
        eprintln!("Configuration error: {}", e);
        std::process::exit(1);
    });

    info!("Starting conversion process...");
    if config.verbose {
        info!("Configuration: {:?}", config);
    }

    // Create and run converter
    let mut converter = Converter::new(config);

    match converter.run() {
        Ok(()) => {
            let stats = converter.get_conversion_stats();
            info!("Conversion completed successfully");
            info!("Processed {} files", stats.total_files_processed);

            println!("Conversion completed successfully");
            Ok(())
        }
        Err(e) => {
            error!("Conversion failed: {:#}", e);
            eprintln!("Error: {:#}", e);
            std::process::exit(1);
        }
    }
}
