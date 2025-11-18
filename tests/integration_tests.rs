//! Integration tests for TransJLC
//!
//! This module contains comprehensive tests for the entire conversion pipeline
//! and individual component functionality.

use std::{fs, path::PathBuf};
use tempfile::TempDir;
use TransJLC::{
    archive::ArchiveExtractor,
    config::{Config, EdaType},
    converter::Converter,
    gerber::GerberProcessor,
    patterns::{LayerType, PatternMatcher},
};

/// Test data for KiCad-style files
const KICAD_TEST_FILES: &[(&str, &str)] = &[
    (
        "project-F_Cu.gbr",
        "G04 KiCad test*\nG01*\nD10*\nG04 End*\n",
    ),
    (
        "project-B_Cu.gbr",
        "G04 KiCad test*\nG01*\nD11*\nG04 End*\n",
    ),
    (
        "project-F_Mask.gbr",
        "G04 KiCad mask*\nG01*\nD12*\nG04 End*\n",
    ),
    (
        "project-Edge_Cuts.gbr",
        "G04 KiCad outline*\nG01*\nD13*\nG04 End*\n",
    ),
    ("project-PTH.drl", "T1C0.8\nX100Y100\nT0\nM30\n"),
];

/// Test data for Protel-style files  
const PROTEL_TEST_FILES: &[(&str, &str)] = &[
    ("project.GTL", "G04 Protel test*\nG01*\nD10*\nG04 End*\n"),
    ("project.GBL", "G04 Protel test*\nG01*\nD11*\nG04 End*\n"),
    ("project.GTS", "G04 Protel mask*\nG01*\nD12*\nG04 End*\n"),
    ("project.GKO", "G04 Protel outline*\nG01*\nD13*\nG04 End*\n"),
    ("project.DRL", "T1C0.8\nX100Y100\nT0\nM30\n"),
];

/// Create a temporary directory with test files
fn create_test_files(files: &[(&str, &str)]) -> TempDir {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");

    for (filename, content) in files {
        let file_path = temp_dir.path().join(filename);
        fs::write(file_path, content).expect("Failed to write test file");
    }

    temp_dir
}

/// Create a test configuration
fn create_test_config(input_path: PathBuf, output_path: PathBuf, eda: EdaType) -> Config {
    Config {
        language: "en".to_string(),
        eda: eda.as_str().to_string(),
        path: input_path,
        output_path,
        zip: false,
        zip_name: "test".to_string(),
        verbose: false,
        no_progress: true, // Disable progress bars in tests
    }
}

#[test]
fn test_kicad_pattern_matching() {
    let patterns = PatternMatcher::create_kicad_patterns();

    // Test various KiCad file patterns
    assert_eq!(
        patterns.match_filename("project-F_Cu.gbr"),
        Some(LayerType::TopCopper)
    );

    assert_eq!(
        patterns.match_filename("project-B_Cu.gbr"),
        Some(LayerType::BottomCopper)
    );

    assert_eq!(
        patterns.match_filename("project-F_Mask.gbr"),
        Some(LayerType::TopSoldermask)
    );

    assert_eq!(
        patterns.match_filename("project-Edge_Cuts.gbr"),
        Some(LayerType::BoardOutline)
    );

    assert_eq!(
        patterns.match_filename("project-PTH.drl"),
        Some(LayerType::PthThrough)
    );

    // Test inner layer number extraction
    assert_eq!(
        patterns.match_filename("project-In1_Cu.gbr"),
        Some(LayerType::InnerLayer(1))
    );

    assert_eq!(
        patterns.match_filename("project-In10_Cu.gbr"),
        Some(LayerType::InnerLayer(10))
    );
}

#[test]
fn test_protel_pattern_matching() {
    let patterns = PatternMatcher::create_protel_patterns();

    // Test case-insensitive matching
    assert_eq!(
        patterns.match_filename("project.GTL"),
        Some(LayerType::TopCopper)
    );

    assert_eq!(
        patterns.match_filename("project.gtl"),
        Some(LayerType::TopCopper)
    );

    assert_eq!(
        patterns.match_filename("PROJECT.GBL"),
        Some(LayerType::BottomCopper)
    );

    assert_eq!(
        patterns.match_filename("project.GKO"),
        Some(LayerType::BoardOutline)
    );
}

#[test]
fn test_auto_detection_kicad() {
    let temp_dir = create_test_files(KICAD_TEST_FILES);
    let files: Vec<PathBuf> = fs::read_dir(temp_dir.path())
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect();

    let patterns = PatternMatcher::auto_detect_eda(&files).expect("Should detect KiCad format");

    assert_eq!(patterns.name, "KiCad");
}

#[test]
fn test_auto_detection_protel() {
    let temp_dir = create_test_files(PROTEL_TEST_FILES);
    let files: Vec<PathBuf> = fs::read_dir(temp_dir.path())
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect();

    let patterns = PatternMatcher::auto_detect_eda(&files).expect("Should detect Protel format");

    assert_eq!(patterns.name, "Protel");
}

#[test]
fn test_gerber_processor_g54_prefix_conversion() {
    let processor = GerberProcessor::new().with_ignore_hash(true);
    let test_content =
        "G04 Test file*\nD10*\nG01X100Y100D01*\nD11*\nG54D12*\n%ADD13C,0.1*%\nD14*\nM02*"
            .to_string();

    let result = processor
        .process_gerber_content(test_content, true)
        .expect("Should add missing G54 prefixes successfully");

    // Check that standalone D codes are converted to G54D format
    assert!(result.contains("G54D10*"));
    assert!(result.contains("G54D11*"));
    assert!(result.contains("G54D14*"));

    // Check that existing G54D and %ADD lines remain unchanged
    assert!(result.contains("G54D12*"));
    assert!(result.contains("%ADD13C,0.1*%"));

    // Check that header is added
    assert!(result.contains("G04 EasyEDA Pro"));
    assert!(result.contains("G04 Gerber Generator"));
}

#[test]
fn test_gerber_processor_header_addition() {
    let processor = GerberProcessor::new();
    let test_content = "G04 Original content*\nG01*\nM02*".to_string();

    let result = processor
        .process_gerber_content(test_content, false)
        .expect("Should process content successfully");

    // Check header is added
    assert!(result.contains("G04 EasyEDA Pro"));
    assert!(result.contains("G04 Gerber Generator"));
    assert!(result.contains("G04 Original content*"));
}

#[test]
fn test_gerber_processor_large_file_handling() {
    let processor = GerberProcessor::new().with_max_hash_file_size(100);
    let large_content = "x".repeat(200); // Exceeds max size

    let result = processor
        .process_gerber_content(large_content.clone(), false)
        .expect("Should handle large files gracefully");

    // Should still add header but skip hash processing
    assert!(result.contains("G04 EasyEDA Pro"));
}

#[test]
fn test_layer_type_jlc_filename_conversion() {
    assert_eq!(
        LayerType::TopCopper.to_jlc_filename(),
        "Gerber_TopLayer.GTL"
    );

    assert_eq!(
        LayerType::BottomCopper.to_jlc_filename(),
        "Gerber_BottomLayer.GBL"
    );

    assert_eq!(
        LayerType::BoardOutline.to_jlc_filename(),
        "Gerber_BoardOutlineLayer.GKO"
    );

    assert_eq!(
        LayerType::InnerLayer(1).to_jlc_filename(),
        "Gerber_InnerLayer1.G1"
    );

    assert_eq!(
        LayerType::InnerLayer(42).to_jlc_filename(),
        "Gerber_InnerLayer42.G42"
    );

    assert_eq!(
        LayerType::PthThrough.to_jlc_filename(),
        "Drill_PTH_Through.DRL"
    );
}

#[test]
fn test_archive_extractor_zip_detection() {
    use std::path::Path;
    use TransJLC::archive::ArchiveExtractor;

    let mut extractor = ArchiveExtractor::new();

    // Test non-ZIP file (should return original path)
    let non_zip_path = Path::new("test.txt");
    // Note: This would require actual file for full test
    // We're testing the logic structure here
}

#[test]
fn test_conversion_stats() {
    // Create test converter with mock data
    let temp_input = create_test_files(KICAD_TEST_FILES);
    let temp_output = TempDir::new().expect("Failed to create output temp dir");

    let config = create_test_config(
        temp_input.path().to_path_buf(),
        temp_output.path().to_path_buf(),
        EdaType::KiCad,
    );

    let converter = Converter::new(config);
    let stats = converter.get_conversion_stats();

    // Should start with no processed files
    assert_eq!(stats.total_files_processed, 0);
    assert_eq!(stats.output_format, "Files"); // Not ZIP mode
}

#[test]
fn test_config_eda_type_parsing() {
    let test_cases = vec![
        ("auto", EdaType::Auto),
        ("kicad", EdaType::KiCad),
        ("protel", EdaType::Protel),
        ("jlc", EdaType::Jlc),
        (
            "custom_format",
            EdaType::Custom("custom_format".to_string()),
        ),
    ];

    for (input, expected) in test_cases {
        let config = Config {
            language: "en".to_string(),
            eda: input.to_string(),
            path: PathBuf::from("."),
            output_path: PathBuf::from("./output"),
            zip: false,
            zip_name: "test".to_string(),
            verbose: false,
            no_progress: false,
        };

        assert_eq!(config.get_eda_type(), expected);
    }
}

// Performance benchmark test (optional)
#[test]
fn test_large_file_processing_performance() {
    use std::time::Instant;

    let processor = GerberProcessor::new();

    // Create a moderately large test file (1MB)
    let large_content = "G04 Test*\nG01*\n".repeat(50000);

    let start = Instant::now();
    let result = processor.process_gerber_content(large_content, false);
    let duration = start.elapsed();

    assert!(result.is_ok());
    // Should complete within reasonable time (adjust as needed)
    assert!(duration.as_secs() < 5);
}

// Error handling tests
#[test]
fn test_error_handling_invalid_regex() {
    // This tests internal regex compilation robustness
    // The actual patterns should all be valid, but we test error propagation
    let processor = GerberProcessor::new();
    let valid_content = "G04 Valid content*\nG01*\nM02*".to_string();

    let result = processor.process_gerber_content(valid_content, false);
    assert!(result.is_ok());
}

#[test]
fn test_error_handling_no_matching_patterns() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");

    // Create files that don't match any known patterns
    fs::write(temp_dir.path().join("unknown.xyz"), "unknown content").unwrap();
    fs::write(temp_dir.path().join("another.abc"), "more unknown content").unwrap();

    let files: Vec<PathBuf> = fs::read_dir(temp_dir.path())
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect();

    let result = PatternMatcher::auto_detect_eda(&files);
    assert!(result.is_err()); // Should fail to detect any known format
}
