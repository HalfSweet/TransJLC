//! Gerber file processing and format conversion
//!
//! This module handles Gerber file format-specific operations including
//! aperture prefix normalization and hash aperture generation.

use crate::error::Result;
use anyhow::Context;
use md5::{Digest, Md5};
use rand::Rng;
use regex::Regex;
use tracing::{debug, info, warn};

/// Gerber file processor for format-specific conversions
pub struct GerberProcessor {
    /// Whether to ignore hash aperture generation
    ignore_hash: bool,

    /// Whether this is an imported PCB document
    is_imported_pcb_doc: bool,

    /// Whether to inject a synthetic EasyEDA Pro header
    inject_header: bool,

    /// Maximum file size for hash processing (bytes)
    max_hash_file_size: usize,
}

impl Default for GerberProcessor {
    fn default() -> Self {
        Self {
            ignore_hash: false,
            is_imported_pcb_doc: false,
            inject_header: true,
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

    /// Configure whether to inject a synthetic EasyEDA Pro header
    pub fn with_inject_header(mut self, inject: bool) -> Self {
        self.inject_header = inject;
        self
    }

    /// Configure maximum file size for hash processing
    pub fn with_max_hash_file_size(mut self, size: usize) -> Self {
        self.max_hash_file_size = size;
        self
    }

    /// Process a Gerber file content with all necessary transformations
    pub fn process_gerber_content(
        &self,
        content: String,
        needs_g54_aperture_prefix: bool,
    ) -> Result<String> {
        info!("Processing Gerber files...");

        let mut processed_content = self.normalize_gerber_content(content)?;

        // Add header information
        if self.inject_header {
            processed_content = self.add_gerber_header(processed_content);
        }

        // Apply aperture prefix normalization when required
        if needs_g54_aperture_prefix {
            debug!("Ensuring G54 aperture prefixes are present");
            processed_content = self.add_missing_g54_aperture_prefix(processed_content)?;
        }

        // Add hash aperture for file fingerprinting
        processed_content = self.add_hash_aperture_to_gerber(processed_content)?;

        info!("Gerber file processing completed");
        Ok(processed_content)
    }

    /// Apply Gerber normalizations that are safe no-ops on standard exports
    fn normalize_gerber_content(&self, content: String) -> Result<String> {
        let mut result = content;
        result = self.split_combined_fs_mo(result)?;
        result = self.strip_deprecated_attribute_blocks(result)?;
        result = self.normalize_aperture_decimals(result)?;
        Ok(result)
    }

    /// Normalize Excellon drill tool diameters that omit the leading zero
    pub fn normalize_excellon_drill_content(&self, content: String) -> Result<String> {
        let tool_regex =
            Regex::new(r"(?m)^(T\d+C)\.(\d)").context("Failed to compile Excellon tool regex")?;
        Ok(tool_regex.replace_all(&content, "${1}0.${2}").to_string())
    }

    /// Split combined Gerber extended commands such as %FS...*MO...*%
    fn split_combined_fs_mo(&self, content: String) -> Result<String> {
        let fs_mo_regex = Regex::new(r"%\s*(FS[^*%]*)\*\s*(MO[^*%]*)\*\s*%")
            .context("Failed to compile combined FS/MO regex")?;
        let mo_fs_regex = Regex::new(r"%\s*(MO[^*%]*)\*\s*(FS[^*%]*)\*\s*%")
            .context("Failed to compile combined MO/FS regex")?;

        let result = fs_mo_regex
            .replace_all(&content, "%${1}*%\n%${2}*%")
            .to_string();
        Ok(mo_fs_regex
            .replace_all(&result, "%${1}*%\n%${2}*%")
            .to_string())
    }

    /// Remove deprecated default extended attribute blocks rejected by stricter parsers
    fn strip_deprecated_attribute_blocks(&self, content: String) -> Result<String> {
        let deprecated_regex = Regex::new(r"%(?:IR|IP|OF|MI|SF)[^%]*%\n?")
            .context("Failed to compile deprecated attribute regex")?;
        Ok(deprecated_regex.replace_all(&content, "").to_string())
    }

    /// Add leading zeros to decimal values inside aperture definitions only
    fn normalize_aperture_decimals(&self, content: String) -> Result<String> {
        let aperture_line_regex = Regex::new(r"(?m)^%ADD\d+[A-Z][^*]*\*%$")
            .context("Failed to compile aperture line regex")?;
        let leading_dot_regex =
            Regex::new(r"([,X])\.(\d)").context("Failed to compile leading dot regex")?;

        Ok(aperture_line_regex
            .replace_all(&content, |caps: &regex::Captures| {
                leading_dot_regex
                    .replace_all(&caps[0], "${1}0.${2}")
                    .to_string()
            })
            .to_string())
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

    /// Convert aperture format from Dx* to G54Dx* when missing
    fn add_missing_g54_aperture_prefix(&self, content: String) -> Result<String> {
        info!("Converting aperture selections to include G54 prefixes");

        let lines: Vec<&str> = content.split('\n').collect();
        let mut result_lines = Vec::new();
        // Match lines that start with Dx* (optional whitespace before D)
        let aperture_regex =
            Regex::new(r"^(\s*)(D\d{2,4}\*)").context("Failed to compile aperture regex")?;

        for line in lines {
            let trimmed = line.trim_start();
            if trimmed.starts_with("%ADD") || trimmed.starts_with("G54D") {
                result_lines.push(line.to_string());
                continue;
            }

            if let Some(caps) = aperture_regex.captures(line) {
                let leading_ws = caps.get(1).map_or("", |m| m.as_str());
                let aperture = caps.get(2).map_or("", |m| m.as_str());
                let matched_len = caps.get(0).map_or(0, |m| m.end());
                let rest = &line[matched_len..];
                result_lines.push(format!("{leading_ws}G54{aperture}{rest}"));
            } else {
                result_lines.push(line.to_string());
            }
        }

        debug!("Aperture prefix normalization completed");
        Ok(result_lines.join("\n"))
    }

    /// Check if the content still has apertures missing the G54 prefix
    pub(crate) fn has_missing_g54_aperture_prefix(&self, content: &str) -> Result<bool> {
        let aperture_regex =
            Regex::new(r"^(\s*)D\d{2,4}\*").context("Failed to compile aperture regex")?;

        for line in content.lines() {
            let trimmed = line.trim_start();
            if trimmed.starts_with("%ADD") || trimmed.starts_with("G54D") {
                continue;
            }

            if aperture_regex.is_match(line) {
                return Ok(true);
            }
        }

        Ok(false)
    }

    /// Add hash aperture to Gerber file for fingerprinting
    fn add_hash_aperture_to_gerber(&self, content: String) -> Result<String> {
        if self.ignore_hash || content.len() > self.max_hash_file_size {
            if content.len() > self.max_hash_file_size {
                warn!(
                    "File too large for hash processing ({} bytes), skipping",
                    content.len()
                );
            }
            return Ok(content);
        }

        info!("Adding hash aperture to Gerber file");

        let result = self.add_hash_aperture_fixed(content)?;

        debug!("Hash aperture added successfully");
        Ok(result)
    }

    /// Append a JLC-validator-compatible hash aperture without renumbering existing apertures
    fn add_hash_aperture_fixed(&self, content: String) -> Result<String> {
        let aperture_regex = Regex::new(r"(?m)^%ADD(\d{2,4})(\D[^\n]*)$")
            .context("Failed to compile aperture definition regex")?;
        let matches: Vec<_> = aperture_regex.find_iter(&content).collect();
        if matches.is_empty() {
            debug!("No aperture definitions found; skipping hash aperture");
            return Ok(content);
        }

        let use_regex = Regex::new(r"(?m)(?:^[ \t]*|G54)D(\d{2,4})\*")
            .context("Failed to compile aperture use regex")?;
        let used_numbers = use_regex
            .captures_iter(&content)
            .filter_map(|caps| caps.get(1))
            .filter_map(|m| m.as_str().parse::<u32>().ok())
            .collect::<std::collections::HashSet<_>>();

        let mut delete_ranges = Vec::new();
        for m in aperture_regex.captures_iter(&content) {
            let Some(full_match) = m.get(0) else {
                continue;
            };
            let Some(number_match) = m.get(1) else {
                continue;
            };
            let Ok(number) = number_match.as_str().parse::<u32>() else {
                continue;
            };
            if used_numbers.contains(&number) {
                continue;
            }

            let mut end = full_match.end();
            if content.as_bytes().get(end) == Some(&b'\n') {
                end += 1;
            }
            delete_ranges.push((full_match.start(), end));
        }

        let mut cleaned = content;
        for (start, end) in delete_ranges.into_iter().rev() {
            cleaned.replace_range(start..end, "");
        }

        let aperture_layout = {
            let matches_after: Vec<_> = aperture_regex.find_iter(&cleaned).collect();
            matches_after.last().map(|last_match| {
                let max_number = aperture_regex
                    .captures_iter(&cleaned)
                    .filter_map(|caps| caps.get(1))
                    .filter_map(|m| m.as_str().parse::<u32>().ok())
                    .max()
                    .unwrap_or(9);
                (max_number, last_match.end())
            })
        };

        let (target_number, insertion_point) =
            if let Some((max_number, last_match_end)) = aperture_layout {
                let mut insertion_point = last_match_end;
                if cleaned.as_bytes().get(insertion_point) == Some(&b'\n') {
                    insertion_point += 1;
                } else {
                    cleaned.insert(insertion_point, '\n');
                    insertion_point += 1;
                }

                (max_number + 1, insertion_point)
            } else {
                if !cleaned.ends_with('\n') {
                    cleaned.push('\n');
                }
                (10, cleaned.len())
            };

        let mut hasher = Md5::new();
        hasher.update(cleaned.as_bytes());
        let hash_hex = format!("{:x}", hasher.finalize());
        let last_two_hex = &hash_hex[hash_hex.len() - 2..];
        let hash_number = u32::from_str_radix(last_two_hex, 16).unwrap_or(0) % 100;

        let mut rng = rand::thread_rng();
        let random_prefix = rng.gen_range(0..=99);
        let mut size = format!("0.{random_prefix:02}{hash_number:02}");
        if size == "0.0000" {
            size = "0.0100".to_string();
        }

        let hash_line = format!("%ADD{target_number}C,{size}*%\n");
        Ok(format!(
            "{}{}{}",
            &cleaned[..insertion_point],
            hash_line,
            &cleaned[insertion_point..]
        ))
    }

    /// Verify the final hash aperture using the same stripping rule as JLC's validator
    pub fn verify_hash_aperture(&self, content: &str) -> Result<bool> {
        let aperture_regex = Regex::new(r"(?m)^%ADD(\d{2,4})(\D[^\n]*)$")
            .context("Failed to compile aperture verifier regex")?;
        let Some(last_match) = aperture_regex.find_iter(content).last() else {
            return Ok(true);
        };

        let line = last_match.as_str();
        let circle_size_regex =
            Regex::new(r"C,(\d+(?:\.\d+))\*%\r?$").context("Failed to compile hash size regex")?;
        let Some(caps) = circle_size_regex.captures(line) else {
            return Ok(true);
        };
        let Some(size_match) = caps.get(1) else {
            return Ok(true);
        };
        let size = size_match.as_str();
        let Some((_, decimals)) = size.split_once('.') else {
            return Ok(true);
        };
        if decimals.len() != 4 {
            return Ok(true);
        }

        let expected_suffix = &decimals[decimals.len() - 2..];
        let mut end = last_match.end();
        if content.as_bytes().get(end) == Some(&b'\n') {
            end += 1;
        }

        let mut stripped = String::with_capacity(content.len() - (end - last_match.start()));
        stripped.push_str(&content[..last_match.start()]);
        stripped.push_str(&content[end..]);

        let mut hasher = Md5::new();
        hasher.update(stripped.as_bytes());
        let hash_hex = format!("{:x}", hasher.finalize());
        let last_two_hex = &hash_hex[hash_hex.len() - 2..];
        let actual_suffix = format!(
            "{:02}",
            u32::from_str_radix(last_two_hex, 16).unwrap_or(0) % 100
        );

        Ok(actual_suffix == expected_suffix)
    }
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
    fn test_g54_aperture_conversion() {
        let processor = GerberProcessor::new();
        let test_content = "G04 Test*\nD10*\nG54D11*\n%ADD12C,0.1*%\nD13*".to_string();

        let result = processor
            .add_missing_g54_aperture_prefix(test_content)
            .unwrap();

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
    fn test_hash_aperture_is_appended_and_verifies() {
        let processor = GerberProcessor::new();
        let test_content = "%FSLAX24Y24*%\n%MOMM*%\n%ADD10C,0.1*%\n%ADD11C,0.2*%\nG54D10*\nX0Y0D02*\nG54D11*\nX100Y100D01*\nM02*\n".to_string();

        let result = processor.add_hash_aperture_to_gerber(test_content).unwrap();

        assert!(result.contains("%ADD10C,0.1*%"));
        assert!(result.contains("%ADD11C,0.2*%"));
        assert!(result.contains("G54D11*"));
        assert!(result.contains("%ADD12C,0."));
        assert!(processor.verify_hash_aperture(&result).unwrap());
    }

    #[test]
    fn test_stale_unreferenced_hash_aperture_is_removed() {
        let processor = GerberProcessor::new();
        let test_content = "%ADD10C,0.1*%\n%ADD11C,0.1234*%\nG54D10*\nX0Y0D02*\nM02*\n".to_string();

        let result = processor.add_hash_aperture_to_gerber(test_content).unwrap();

        assert!(!result.contains("%ADD11C,0.1234*%"));
        assert!(result.contains("%ADD11C,0."));
        assert!(processor.verify_hash_aperture(&result).unwrap());
    }

    #[test]
    fn test_large_file_handling() {
        let processor = GerberProcessor::new().with_max_hash_file_size(100);
        let large_content = "x".repeat(200); // Exceeds max size

        let result = processor
            .add_hash_aperture_to_gerber(large_content.clone())
            .unwrap();

        // Should return original content unchanged for large files
        assert_eq!(result, large_content);
    }

    #[test]
    fn test_detect_missing_g54_prefix() {
        let processor = GerberProcessor::new();
        let needs_fix = processor
            .has_missing_g54_aperture_prefix("D10*\nG54D11*\n")
            .unwrap();

        assert!(needs_fix);

        let no_fix = processor
            .has_missing_g54_aperture_prefix("G54D10*\n%ADD12C,0.1*%\n")
            .unwrap();

        assert!(!no_fix);

        let inline_only = processor
            .has_missing_g54_aperture_prefix("X30584000Y-7866000D03*\n")
            .unwrap();

        assert!(!inline_only);
    }

    #[test]
    fn test_inline_d_codes_not_modified() {
        let processor = GerberProcessor::new();
        let content = "X30584000Y-7866000D03*\nD10*\n";
        let result = processor
            .add_missing_g54_aperture_prefix(content.to_string())
            .unwrap();

        assert!(result.contains("X30584000Y-7866000D03*"));
        assert!(result.contains("G54D10*"));
    }

    #[test]
    fn test_universal_gerber_normalization() {
        let processor = GerberProcessor::new().with_ignore_hash(true);
        let content = "%FSLAX24Y24* MISSING?%\n%FSLAX24Y24*%".to_string();
        let normalized = processor.normalize_gerber_content(content).unwrap();
        assert!(normalized.contains("%FSLAX24Y24*%"));

        let content =
            "%FSLAX24Y24*MOMM*%\n%IR0*IPPOS*OFA0B0*MIA0B0*SFA1B1*%\n%ADD10C,.15*%\n".to_string();
        let normalized = processor.normalize_gerber_content(content).unwrap();
        assert!(normalized.contains("%FSLAX24Y24*%\n%MOMM*%"));
        assert!(!normalized.contains("%IR0"));
        assert!(normalized.contains("%ADD10C,0.15*%"));
    }

    #[test]
    fn test_excellon_tool_decimal_normalization() {
        let processor = GerberProcessor::new();
        let result = processor
            .normalize_excellon_drill_content("T01C.01\nT02C0.02\n".to_string())
            .unwrap();

        assert!(result.contains("T01C0.01"));
        assert!(result.contains("T02C0.02"));
    }
}
