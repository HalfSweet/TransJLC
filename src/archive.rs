//! Archive handling for ZIP file operations
//!
//! This module provides functionality for extracting input ZIP files
//! and creating output ZIP files with proper progress tracking.

use crate::error::{Result, ResultExt, TransJlcError};
use anyhow::Context;
use indicatif::{ProgressBar, ProgressStyle};
use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use tempfile::TempDir;
use tracing::{info, warn};
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

        info!("Extracting archive: {}", input_path.display());

        // Create temporary directory
        let temp_dir =
            TempDir::new().context("Failed to create temporary directory for ZIP extraction")?;

        let temp_path = temp_dir.path();

        // Extract ZIP file
        self.extract_zip_to_directory(input_path, temp_path, show_progress)
            .with_path_context("extract ZIP file", input_path)?;
        self.recursive_unzip(temp_path)
            .with_path_context("extract nested ZIP files", input_path)?;
        let flat_path = temp_path.join("_flat");
        fs::create_dir_all(&flat_path)
            .with_path_context("create flat extraction directory", &flat_path)?;
        let flattened_count = self
            .flatten_processable_files(temp_path, &flat_path)
            .with_path_context("flatten extracted files", input_path)?;

        let extracted_path = if flattened_count > 0 {
            flat_path
        } else {
            temp_path.to_path_buf()
        };
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
        info!("Extracted {} entries from archive", total_files);

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

            let Some(enclosed_name) = file.enclosed_name().map(|p| p.to_path_buf()) else {
                warn!("Skipping unsafe ZIP entry path: {}", file.name());
                continue;
            };
            let outpath = target_dir.join(enclosed_name);

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

    /// Recursively extract nested ZIP files under the extraction root
    fn recursive_unzip(&self, root: &Path) -> Result<()> {
        loop {
            let mut zips = Vec::new();
            Self::collect_files(root, &mut zips, |path| {
                path.is_file()
                    && path
                        .extension()
                        .and_then(|ext| ext.to_str())
                        .map(|ext| ext.eq_ignore_ascii_case("zip"))
                        .unwrap_or(false)
            })?;

            if zips.is_empty() {
                return Ok(());
            }

            for zip_path in zips {
                let Some(parent) = zip_path.parent() else {
                    continue;
                };
                self.extract_zip_to_directory(&zip_path, parent, false)
                    .with_path_context("extract nested ZIP file", &zip_path)?;
                if let Err(err) = fs::remove_file(&zip_path) {
                    warn!(
                        "Failed to remove nested ZIP after extraction ({}): {}",
                        zip_path.display(),
                        err
                    );
                }
            }
        }
    }

    /// Flatten processable files into one working directory
    fn flatten_processable_files(&self, root: &Path, dest: &Path) -> Result<usize> {
        let mut files = Vec::new();
        Self::collect_files(root, &mut files, |path| {
            path.is_file() && Self::looks_processable(path)
        })?;

        let mut candidates: HashMap<String, PathBuf> = HashMap::new();
        for path in files {
            if path.starts_with(dest) {
                continue;
            }
            if path
                .strip_prefix(root)
                .ok()
                .map(|relative| {
                    relative.components().any(|component| {
                        component
                            .as_os_str()
                            .to_str()
                            .map(|part| part.starts_with("__"))
                            .unwrap_or(false)
                    })
                })
                .unwrap_or(false)
            {
                continue;
            }

            let Some(filename) = path.file_name().and_then(|name| name.to_str()) else {
                continue;
            };

            match candidates.get(filename) {
                None => {
                    candidates.insert(filename.to_string(), path);
                }
                Some(existing) if Self::prefer_candidate(root, &path, existing) => {
                    candidates.insert(filename.to_string(), path);
                }
                Some(_) => {}
            }
        }

        let count = candidates.len();
        for (filename, source) in candidates {
            fs::copy(&source, dest.join(filename))
                .with_path_context("copy flattened file", &source)?;
        }

        info!("Flattened {} processable files", count);
        Ok(count)
    }

    fn collect_files<F>(root: &Path, files: &mut Vec<PathBuf>, predicate: F) -> Result<()>
    where
        F: Fn(&Path) -> bool + Copy,
    {
        for entry in fs::read_dir(root).with_path_context("read directory", root)? {
            let entry = entry.with_path_context("read directory entry", root)?;
            let path = entry.path();
            if path.is_dir() {
                Self::collect_files(&path, files, predicate)?;
            } else if predicate(&path) {
                files.push(path);
            }
        }

        Ok(())
    }

    fn prefer_candidate(root: &Path, new_path: &Path, old_path: &Path) -> bool {
        let new_depth = new_path
            .strip_prefix(root)
            .map(|path| path.components().count())
            .unwrap_or(usize::MAX);
        let old_depth = old_path
            .strip_prefix(root)
            .map(|path| path.components().count())
            .unwrap_or(usize::MAX);

        if new_depth != old_depth {
            return new_depth < old_depth;
        }

        let new_mtime = new_path.metadata().and_then(|meta| meta.modified()).ok();
        let old_mtime = old_path.metadata().and_then(|meta| meta.modified()).ok();
        new_mtime > old_mtime
    }

    fn looks_processable(path: &Path) -> bool {
        if let Some(filename) = path.file_name().and_then(|name| name.to_str()) {
            if filename.eq_ignore_ascii_case("FlyingProbeTesting.json") {
                return true;
            }
        }

        let Some(ext) = path.extension().and_then(|ext| ext.to_str()) else {
            return false;
        };
        let ext = ext.to_ascii_lowercase();
        matches!(
            ext.as_str(),
            "gtl"
                | "gbl"
                | "gto"
                | "gbo"
                | "gts"
                | "gbs"
                | "gtp"
                | "gbp"
                | "gko"
                | "gm1"
                | "gm2"
                | "gm3"
                | "gm4"
                | "gm5"
                | "gm9"
                | "gm10"
                | "gm12"
                | "gd1"
                | "gg1"
                | "gbr"
                | "drl"
                | "txt"
                | "tap"
                | "nc"
                | "gdd"
                | "pho"
                | "art"
        ) || (ext.len() > 1
            && ext.starts_with('g')
            && ext[1..].chars().all(|ch| ch.is_ascii_digit()))
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

        info!("Creating archive: {}", output_path.display());

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
    use std::io::Write;
    use tempfile::tempdir;

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

    #[test]
    fn test_nested_zip_is_flattened() {
        let temp_dir = tempdir().expect("create temp dir");
        let inner_zip = temp_dir.path().join("inner.zip");
        write_test_zip(
            &inner_zip,
            vec![
                ("nested/project.GTL", b"top".to_vec()),
                ("nested/FlyingProbeTesting.json", b"{\"ok\":true}".to_vec()),
            ],
        );

        let outer_zip = temp_dir.path().join("outer.zip");
        write_test_zip(
            &outer_zip,
            vec![
                (
                    "wrapper/inner.zip",
                    fs::read(&inner_zip).expect("read inner zip"),
                ),
                ("wrapper/project.GKO", b"outline".to_vec()),
            ],
        );

        let mut extractor = ArchiveExtractor::new();
        let working = extractor
            .extract_if_needed(&outer_zip, false)
            .expect("extract outer zip");

        assert!(working.join("project.GTL").exists());
        assert!(working.join("project.GKO").exists());
        assert!(working.join("FlyingProbeTesting.json").exists());
    }

    fn write_test_zip(path: &Path, entries: Vec<(&str, Vec<u8>)>) {
        let file = fs::File::create(path).expect("create test zip");
        let mut zip = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);

        for (name, bytes) in entries {
            zip.start_file(name, options).expect("start zip entry");
            zip.write_all(&bytes).expect("write zip entry");
        }

        zip.finish().expect("finish test zip");
    }
}
