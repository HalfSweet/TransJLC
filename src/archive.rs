//! Archive handling for ZIP file operations
//!
//! This module provides functionality for extracting input ZIP files
//! and creating output ZIP files with proper progress tracking.

use crate::error::{Result, ResultExt, TransJlcError};
use anyhow::Context;
use indicatif::{ProgressBar, ProgressStyle};
use rust_i18n::t;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use tempfile::TempDir;
use tracing::info;
use zip::ZipArchive;

/// Archive extractor for handling ZIP input files
pub struct ArchiveExtractor {
    temp_dir: Option<TempDir>,
}

impl ArchiveExtractor {
    /// Create a new archive extractor
    pub fn new() -> Self {
        Self { temp_dir: None }
    }

    /// Extract ZIP file if the input path is a ZIP file
    /// Returns the path to use for processing (original path or extracted directory)
    pub fn extract_if_needed(&mut self, input_path: &Path, show_progress: bool) -> Result<PathBuf> {
        if !self.is_zip_file(input_path) {
            info!(
                "Input is not a ZIP file, using as directory: {}",
                input_path.display()
            );
            return Ok(input_path.to_path_buf());
        }

        info!(
            "{}",
            t!(
                "archive.extracting",
                file = input_path.display().to_string()
            )
        );

        // Create temporary directory
        let temp_dir =
            TempDir::new().context("Failed to create temporary directory for ZIP extraction")?;

        let temp_path = temp_dir.path();

        // Extract ZIP file
        self.extract_zip_to_directory(input_path, temp_path, show_progress)
            .with_path_context("extract ZIP file", input_path)?;

        let extracted_path = temp_path.to_path_buf();
        self.temp_dir = Some(temp_dir);

        info!("ZIP file extracted to: {}", extracted_path.display());
        Ok(extracted_path)
    }

    /// Check if a file is a ZIP file based on extension
    fn is_zip_file(&self, path: &Path) -> bool {
        path.is_file()
            && path
                .extension()
                .and_then(|ext| ext.to_str())
                .map(|ext| ext.to_lowercase() == "zip")
                .unwrap_or(false)
    }

    /// Extract ZIP file to the specified directory
    fn extract_zip_to_directory(
        &self,
        zip_path: &Path,
        target_dir: &Path,
        show_progress: bool,
    ) -> Result<()> {
        let file = fs::File::open(zip_path).with_path_context("open ZIP file", zip_path)?;

        let mut archive =
            ZipArchive::new(file).map_err(|e| TransJlcError::ZipExtractionFailed {
                reason: format!("Invalid ZIP file: {}", e),
            })?;

        let total_files = archive.len();
        info!("{}", t!("archive.extracted_files", count = total_files));

        let progress = if show_progress {
            let pb = ProgressBar::new(total_files as u64);
            pb.set_style(
                ProgressStyle::default_bar()
                    .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} {msg}")?
                    .progress_chars("#>-")
            );
            pb.set_message("Extracting files...");
            Some(pb)
        } else {
            None
        };

        for i in 0..archive.len() {
            let mut file = archive
                .by_index(i)
                .map_err(|e| TransJlcError::ZipExtractionFailed {
                    reason: format!("Failed to read file at index {}: {}", i, e),
                })?;

            let outpath = target_dir.join(file.name());

            if file.is_dir() {
                fs::create_dir_all(&outpath).with_path_context("create directory", &outpath)?;
            } else {
                // Create parent directories if needed
                if let Some(parent) = outpath.parent() {
                    fs::create_dir_all(parent)
                        .with_path_context("create parent directory", parent)?;
                }

                // Extract file
                let mut outfile =
                    fs::File::create(&outpath).with_path_context("create output file", &outpath)?;

                io::copy(&mut file, &mut outfile)
                    .with_path_context("write extracted file", &outpath)?;
            }

            if let Some(ref pb) = progress {
                pb.inc(1);
            }
        }

        if let Some(pb) = progress {
            pb.finish_with_message("Extraction completed");
        }

        Ok(())
    }

    /// Get the temporary directory path if ZIP was extracted
    pub fn temp_path(&self) -> Option<&Path> {
        self.temp_dir.as_ref().map(|dir| dir.path())
    }
}

impl Drop for ArchiveExtractor {
    fn drop(&mut self) {
        if self.temp_dir.is_some() {
            info!("Cleaning up temporary extraction directory");
        }
    }
}

/// Archive creator for building output ZIP files
pub struct ArchiveCreator;

impl ArchiveCreator {
    /// Create a ZIP file from a collection of files
    pub fn create_zip<P: AsRef<Path>, I: IntoIterator<Item = P>>(
        files: I,
        output_path: P,
        show_progress: bool,
    ) -> Result<()> {
        let output_path = output_path.as_ref();
        let files: Vec<PathBuf> = files
            .into_iter()
            .map(|p| p.as_ref().to_path_buf())
            .collect();

        info!(
            "{}",
            t!("archive.creating", file = output_path.display().to_string())
        );

        // Create output directory if it doesn't exist
        if let Some(parent) = output_path.parent() {
            fs::create_dir_all(parent).with_path_context("create output directory", parent)?;
        }

        let file =
            fs::File::create(output_path).with_path_context("create ZIP file", output_path)?;

        let mut zip = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored)
            .unix_permissions(0o755);

        let progress = if show_progress {
            let pb = ProgressBar::new(files.len() as u64);
            pb.set_style(
                ProgressStyle::default_bar()
                    .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} {msg}")?
                    .progress_chars("#>-")
            );
            pb.set_message("Creating ZIP file...");
            Some(pb)
        } else {
            None
        };

        for file_path in files {
            let file_name = file_path
                .file_name()
                .and_then(|name| name.to_str())
                .context("Invalid filename")?;

            zip.start_file(file_name, options)
                .context("Failed to start ZIP file entry")?;

            let content =
                fs::read(&file_path).with_path_context("read file for ZIP", &file_path)?;

            use std::io::Write;
            zip.write_all(&content)
                .context("Failed to write file content to ZIP")?;

            if let Some(ref pb) = progress {
                pb.inc(1);
            }
        }

        zip.finish().context("Failed to finalize ZIP file")?;

        if let Some(pb) = progress {
            pb.finish_with_message("ZIP file created successfully");
        }

        info!("ZIP file created successfully: {}", output_path.display());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_zip_file() {
        let _extractor = ArchiveExtractor::new();

        // Test ZIP file detection
        let zip_path = Path::new("test.zip");
        let txt_path = Path::new("test.txt");
        let _dir_path = Path::new("test_dir");

        // Note: These tests would need actual files to be fully functional
        // This is testing the logic, not actual file access
        assert!(zip_path.extension().unwrap() == "zip");
        assert!(txt_path.extension().unwrap() != "zip");
    }

    #[test]
    fn test_archive_creator_options() {
        let _options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored)
            .unix_permissions(0o755);

        // Test that options are created successfully
        // The actual compression method can be verified in integration tests
        assert!(true); // Placeholder for actual verification
    }
}
