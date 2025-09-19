//! Progress tracking and display using indicatif
//!
//! This module provides unified progress bar functionality
//! for various operations throughout the application.

use indicatif::{ProgressBar, ProgressStyle};
use std::time::Duration;
use tracing::info;

/// Progress tracker for TransJLC operations
pub struct ProgressTracker {
    enabled: bool,
}

impl ProgressTracker {
    /// Create a new progress tracker
    pub fn new(enabled: bool) -> Self {
        Self { enabled }
    }

    /// Create a progress bar for file operations
    pub fn create_file_progress(&self, total: usize, operation: &str) -> Option<ProgressBar> {
        if !self.enabled || total == 0 {
            return None;
        }

        let pb = ProgressBar::new(total as u64);
        pb.set_style(
            ProgressStyle::default_bar()
                .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} ({percent}%) {msg}")
                .unwrap_or_else(|_| ProgressStyle::default_bar())
                .progress_chars("█▉▊▋▌▍▎▏ ")
        );
        pb.set_message(format!("{}...", operation));
        pb.enable_steady_tick(Duration::from_millis(100));

        info!("Started progress tracking for: {}", operation);
        Some(pb)
    }

    /// Create a spinner for indeterminate operations
    pub fn create_spinner(&self, message: &str) -> Option<ProgressBar> {
        if !self.enabled {
            return None;
        }

        let pb = ProgressBar::new_spinner();
        pb.set_style(
            ProgressStyle::default_spinner()
                .template("{spinner:.green} {msg}")
                .unwrap_or_else(|_| ProgressStyle::default_spinner()),
        );
        pb.set_message(message.to_string());
        pb.enable_steady_tick(Duration::from_millis(80));

        info!("Started spinner for: {}", message);
        Some(pb)
    }

    /// Create a progress bar for conversion operations
    pub fn create_conversion_progress(&self, total: usize) -> Option<ProgressBar> {
        self.create_file_progress(total, "Converting files")
    }

    /// Create a progress bar for archive operations
    pub fn create_archive_progress(
        &self,
        total: usize,
        is_extraction: bool,
    ) -> Option<ProgressBar> {
        let operation = if is_extraction {
            "Extracting archive"
        } else {
            "Creating archive"
        };
        self.create_file_progress(total, operation)
    }

    /// Update progress and optionally change message
    pub fn update_progress(pb: &Option<ProgressBar>, increment: u64, message: Option<&str>) {
        if let Some(ref progress) = pb {
            progress.inc(increment);
            if let Some(msg) = message {
                progress.set_message(msg.to_string());
            }
        }
    }

    /// Finish progress with success message
    pub fn finish_progress(pb: Option<ProgressBar>, success_message: &str) {
        if let Some(progress) = pb {
            progress.finish_with_message(success_message.to_string());
            info!("Progress completed: {}", success_message);
        }
    }

    /// Finish progress with error message
    pub fn finish_with_error(pb: Option<ProgressBar>, error_message: &str) {
        if let Some(progress) = pb {
            progress.abandon_with_message(format!("❌ {}", error_message));
        }
    }

    /// Create a multi-progress for complex operations
    pub fn create_multi_progress(&self) -> Option<indicatif::MultiProgress> {
        if !self.enabled {
            return None;
        }

        Some(indicatif::MultiProgress::new())
    }
}

/// Utility trait for easy progress tracking integration
pub trait WithProgress<T> {
    /// Execute operation with progress tracking
    fn with_progress<F>(self, tracker: &ProgressTracker, operation: &str, f: F) -> T
    where
        F: FnOnce(&Option<ProgressBar>) -> T;
}

impl<I, T> WithProgress<Vec<T>> for I
where
    I: IntoIterator,
    I::Item: Clone,
{
    fn with_progress<F>(self, tracker: &ProgressTracker, operation: &str, f: F) -> Vec<T>
    where
        F: FnOnce(&Option<ProgressBar>) -> Vec<T>,
    {
        let items: Vec<_> = self.into_iter().collect();
        let pb = tracker.create_file_progress(items.len(), operation);
        let result = f(&pb);
        ProgressTracker::finish_progress(pb, &format!("{} completed successfully", operation));
        result
    }
}

/// Progress-aware operation wrapper
pub struct ProgressAwareOperation<T> {
    pub total: usize,
    pub current: usize,
    pub data: T,
}

impl<T> ProgressAwareOperation<T> {
    pub fn new(data: T, total: usize) -> Self {
        Self {
            total,
            current: 0,
            data,
        }
    }

    pub fn increment(&mut self) {
        self.current += 1;
    }

    pub fn progress_fraction(&self) -> f64 {
        if self.total == 0 {
            0.0
        } else {
            self.current as f64 / self.total as f64
        }
    }

    pub fn is_complete(&self) -> bool {
        self.current >= self.total
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_progress_tracker_creation() {
        let enabled_tracker = ProgressTracker::new(true);
        let disabled_tracker = ProgressTracker::new(false);

        assert!(enabled_tracker.enabled);
        assert!(!disabled_tracker.enabled);
    }

    #[test]
    fn test_progress_bar_creation_when_disabled() {
        let tracker = ProgressTracker::new(false);
        let pb = tracker.create_file_progress(10, "test operation");

        assert!(pb.is_none());
    }

    #[test]
    fn test_progress_bar_creation_when_enabled() {
        let tracker = ProgressTracker::new(true);
        let pb = tracker.create_file_progress(10, "test operation");

        assert!(pb.is_some());
    }

    #[test]
    fn test_progress_aware_operation() {
        let mut op = ProgressAwareOperation::new("test", 3);

        assert_eq!(op.progress_fraction(), 0.0);
        assert!(!op.is_complete());

        op.increment();
        assert_eq!(op.current, 1);
        assert!((op.progress_fraction() - 0.333).abs() < 0.01);

        op.increment();
        op.increment();
        assert!(op.is_complete());
        assert_eq!(op.progress_fraction(), 1.0);
    }

    #[test]
    fn test_zero_total_progress() {
        let tracker = ProgressTracker::new(true);
        let pb = tracker.create_file_progress(0, "empty operation");

        assert!(pb.is_none());
    }

    #[test]
    fn test_spinner_creation() {
        let enabled_tracker = ProgressTracker::new(true);
        let disabled_tracker = ProgressTracker::new(false);

        let enabled_spinner = enabled_tracker.create_spinner("Processing...");
        let disabled_spinner = disabled_tracker.create_spinner("Processing...");

        assert!(enabled_spinner.is_some());
        assert!(disabled_spinner.is_none());
    }
}
