//! TransJLC - Convert Gerber files to JLCEDA format
//! 
//! This library provides functionality to convert PCB Gerber files from various
//! EDA software formats to JLCEDA format with modern Rust practices.

#![allow(non_snake_case)]

// Public API modules
pub mod archive;
pub mod config;
pub mod converter;
pub mod error;
pub mod gerber;
pub mod patterns;
pub mod progress;

// Re-export main types for convenience
pub use config::{Config, EdaType};
pub use converter::{Converter, ConversionStats};
pub use error::{Result, ResultExt, TransJlcError};

// Initialize i18n for the library
rust_i18n::i18n!("i18n");
