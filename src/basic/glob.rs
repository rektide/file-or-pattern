//! TinyGlobbyProcessor implementation.

use crate::fop::{Fop, Pattern, ProcessorError};
use crate::processor::{AsyncProcessor, Processor};
use glob::glob;
use std::sync::Arc;

/// Processor for expanding glob patterns.
///
/// Uses glob crate to expand patterns into multiple Fops.
/// Skips FOPs that already have filename set.
pub struct TinyGlobbyProcessor;

impl TinyGlobbyProcessor {
    /// Create a new TinyGlobbyProcessor.
    pub fn new() -> Self {
        Self
    }
}

impl Default for TinyGlobbyProcessor {
    fn default() -> Self {
        Self::new()
    }
}

impl Processor for TinyGlobbyProcessor {
    fn process<'a, I>(&self, input: I) -> impl Iterator<Item = Fop> + 'a
    where
        I: Iterator<Item = Fop> + 'a,
    {
        let name = Processor::name(self).to_string();
        input.flat_map(move |fop| {
            // Skip if filename already set (don't glob concrete files)
            if fop.filename.is_some() {
                return vec![fop].into_iter();
            }

            // Try to expand glob pattern
            let pattern = fop.file_or_pattern.clone();
            match glob(&pattern) {
                Ok(paths) => {
                    let mut results = Vec::new();

                    for entry in paths {
                        match entry {
                            Ok(path) => {
                                let mut new_fop = fop.clone();
                                new_fop.filename = Some(path);
                                results.push(new_fop);
                            }
                            Err(e) => {
                                let err = ProcessorError::new(
                                    name.as_str(),
                                    format!("Failed to read glob entry: {}", e),
                                );
                                let mut error_fop = fop.clone();
                                error_fop.err = Some(err);
                                results.push(error_fop);
                            }
                        }
                    }

                    // Add pattern to all results using Arc for cheap cloning
                    if !results.is_empty() {
                        let pattern_arc = Arc::new(Pattern::new(&*pattern));
                        for fop in &mut results {
                            fop.pattern = Some(pattern_arc.clone());
                        }
                    }

                    results.into_iter()
                }
                Err(e) => {
                    // Pattern is invalid, pass through with error
                    let err =
                        ProcessorError::new(name.as_str(), format!("Invalid glob pattern: {}", e));
                    let mut error_fop = fop;
                    error_fop.err = Some(err);
                    vec![error_fop].into_iter()
                }
            }
        })
    }

    fn name(&self) -> &str {
        "TinyGlobbyProcessor"
    }
}

impl AsyncProcessor for TinyGlobbyProcessor {
    fn name(&self) -> &'static str {
        "TinyGlobbyProcessor"
    }

    async fn process_one(&self, fop: Fop) -> Vec<Fop> {
        let name = "TinyGlobbyProcessor";
        let file_or_pattern_for_error = fop.file_or_pattern.clone();

        tokio::task::spawn_blocking(move || {
            let file_or_pattern = fop.file_or_pattern.clone();

            if fop.filename.is_some() {
                return vec![fop];
            }

            let pattern = fop.file_or_pattern.clone();
            match glob(&pattern) {
                Ok(paths) => {
                    let mut results = Vec::new();

                    for entry in paths {
                        match entry {
                            Ok(path) => {
                                let mut new_fop = fop.clone();
                                new_fop.filename = Some(path);
                                results.push(new_fop);
                            }
                            Err(e) => {
                                let err = ProcessorError::new(
                                    name,
                                    format!("Failed to read glob entry: {}", e),
                                );
                                let mut error_fop = fop.clone();
                                error_fop.err = Some(err);
                                results.push(error_fop);
                            }
                        }
                    }

                    if !results.is_empty() {
                        let pattern_arc = Arc::new(Pattern::new(&*file_or_pattern));
                        for fop in &mut results {
                            fop.pattern = Some(pattern_arc.clone());
                        }
                    }

                    results
                }
                Err(e) => {
                    let err =
                        ProcessorError::new(name, format!("Invalid glob pattern: {}", e));
                    let mut error_fop = fop;
                    error_fop.err = Some(err);
                    vec![error_fop]
                }
            }
        })
        .await
        .unwrap_or_else(|e| {
            let err = ProcessorError::new(name, format!("Task join error: {}", e));
            let mut error_fop = Fop::new(&*file_or_pattern_for_error);
            error_fop.err = Some(err);
            vec![error_fop]
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;
    use tempfile::tempdir;

    #[test]
    fn test_tiny_globby_processor() {
        let processor = TinyGlobbyProcessor::new();
        assert_eq!(Processor::name(&processor), "TinyGlobbyProcessor");
    }

    #[test]
    fn test_glob_pattern_expansion() {
        let dir = tempdir().unwrap();
        let dir_path = dir.path();

        // Create test files
        fs::write(dir_path.join("test1.txt"), "content1").unwrap();
        fs::write(dir_path.join("test2.txt"), "content2").unwrap();
        fs::write(dir_path.join("other.rs"), "rust").unwrap();

        // Create glob pattern
        let pattern = dir_path.join("*.txt").to_str().unwrap().to_string();

        let processor = TinyGlobbyProcessor::new();
        let fop = Fop::new(&pattern);

        let results: Vec<_> = processor.process(vec![fop].into_iter()).collect();

        assert!(results.len() >= 2);
        assert!(results[0].pattern.is_some());
    }

    #[test]
    fn test_glob_skips_existing_filename() {
        let processor = TinyGlobbyProcessor::new();
        let mut fop = Fop::new("*.txt");
        fop.filename = Some(PathBuf::from("/concrete/path.txt"));

        let results: Vec<_> = processor.process(vec![fop].into_iter()).collect();

        assert_eq!(results.len(), 1);
        assert_eq!(
            results[0].filename.as_ref().unwrap(),
            &PathBuf::from("/concrete/path.txt")
        );
    }

    #[test]
    fn test_glob_invalid_pattern() {
        let processor = TinyGlobbyProcessor::new();
        let fop = Fop::new("[invalid[.txt");

        let results: Vec<_> = processor.process(vec![fop].into_iter()).collect();

        assert_eq!(results.len(), 1);
        assert!(results[0].err.is_some());
        assert_eq!(
            results[0].err.as_ref().unwrap().processor,
            "TinyGlobbyProcessor"
        );
    }

    #[test]
    fn test_glob_no_matches() {
        let processor = TinyGlobbyProcessor::new();
        let fop = Fop::new("/nonexistent/path/*.txt");

        let results: Vec<_> = processor.process(vec![fop].into_iter()).collect();

        // Should return empty results (no matches)
        assert_eq!(results.len(), 0);
    }

    #[test]
    fn test_default() {
        let processor = TinyGlobbyProcessor::default();
        assert_eq!(Processor::name(&processor), "TinyGlobbyProcessor");
    }

    #[tokio::test]
    async fn test_async_tiny_globby_processor() {
        let processor = TinyGlobbyProcessor::new();
        assert_eq!(AsyncProcessor::name(&processor), "TinyGlobbyProcessor");
    }

    #[tokio::test]
    async fn test_async_glob_pattern_expansion() {
        let dir = tempdir().unwrap();
        let dir_path = dir.path();

        fs::write(dir_path.join("test1.txt"), "content1").unwrap();
        fs::write(dir_path.join("test2.txt"), "content2").unwrap();
        fs::write(dir_path.join("other.rs"), "rust").unwrap();

        let pattern = dir_path.join("*.txt").to_str().unwrap().to_string();

        let processor = TinyGlobbyProcessor::new();
        let fop = Fop::new(&pattern);

        let results = processor.process_one(fop).await;

        assert!(results.len() >= 2);
        assert!(results[0].pattern.is_some());
    }

    #[tokio::test]
    async fn test_async_glob_skips_existing_filename() {
        let processor = TinyGlobbyProcessor::new();
        let mut fop = Fop::new("*.txt");
        fop.filename = Some(PathBuf::from("/concrete/path.txt"));

        let results = processor.process_one(fop).await;

        assert_eq!(results.len(), 1);
        assert_eq!(
            results[0].filename.as_ref().unwrap(),
            &PathBuf::from("/concrete/path.txt")
        );
    }

    #[tokio::test]
    async fn test_async_glob_invalid_pattern() {
        let processor = TinyGlobbyProcessor::new();
        let fop = Fop::new("[invalid[.txt");

        let results = processor.process_one(fop).await;

        assert_eq!(results.len(), 1);
        assert!(results[0].err.is_some());
        assert_eq!(
            results[0].err.as_ref().unwrap().processor,
            "TinyGlobbyProcessor"
        );
    }

    #[tokio::test]
    async fn test_async_glob_no_matches() {
        let processor = TinyGlobbyProcessor::new();
        let fop = Fop::new("/nonexistent/path/*.txt");

        let results = processor.process_one(fop).await;

        assert_eq!(results.len(), 0);
    }
}
