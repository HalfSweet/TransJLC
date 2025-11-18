//! Core conversion engine for TransJLC
//!
//! This module orchestrates the conversion process from various EDA formats
//! to JLC format, handling file discovery, pattern matching, and processing.

use crate::{
    archive::{ArchiveCreator, ArchiveExtractor},
    config::{Config, EdaType},
    error::{Result, ResultExt, TransJlcError},
    gerber::GerberProcessor,
    patterns::{EdaPatterns, LayerType, PatternMatcher},
    progress::ProgressTracker,
};
use anyhow::Context;
use rust_embed::RustEmbed;
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
};
use tracing::{debug, info, warn};

#[derive(RustEmbed)]
#[folder = "Assets/"]
struct Asset;

/// The main conversion engine
pub struct Converter {
    config: Config,
    progress_tracker: ProgressTracker,
    archive_extractor: ArchiveExtractor,
    gerber_processor: GerberProcessor,
    processed_files: HashMap<LayerType, PathBuf>,
}

impl Converter {
    /// Create a new converter with the given configuration
    pub fn new(config: Config) -> Self {
        let progress_enabled = !config.no_progress;

        Self {
            config,
            progress_tracker: ProgressTracker::new(progress_enabled),
            archive_extractor: ArchiveExtractor::new(),
            gerber_processor: GerberProcessor::new(),
            processed_files: HashMap::new(),
        }
    }

    /// Run the complete conversion process
    pub fn run(&mut self) -> Result<()> {
        let start = std::time::Instant::now();
        info!("Starting conversion process...");

        // Validate configuration
        self.config
            .validate()
            .context("Configuration validation failed")?;

        // Extract archive if needed
        let working_path = self
            .extract_input_files()
            .context("Failed to extract input files")?;

        // Discover and analyze files
        let files = self
            .discover_files(&working_path)
            .context("Failed to discover input files")?;

        // Detect EDA format and create pattern matcher
        let patterns = self
            .create_pattern_matcher(&files)
            .context("Failed to create pattern matcher")?;

        // Process files
        self.process_files(&files, &patterns, &working_path)
            .context("Failed to process files")?;

        // Add required assets
        self.add_required_assets()
            .context("Failed to add required assets")?;

        // Create final output
        self.create_output().context("Failed to create output")?;

        info!("Conversion completed in {} ms", start.elapsed().as_millis());
        Ok(())
    }

    /// Extract input files from archive if necessary
    fn extract_input_files(&mut self) -> Result<PathBuf> {
        let progress = self.progress_tracker.create_spinner("Analyzing input...");

        let working_path = self
            .archive_extractor
            .extract_if_needed(&self.config.path, !self.config.no_progress)
            .with_path_context("analyze input", &self.config.path)?;

        ProgressTracker::finish_progress(progress, "Input analysis completed");
        Ok(working_path)
    }

    /// Discover all files in the working directory
    fn discover_files(&self, working_path: &Path) -> Result<Vec<PathBuf>> {
        info!("Processing files in {}", working_path.display());

        let files = fs::read_dir(working_path)
            .with_path_context("read directory", working_path)?
            .filter_map(|entry| {
                entry.ok().and_then(|e| {
                    let path = e.path();
                    if path.is_file() {
                        Some(path)
                    } else {
                        None
                    }
                })
            })
            .collect::<Vec<_>>();

        info!("Discovered {} files", files.len());
        debug!("Files found: {:?}", files);

        if files.is_empty() {
            return Err(TransJlcError::FileNotFound {
                path: working_path.display().to_string(),
            }
            .into());
        }

        Ok(files)
    }

    /// Create appropriate pattern matcher based on configuration and file analysis
    fn create_pattern_matcher(&self, files: &[PathBuf]) -> Result<EdaPatterns> {
        info!("Detecting EDA tool type for {} files...", files.len());

        let patterns = match self.config.get_eda_type() {
            EdaType::Auto => {
                info!("Attempting to auto-detect an EDA format");
                PatternMatcher::auto_detect_eda(files)?
            }
            EdaType::KiCad => {
                info!("Using KiCad naming patterns");
                PatternMatcher::create_kicad_patterns()
            }
            EdaType::Protel => {
                info!("Using Protel naming patterns");
                PatternMatcher::create_protel_patterns()
            }
            EdaType::Jlc => {
                info!("Using JLC naming patterns");
                PatternMatcher::create_jlc_patterns()
            }
            EdaType::Custom(name) => {
                warn!("Using custom pattern matcher for: {}", name);
                PatternMatcher::create_custom_patterns(name)
            }
        };

        Ok(patterns)
    }

    /// Process all discovered files using the pattern matcher
    fn process_files(
        &mut self,
        files: &[PathBuf],
        patterns: &EdaPatterns,
        working_path: &Path,
    ) -> Result<()> {
        info!("Processing Gerber files...");

        let progress = self
            .progress_tracker
            .create_conversion_progress(files.len());
        let needs_g54_aperture_prefix = self.determine_g54_requirement(files, patterns)?;

        for file in files {
            self.process_single_file(file, patterns, working_path, needs_g54_aperture_prefix)
                .with_path_context("process file", file)?;

            ProgressTracker::update_progress(&progress, 1, None);
        }

        ProgressTracker::finish_progress(progress, "File processing completed");

        info!("Processed {} files", self.processed_files.len());
        Ok(())
    }

    /// Process a single file
    fn process_single_file(
        &mut self,
        file_path: &Path,
        patterns: &EdaPatterns,
        _working_path: &Path,
        needs_g54_aperture_prefix: bool,
    ) -> Result<()> {
        let filename = file_path
            .file_name()
            .and_then(|name| name.to_str())
            .context("Invalid filename")?;

        debug!("Processing file: {}", filename);

        // Try to match the file to a layer type
        if let Some(layer_type) = patterns.match_filename(filename) {
            info!("Matched {} to layer type: {:?}", filename, layer_type);

            // Determine output filename and path
            let output_filename = layer_type.to_jlc_filename();
            let output_path = self.get_output_file_path(&output_filename);

            // Read and process file content
            let content =
                fs::read_to_string(file_path).with_path_context("read file content", file_path)?;

            // Apply processing if it's a Gerber file (not drill files)
            let processed_content = if self.should_process_gerber(&layer_type) {
                self.gerber_processor
                    .process_gerber_content(content, needs_g54_aperture_prefix)?
            } else {
                content
            };

            // Write processed content
            self.write_output_file(&output_path, &processed_content)
                .with_path_context("write output file", &output_path)?;

            // Track the processed file
            self.processed_files.insert(layer_type, output_path);
        } else {
            debug!("No pattern match for file: {}", filename);
        }

        Ok(())
    }

    /// Determine whether any target file is missing the required G54 aperture prefix
    fn determine_g54_requirement(&self, files: &[PathBuf], patterns: &EdaPatterns) -> Result<bool> {
        for file in files {
            let Some(filename) = file.file_name().and_then(|name| name.to_str()) else {
                continue;
            };

            let Some(layer_type) = patterns.match_filename(filename) else {
                continue;
            };

            if !self.should_process_gerber(&layer_type) {
                continue;
            }

            let content = fs::read_to_string(file).with_path_context("read file content", file)?;

            if self
                .gerber_processor
                .has_missing_g54_aperture_prefix(&content)?
            {
                return Ok(true);
            }
        }

        Ok(false)
    }

    /// Determine if a layer type should undergo Gerber processing
    fn should_process_gerber(&self, layer_type: &LayerType) -> bool {
        !matches!(
            layer_type,
            LayerType::NpthThrough | LayerType::PthThrough | LayerType::PthThroughVia
        )
    }

    /// Get the full output file path
    fn get_output_file_path(&self, filename: &str) -> PathBuf {
        self.get_working_output_dir().join(filename)
    }

    /// Get the working output directory (temporary or final output directory)
    fn get_working_output_dir(&self) -> PathBuf {
        // If we have a temporary extraction directory, use it for intermediate processing
        if let Some(temp_path) = self.archive_extractor.temp_path() {
            temp_path.to_path_buf()
        } else {
            self.config.output_path.clone()
        }
    }

    /// Write content to output file, creating directories as needed
    fn write_output_file(&self, output_path: &Path, content: &str) -> Result<()> {
        if let Some(parent) = output_path.parent() {
            fs::create_dir_all(parent).with_path_context("create output directory", parent)?;
        }

        fs::write(output_path, content).with_path_context("write file", output_path)?;

        debug!("Written output file: {}", output_path.display());
        Ok(())
    }

    /// Add required assets (like PCB ordering instructions)
    fn add_required_assets(&mut self) -> Result<()> {
        info!("Adding required assets");

        const ASSET_NAME: &str = "PCB下单必读.txt";

        let content =
            Asset::get(ASSET_NAME).context("Required asset not found in embedded files")?;

        let output_path = self.get_working_output_dir().join(ASSET_NAME);

        fs::write(&output_path, content.data.as_ref())
            .with_path_context("write required asset", &output_path)?;

        // Track the asset as an "other" file
        self.processed_files.insert(LayerType::Other, output_path);

        info!("Added required asset: {}", ASSET_NAME);
        Ok(())
    }

    /// Create the final output (files or ZIP archive)
    fn create_output(&self) -> Result<()> {
        info!("Creating final output");

        let file_paths: Vec<PathBuf> = self.processed_files.values().cloned().collect();

        if self.config.zip {
            // Create ZIP archive
            let zip_path = self
                .config
                .output_path
                .join(format!("{}.zip", self.config.zip_name));

            ArchiveCreator::create_zip(&file_paths, &zip_path, !self.config.no_progress)?;

            info!("Created ZIP archive: {}", zip_path.display());
        } else {
            // Copy files to final output directory
            self.copy_files_to_output(&file_paths)?;
            info!("Copied {} files to output directory", file_paths.len());
        }

        Ok(())
    }

    /// Copy processed files to the final output directory
    fn copy_files_to_output(&self, file_paths: &[PathBuf]) -> Result<()> {
        let progress = self
            .progress_tracker
            .create_file_progress(file_paths.len(), "Copying files to output");

        // Ensure output directory exists
        fs::create_dir_all(&self.config.output_path)
            .with_path_context("create output directory", &self.config.output_path)?;

        for file_path in file_paths {
            if let Some(filename) = file_path.file_name() {
                let dest_path = self.config.output_path.join(filename);

                fs::copy(file_path, &dest_path)
                    .with_path_context("copy file to output", &dest_path)?;

                ProgressTracker::update_progress(&progress, 1, None);
            }
        }

        ProgressTracker::finish_progress(progress, "File copying completed");
        Ok(())
    }

    /// Get statistics about the conversion process
    pub fn get_conversion_stats(&self) -> ConversionStats {
        ConversionStats {
            total_files_processed: self.processed_files.len(),
            layer_types_found: self.processed_files.keys().cloned().collect(),
            output_format: if self.config.zip { "ZIP" } else { "Files" }.to_string(),
        }
    }
}

/// Statistics about the conversion process
#[derive(Debug)]
pub struct ConversionStats {
    pub total_files_processed: usize,
    pub layer_types_found: Vec<LayerType>,
    pub output_format: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_converter_creation() {
        let config = Config {
            eda: "kicad".to_string(),
            path: PathBuf::from("."),
            output_path: PathBuf::from("./output"),
            zip: false,
            zip_name: "test".to_string(),
            verbose: false,
            no_progress: true,
        };

        let converter = Converter::new(config);
        assert!(converter.processed_files.is_empty());
    }

    #[test]
    fn test_working_output_dir() {
        let config = Config {
            eda: "kicad".to_string(),
            path: PathBuf::from("."),
            output_path: PathBuf::from("./output"),
            zip: false,
            zip_name: "test".to_string(),
            verbose: false,
            no_progress: true,
        };

        let converter = Converter::new(config);
        let working_dir = converter.get_working_output_dir();

        // Should use config output path when no temp directory
        assert_eq!(working_dir, PathBuf::from("./output"));
    }

    #[test]
    fn test_should_process_gerber() {
        let config = Config {
            eda: "kicad".to_string(),
            path: PathBuf::from("."),
            output_path: PathBuf::from("./output"),
            zip: false,
            zip_name: "test".to_string(),
            verbose: false,
            no_progress: true,
        };

        let converter = Converter::new(config);

        // Drill files should not be processed as Gerber
        assert!(!converter.should_process_gerber(&LayerType::NpthThrough));
        assert!(!converter.should_process_gerber(&LayerType::PthThrough));
        assert!(!converter.should_process_gerber(&LayerType::PthThroughVia));

        // Other layer types should be processed
        assert!(converter.should_process_gerber(&LayerType::TopCopper));
        assert!(converter.should_process_gerber(&LayerType::BoardOutline));
        assert!(converter.should_process_gerber(&LayerType::InnerLayer(1)));
    }

    #[test]
    fn test_conversion_stats() {
        let config = Config {
            eda: "kicad".to_string(),
            path: PathBuf::from("."),
            output_path: PathBuf::from("./output"),
            zip: true,
            zip_name: "test".to_string(),
            verbose: false,
            no_progress: true,
        };

        let mut converter = Converter::new(config);

        // Add some mock processed files
        converter
            .processed_files
            .insert(LayerType::TopCopper, PathBuf::from("top.gtl"));
        converter
            .processed_files
            .insert(LayerType::BottomCopper, PathBuf::from("bottom.gbl"));

        let stats = converter.get_conversion_stats();

        assert_eq!(stats.total_files_processed, 2);
        assert_eq!(stats.output_format, "ZIP");
        assert!(stats.layer_types_found.contains(&LayerType::TopCopper));
        assert!(stats.layer_types_found.contains(&LayerType::BottomCopper));
    }

    #[test]
    fn test_determine_g54_requirement() {
        let temp_dir = tempdir().expect("Failed to create temp dir");
        let file_missing = temp_dir.path().join("project-F_Cu.gbr");
        let file_prefixed = temp_dir.path().join("project-B_Cu.gbr");

        fs::write(&file_missing, "G04*\nD10*\n").expect("Failed to write missing file");
        fs::write(&file_prefixed, "G04*\nG54D11*\n").expect("Failed to write prefixed file");

        let config = Config {
            eda: "kicad".to_string(),
            path: PathBuf::from("."),
            output_path: PathBuf::from("./output"),
            zip: false,
            zip_name: "test".to_string(),
            verbose: false,
            no_progress: true,
        };

        let converter = Converter::new(config);
        let patterns = PatternMatcher::create_kicad_patterns();
        let files = vec![file_missing.clone(), file_prefixed.clone()];

        let needs_prefix = converter
            .determine_g54_requirement(&files, &patterns)
            .expect("Detection should succeed");

        assert!(needs_prefix);

        fs::write(&file_missing, "G04*\nG54D10*\n").expect("Failed to rewrite missing file");

        let needs_prefix = converter
            .determine_g54_requirement(&files, &patterns)
            .expect("Detection should succeed");

        assert!(!needs_prefix);
    }
}
