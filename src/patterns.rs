//! Pattern matching for different EDA file naming conventions
//! 
//! This module provides pattern matching capabilities for identifying
//! and mapping files from different EDA software to JLC format.

use crate::error::{Result, TransJlcError};
use regex::Regex;
use rust_i18n::t;
use std::collections::HashMap;
use std::path::Path;
use tracing::{debug, info, warn};

/// Represents a layer type in PCB files
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum LayerType {
    // Drill layers
    NpthThrough,
    PthThrough,
    PthThroughVia,
    
    // Bottom layers
    BottomSilkscreen,
    BottomSoldermask,
    BottomPasteMask,
    BottomCopper,
    
    // Top layers  
    TopSilkscreen,
    TopSoldermask,
    TopPasteMask,
    TopCopper,
    
    // Special layers
    BoardOutline,
    InnerLayer(u32), // Layer number
    
    // Other files
    Other,
}

impl LayerType {
    /// Get the JLC standard filename for this layer type
    pub fn to_jlc_filename(&self) -> String {
        match self {
            LayerType::NpthThrough => "Drill_NPTH_Through.DRL".to_string(),
            LayerType::PthThrough => "Drill_PTH_Through.DRL".to_string(),
            LayerType::PthThroughVia => "Drill_PTH_Through_Via.DRL".to_string(),
            
            LayerType::BottomSilkscreen => "Gerber_BottomSilkscreenLayer.GBO".to_string(),
            LayerType::BottomSoldermask => "Gerber_BottomSolderMaskLayer.GBS".to_string(),
            LayerType::BottomPasteMask => "Gerber_BottomPasteMaskLayer.GBP".to_string(),
            LayerType::BottomCopper => "Gerber_BottomLayer.GBL".to_string(),
            
            LayerType::TopSilkscreen => "Gerber_TopSilkscreenLayer.GTO".to_string(),
            LayerType::TopSoldermask => "Gerber_TopSolderMaskLayer.GTS".to_string(),
            LayerType::TopPasteMask => "Gerber_TopPasteMaskLayer.GTP".to_string(),
            LayerType::TopCopper => "Gerber_TopLayer.GTL".to_string(),
            
            LayerType::BoardOutline => "Gerber_BoardOutlineLayer.GKO".to_string(),
            LayerType::InnerLayer(num) => format!("Gerber_InnerLayer{}.G{}", num, num),
            
            LayerType::Other => "Unknown".to_string(),
        }
    }
}

/// Pattern matcher for a specific EDA software
#[derive(Debug, Clone)]
pub struct EdaPatterns {
    pub name: String,
    patterns: HashMap<LayerType, Vec<String>>,
}

impl EdaPatterns {
    /// Create a new pattern matcher
    pub fn new(name: String) -> Self {
        Self {
            name,
            patterns: HashMap::new(),
        }
    }

    /// Add a pattern for a specific layer type
    pub fn add_pattern(&mut self, layer_type: LayerType, pattern: String) {
        self.patterns
            .entry(layer_type)
            .or_insert_with(Vec::new)
            .push(pattern);
    }

    /// Match a filename against all patterns and return the layer type
    pub fn match_filename(&self, filename: &str) -> Option<LayerType> {
        for (layer_type, patterns) in &self.patterns {
            for pattern in patterns {
                if let Ok(regex) = Regex::new(pattern) {
                    if regex.is_match(filename) {
                        debug!("Matched '{}' to {:?} using pattern '{}'", filename, layer_type, pattern);
                        
                        // Handle inner layers specially to extract layer number
                        if matches!(layer_type, LayerType::InnerLayer(_)) {
                            return self.extract_inner_layer_number(filename, &regex);
                        }
                        
                        return Some(layer_type.clone());
                    }
                } else {
                    warn!("Invalid regex pattern: {}", pattern);
                }
            }
        }
        
        debug!("No pattern matched for filename: {}", filename);
        None
    }

    /// Extract inner layer number from filename
    fn extract_inner_layer_number(&self, filename: &str, regex: &Regex) -> Option<LayerType> {
        if let Some(caps) = regex.captures(filename) {
            // Try to find the first numeric capture group
            for i in 1..caps.len() {
                if let Some(matched) = caps.get(i) {
                    if let Ok(num) = matched.as_str().parse::<u32>() {
                        return Some(LayerType::InnerLayer(num));
                    }
                }
            }
        }
        
        // Fallback: look for any number in the filename
        let number_regex = Regex::new(r"(\d+)").ok()?;
        if let Some(caps) = number_regex.captures(filename) {
            if let Some(matched) = caps.get(1) {
                if let Ok(num) = matched.as_str().parse::<u32>() {
                    return Some(LayerType::InnerLayer(num));
                }
            }
        }
        
        None
    }

    /// Check if this pattern set can handle the given files
    pub fn can_handle_files(&self, filenames: &[String]) -> bool {
        // Check if we can find the board outline (essential layer)
        let has_board_outline = filenames.iter().any(|filename| {
            self.match_filename(filename) == Some(LayerType::BoardOutline)
        });

        has_board_outline
    }
}

/// Pattern matcher factory for different EDA software types
pub struct PatternMatcher;

impl PatternMatcher {
    /// Create patterns for KiCad
    pub fn create_kicad_patterns() -> EdaPatterns {
        let mut patterns = EdaPatterns::new("KiCad".to_string());
        
        // Drill files
        patterns.add_pattern(LayerType::NpthThrough, r"-NPTH\.drl$".to_string());
        patterns.add_pattern(LayerType::PthThrough, r"-PTH\.drl$".to_string());
        
        // Copper layers
        patterns.add_pattern(LayerType::TopCopper, r"-F_Cu\.gbr$".to_string());
        patterns.add_pattern(LayerType::BottomCopper, r"-B_Cu\.gbr$".to_string());
        patterns.add_pattern(LayerType::InnerLayer(0), r"-In(\d+)_Cu\.gbr$".to_string());
        
        // Mask layers
        patterns.add_pattern(LayerType::TopSoldermask, r"-F_Mask\.gbr$".to_string());
        patterns.add_pattern(LayerType::BottomSoldermask, r"-B_Mask\.gbr$".to_string());
        patterns.add_pattern(LayerType::TopPasteMask, r"-F_Paste\.gbr$".to_string());
        patterns.add_pattern(LayerType::BottomPasteMask, r"-B_Paste\.gbr$".to_string());
        
        // Silkscreen layers
        patterns.add_pattern(LayerType::TopSilkscreen, r"-F_Silkscreen\.gbr$".to_string());
        patterns.add_pattern(LayerType::BottomSilkscreen, r"-B_Silkscreen\.gbr$".to_string());
        
        // Board outline
        patterns.add_pattern(LayerType::BoardOutline, r"-Edge_Cuts\.gbr$".to_string());
        
        patterns
    }

    /// Create patterns for Protel/Altium Designer
    pub fn create_protel_patterns() -> EdaPatterns {
        let mut patterns = EdaPatterns::new("Protel".to_string());
        
        // Gerber files (case insensitive)
        patterns.add_pattern(LayerType::TopCopper, r"(?i)\.gtl$".to_string());
        patterns.add_pattern(LayerType::BottomCopper, r"(?i)\.gbl$".to_string());
        
        patterns.add_pattern(LayerType::TopSoldermask, r"(?i)\.gts$".to_string());
        patterns.add_pattern(LayerType::BottomSoldermask, r"(?i)\.gbs$".to_string());
        
        patterns.add_pattern(LayerType::TopPasteMask, r"(?i)\.gtp$".to_string());
        patterns.add_pattern(LayerType::BottomPasteMask, r"(?i)\.gbp$".to_string());
        
        patterns.add_pattern(LayerType::TopSilkscreen, r"(?i)\.gto$".to_string());
        patterns.add_pattern(LayerType::BottomSilkscreen, r"(?i)\.gbo$".to_string());
        
        patterns.add_pattern(LayerType::BoardOutline, r"(?i)\.gko$".to_string());
        
        // Inner layers (G1, G2, etc.)
        patterns.add_pattern(LayerType::InnerLayer(0), r"(?i)\.g(\d+)$".to_string());
        
        // Drill files
        patterns.add_pattern(LayerType::PthThrough, r"(?i)\.drl$".to_string());
        patterns.add_pattern(LayerType::Other, r"(?i)\.txt$".to_string());
        patterns.add_pattern(LayerType::Other, r"(?i)\.drr$".to_string());
        
        patterns
    }

    /// Create patterns for JLC EDA
    pub fn create_jlc_patterns() -> EdaPatterns {
        let mut patterns = EdaPatterns::new("JLC".to_string());
        
        // Already in JLC format, so patterns match the output names
        patterns.add_pattern(LayerType::NpthThrough, r"^Drill_NPTH_Through\.DRL$".to_string());
        patterns.add_pattern(LayerType::PthThrough, r"^Drill_PTH_Through\.DRL$".to_string());
        patterns.add_pattern(LayerType::PthThroughVia, r"^Drill_PTH_Through_Via\.DRL$".to_string());
        
        patterns.add_pattern(LayerType::BottomSilkscreen, r"^Gerber_BottomSilkscreenLayer\.GBO$".to_string());
        patterns.add_pattern(LayerType::BottomSoldermask, r"^Gerber_BottomSolderMaskLayer\.GBS$".to_string());
        patterns.add_pattern(LayerType::BottomPasteMask, r"^Gerber_BottomPasteMaskLayer\.GBP$".to_string());
        patterns.add_pattern(LayerType::BottomCopper, r"^Gerber_BottomLayer\.GBL$".to_string());
        
        patterns.add_pattern(LayerType::TopSilkscreen, r"^Gerber_TopSilkscreenLayer\.GTO$".to_string());
        patterns.add_pattern(LayerType::TopSoldermask, r"^Gerber_TopSolderMaskLayer\.GTS$".to_string());
        patterns.add_pattern(LayerType::TopPasteMask, r"^Gerber_TopPasteMaskLayer\.GTP$".to_string());
        patterns.add_pattern(LayerType::TopCopper, r"^Gerber_TopLayer\.GTL$".to_string());
        
        patterns.add_pattern(LayerType::BoardOutline, r"^Gerber_BoardOutlineLayer\.GKO$".to_string());
        patterns.add_pattern(LayerType::InnerLayer(0), r"^Gerber_InnerLayer(\d+)\.G(\d+)$".to_string());
        
        patterns
    }

    /// Auto-detect the EDA type from a list of files
    pub fn auto_detect_eda<P: AsRef<Path>>(files: &[P]) -> Result<EdaPatterns> {
        let filenames: Vec<String> = files
            .iter()
            .filter_map(|p| {
                p.as_ref()
                    .file_name()
                    .and_then(|name| name.to_str())
                    .map(|s| s.to_string())
            })
            .collect();

        info!("Auto-detecting EDA type from {} files", filenames.len());
        debug!("Files to analyze: {:?}", filenames);

        let patterns_to_test = vec![
            Self::create_kicad_patterns(),
            Self::create_protel_patterns(),
            Self::create_jlc_patterns(),
        ];

        for pattern in patterns_to_test {
            if pattern.can_handle_files(&filenames) {
                info!("Detected EDA type: {}", pattern.name);
                return Ok(pattern);
            }
        }

        Err(TransJlcError::NoMatchingPattern.into())
    }

    /// Create patterns for a custom EDA type (placeholder)
    pub fn create_custom_patterns(name: String) -> EdaPatterns {
        warn!("Creating custom pattern matcher for: {}", name);
        EdaPatterns::new(name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_kicad_pattern_matching() {
        let patterns = PatternMatcher::create_kicad_patterns();
        
        // Test board outline detection
        assert_eq!(
            patterns.match_filename("project-Edge_Cuts.gbr"),
            Some(LayerType::BoardOutline)
        );
        
        // Test copper layers
        assert_eq!(
            patterns.match_filename("project-F_Cu.gbr"),
            Some(LayerType::TopCopper)
        );
        
        assert_eq!(
            patterns.match_filename("project-B_Cu.gbr"),
            Some(LayerType::BottomCopper)
        );
        
        // Test inner layer with number extraction
        assert_eq!(
            patterns.match_filename("project-In1_Cu.gbr"),
            Some(LayerType::InnerLayer(1))
        );
    }

    #[test]
    fn test_protel_pattern_matching() {
        let patterns = PatternMatcher::create_protel_patterns();
        
        // Test case insensitive matching
        assert_eq!(
            patterns.match_filename("project.GTL"),
            Some(LayerType::TopCopper)
        );
        
        assert_eq!(
            patterns.match_filename("project.gtl"),
            Some(LayerType::TopCopper)
        );
    }

    #[test]
    fn test_layer_type_to_jlc_filename() {
        assert_eq!(
            LayerType::TopCopper.to_jlc_filename(),
            "Gerber_TopLayer.GTL"
        );
        
        assert_eq!(
            LayerType::InnerLayer(1).to_jlc_filename(),
            "Gerber_InnerLayer1.G1"
        );
        
        assert_eq!(
            LayerType::BoardOutline.to_jlc_filename(),
            "Gerber_BoardOutlineLayer.GKO"
        );
    }

    #[test]
    fn test_can_handle_files() {
        let patterns = PatternMatcher::create_kicad_patterns();
        
        let files_with_outline = vec![
            "project-F_Cu.gbr".to_string(),
            "project-Edge_Cuts.gbr".to_string(),
        ];
        
        let files_without_outline = vec![
            "project-F_Cu.gbr".to_string(),
            "project-B_Cu.gbr".to_string(),
        ];
        
        assert!(patterns.can_handle_files(&files_with_outline));
        assert!(!patterns.can_handle_files(&files_without_outline));
    }
}
