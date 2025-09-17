//! Gerber file processing and format conversion
//! 
//! This module handles Gerber file format-specific operations including
//! KiCad aperture format conversion and hash aperture generation.

use crate::error::Result;
use anyhow::Context;
use md5::{Digest, Md5};
use rand::Rng;
use regex::Regex;
use rust_i18n::t;
use tracing::{debug, info, warn};

/// Gerber file processor for format-specific conversions
pub struct GerberProcessor {
    /// Whether to ignore hash aperture generation
    ignore_hash: bool,
    
    /// Whether this is an imported PCB document
    is_imported_pcb_doc: bool,
    
    /// Maximum file size for hash processing (bytes)
    max_hash_file_size: usize,
}

impl Default for GerberProcessor {
    fn default() -> Self {
        Self {
            ignore_hash: false,
            is_imported_pcb_doc: false,
            max_hash_file_size: 30_000_000, // 30MB
        }
    }
}

impl GerberProcessor {
    /// Create a new Gerber processor with configuration
    pub fn new() -> Self {
        Self::default()
    }

    /// Configure whether to ignore hash aperture generation
    pub fn with_ignore_hash(mut self, ignore: bool) -> Self {
        self.ignore_hash = ignore;
        self
    }

    /// Configure whether this is an imported PCB document
    pub fn with_imported_pcb_doc(mut self, imported: bool) -> Self {
        self.is_imported_pcb_doc = imported;
        self
    }

    /// Configure maximum file size for hash processing
    pub fn with_max_hash_file_size(mut self, size: usize) -> Self {
        self.max_hash_file_size = size;
        self
    }

    /// Process a Gerber file content with all necessary transformations
    pub fn process_gerber_content(&self, content: String, is_kicad: bool) -> Result<String> {
        info!("{}", t!("gerber.processing"));
        
        let mut processed_content = content;

        // Add header information
        processed_content = self.add_gerber_header(processed_content);

        // Apply KiCad-specific transformations if needed
        if is_kicad {
            debug!("Applying KiCad-specific transformations");
            processed_content = self.convert_kicad_aperture_format(processed_content)?;
        }

        // Add hash aperture for file fingerprinting
        processed_content = self.add_hash_aperture_to_gerber(processed_content)?;

        info!("Gerber file processing completed");
        Ok(processed_content)
    }

    /// Add standard header to Gerber file
    fn add_gerber_header(&self, content: String) -> String {
        let now = chrono::Local::now();
        let header = format!(
            "G04 EasyEDA Pro v2.2.42.2, {}*\nG04 Gerber Generator version 0.3*\n",
            now.format("%Y-%m-%d %H:%M:%S")
        );

        // Normalize line endings and add header
        let normalized = content.replace("\r\n", "\n");
        format!("{}{}", header, normalized)
    }

    /// Convert KiCad aperture format from Dx* to G54Dx*
    fn convert_kicad_aperture_format(&self, content: String) -> Result<String> {
        info!("{}", t!("gerber.converting_apertures", file = "gerber"));
        
        let lines: Vec<&str> = content.split('\n').collect();
        let mut result_lines = Vec::new();
        
        // Regex to match standalone Dx* format (not part of %ADD or G54D)
        let aperture_regex = Regex::new(r"(D\d{2,4}\*)")
            .context("Failed to compile aperture regex")?;
        
        for line in lines {
            // Skip lines that already contain %ADD or G54D
            if line.contains("%ADD") || line.contains("G54D") {
                result_lines.push(line.to_string());
            } else {
                // Convert Dx* to G54Dx* in other lines
                let modified_line = aperture_regex.replace_all(line, "G54$1");
                result_lines.push(modified_line.to_string());
            }
        }
        
        debug!("KiCad aperture format conversion completed");
        Ok(result_lines.join("\n"))
    }

    /// Add hash aperture to Gerber file for fingerprinting
    fn add_hash_aperture_to_gerber(&self, content: String) -> Result<String> {
        if self.ignore_hash || content.len() > self.max_hash_file_size {
            if content.len() > self.max_hash_file_size {
                warn!("File too large for hash processing ({} bytes), skipping", content.len());
            }
            return Ok(content);
        }

        info!("Adding hash aperture to Gerber file");
        
        let aperture_info = self.analyze_apertures(&content)?;
        let hash_aperture = self.generate_hash_aperture(&content, &aperture_info)?;
        let result = self.insert_hash_aperture(content, hash_aperture, &aperture_info)?;
        
        debug!("Hash aperture added successfully");
        Ok(result)
    }

    /// Analyze existing apertures in the Gerber file
    fn analyze_apertures(&self, content: &str) -> Result<ApertureInfo> {
        let lines: Vec<&str> = content.split('\n').collect();
        let aperture_regex = Regex::new(r"^%ADD(\d{2,4})\D.*")
            .context("Failed to compile aperture analysis regex")?;
        let aperture_macro_regex = Regex::new(r"^%AD|^%AM")
            .context("Failed to compile aperture macro regex")?;

        let mut aperture_definitions = Vec::new();
        let mut aperture_numbers = Vec::new();
        let mut found_aperture = false;
        let number_max = 9999u32;

        // Scan for aperture definitions (limit to first 200 lines or until non-aperture content)
        for (index, line) in lines.iter().enumerate() {
            if index > 200 
                && (!aperture_macro_regex.is_match(line) || index > 200 + (number_max as usize) * 2) {
                break;
            }

            if let Some(caps) = aperture_regex.captures(line) {
                if let Some(num_str) = caps.get(1) {
                    if let Ok(num) = num_str.as_str().parse::<u32>() {
                        aperture_definitions.push(line.to_string());
                        aperture_numbers.push(num);
                        found_aperture = true;
                    }
                }
            } else if found_aperture {
                break;
            }
        }

        Ok(ApertureInfo {
            definitions: aperture_definitions,
            numbers: aperture_numbers,
            max_number: number_max,
        })
    }

    /// Generate hash-based aperture definition
    fn generate_hash_aperture(&self, content: &str, aperture_info: &ApertureInfo) -> Result<HashAperture> {
        let mut rng = rand::thread_rng();
        
        // Select insertion position
        let selection_index = std::cmp::min(
            5 + rng.gen_range(0..5),
            if aperture_info.numbers.len() > 1 {
                aperture_info.numbers.len() - 1
            } else {
                0
            },
        );

        let selection_count = if aperture_info.numbers.len() <= 5 {
            aperture_info.numbers.len()
        } else {
            selection_index
        };

        let (selected_aperture, target_number) = if selection_count > 0 && selection_index < aperture_info.definitions.len() {
            (
                Some(aperture_info.definitions[selection_index].clone()),
                aperture_info.numbers[selection_index],
            )
        } else {
            // Use default values if no suitable aperture found
            let default_number = if aperture_info.numbers.is_empty() {
                10u32
            } else if aperture_info.numbers.len() <= 5 {
                aperture_info.numbers.last().unwrap() + 1
            } else {
                10u32
            };
            (None, default_number.min(aperture_info.max_number))
        };

        // Calculate hash
        let hash_content = if self.is_imported_pcb_doc {
            format!("494d{}", content)
        } else {
            content.to_string()
        };

        let mut hasher = Md5::new();
        hasher.update(hash_content.as_bytes());
        let hash_result = hasher.finalize();
        let hash_hex = format!("{:x}", hash_result);

        // Convert hash to aperture size
        let last_two_hex = &hash_hex[hash_hex.len() - 2..];
        let hash_number = u32::from_str_radix(last_two_hex, 16).unwrap_or(0) % 100;
        let hash_suffix = format!("{:02}", hash_number);

        let base_size = rng.gen_range(0.0..1.0);
        let size_with_hash = format!("{:.2}{}", base_size, hash_suffix);
        let final_size = if size_with_hash.parse::<f64>().unwrap_or(0.0) == 0.0 {
            "0.0100".to_string()
        } else {
            size_with_hash
        };

        // Create aperture definition
        let aperture_definition = if let Some(ref selected) = selected_aperture {
            let size_regex = Regex::new(r",([\d.]+)")
                .context("Failed to compile size regex")?;
            size_regex
                .replace(selected, |_: &regex::Captures| format!(",{}", final_size))
                .to_string()
        } else {
            format!("%ADD{}C,{}*%", target_number, final_size)
        };

        Ok(HashAperture {
            definition: aperture_definition,
            target_number,
            hash: hash_hex,
        })
    }

    /// Insert hash aperture into Gerber content
    fn insert_hash_aperture(&self, content: String, hash_aperture: HashAperture, aperture_info: &ApertureInfo) -> Result<String> {
        // First, renumber existing apertures to make room
        let renumbered_content = self.renumber_apertures(content, hash_aperture.target_number, aperture_info.max_number)?;
        
        // Then insert the hash aperture
        self.insert_aperture_definition(renumbered_content, hash_aperture)
    }

    /// Renumber existing apertures to make room for hash aperture
    fn renumber_apertures(&self, content: String, target_number: u32, max_number: u32) -> Result<String> {
        let aperture_renumber_regex = Regex::new(r"(?m)^(%ADD|G54D)(\d{2,4})(.*)$")
            .context("Failed to compile renumber regex")?;
        
        let renumbered = aperture_renumber_regex
            .replace_all(&content, |caps: &regex::Captures| {
                let prefix = &caps[1];
                let number: u32 = caps[2].parse().unwrap_or(0);
                let suffix = &caps[3];

                if number < target_number || number == max_number {
                    caps[0].to_string()
                } else {
                    format!("{}{}{}", prefix, number + 1, suffix)
                }
            })
            .to_string();

        Ok(renumbered)
    }

    /// Insert the hash aperture definition at the appropriate location
    fn insert_aperture_definition(&self, content: String, hash_aperture: HashAperture) -> Result<String> {
        // Try to insert before the next aperture definition
        let next_aperture_pattern = format!(r"(?m)^%ADD{}(\D)", hash_aperture.target_number + 1);
        let next_aperture_regex = Regex::new(&next_aperture_pattern)
            .context("Failed to compile next aperture regex")?;

        if next_aperture_regex.is_match(&content) {
            let result = next_aperture_regex
                .replace(&content, |caps: &regex::Captures| {
                    format!("{}\n%ADD{}{}", hash_aperture.definition, hash_aperture.target_number + 1, &caps[1])
                })
                .to_string();
            return Ok(result);
        }

        // Fallback: insert before %LP or G commands
        let lines: Vec<&str> = content.split('\n').collect();
        let mut result_lines = Vec::new();
        let mut inserted = false;
        let mut mo_found = false;

        for line in lines {
            if !mo_found && line.starts_with("%MO") {
                mo_found = true;
            } else if mo_found && !inserted && (line.starts_with("%LP") || line.starts_with("G")) {
                result_lines.push(hash_aperture.definition.as_str());
                inserted = true;
            }
            result_lines.push(line);
        }

        if !inserted {
            result_lines.push(hash_aperture.definition.as_str());
        }

        Ok(result_lines.join("\n"))
    }
}

/// Information about existing apertures in a Gerber file
#[derive(Debug)]
struct ApertureInfo {
    definitions: Vec<String>,
    numbers: Vec<u32>,
    max_number: u32,
}

/// Hash-based aperture information
#[derive(Debug)]
#[allow(dead_code)]
struct HashAperture {
    definition: String,
    target_number: u32,
    hash: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gerber_processor_creation() {
        let processor = GerberProcessor::new()
            .with_ignore_hash(true)
            .with_imported_pcb_doc(false)
            .with_max_hash_file_size(1_000_000);

        assert!(processor.ignore_hash);
        assert!(!processor.is_imported_pcb_doc);
        assert_eq!(processor.max_hash_file_size, 1_000_000);
    }

    #[test]
    fn test_kicad_aperture_conversion() {
        let processor = GerberProcessor::new();
        let test_content = "G04 Test*\nD10*\nG54D11*\n%ADD12C,0.1*%\nD13*".to_string();
        
        let result = processor.convert_kicad_aperture_format(test_content).unwrap();
        
        // D10* should be converted to G54D10*
        assert!(result.contains("G54D10*"));
        // D13* should be converted to G54D13*
        assert!(result.contains("G54D13*"));
        // G54D11* should remain unchanged
        assert!(result.contains("G54D11*"));
        // %ADD12C,0.1*% should remain unchanged
        assert!(result.contains("%ADD12C,0.1*%"));
    }

    #[test]
    fn test_header_addition() {
        let processor = GerberProcessor::new();
        let test_content = "G04 Original content*\nM02*".to_string();
        
        let result = processor.add_gerber_header(test_content);
        
        assert!(result.contains("G04 EasyEDA Pro"));
        assert!(result.contains("G04 Gerber Generator"));
        assert!(result.contains("G04 Original content*"));
    }

    #[test]
    fn test_aperture_analysis() {
        let processor = GerberProcessor::new();
        let test_content = "%ADD10C,0.1*%\n%ADD11R,0.2X0.3*%\nG04 End of apertures*".to_string();
        
        let aperture_info = processor.analyze_apertures(&test_content).unwrap();
        
        assert_eq!(aperture_info.numbers.len(), 2);
        assert!(aperture_info.numbers.contains(&10));
        assert!(aperture_info.numbers.contains(&11));
    }

    #[test]
    fn test_large_file_handling() {
        let processor = GerberProcessor::new().with_max_hash_file_size(100);
        let large_content = "x".repeat(200); // Exceeds max size
        
        let result = processor.add_hash_aperture_to_gerber(large_content.clone()).unwrap();
        
        // Should return original content unchanged for large files
        assert_eq!(result, large_content);
    }
}
