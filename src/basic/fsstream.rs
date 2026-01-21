//! FsstreamProcessor implementation using fsstream crate for async glob expansion.

use crate::fop::{Fop, Pattern, ProcessorError};
use crate::processor::AsyncProcessor;
use fsstream::dir_scanner::DirScanner;
use globset::Glob;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Semaphore;
use tokio_util::sync::CancellationToken;

/// Default concurrency limit for simultaneous directory scans.
const DEFAULT_CONCURRENCY: usize = 64;

/// Glob metacharacters that indicate a pattern (not a literal path).
const GLOB_METACHARACTERS: &[char] = &['*', '?', '[', '{'];

/// Processor for expanding glob patterns using async directory scanning.
///
/// Uses fsstream crate for truly async glob expansion without blocking threads.
/// Includes a per-processor semaphore to limit concurrent directory scans,
/// preventing file descriptor exhaustion when processing many patterns.
///
/// # Example
///
/// ```ignore
/// let processor = FsstreamProcessor::new()
///     .with_concurrency(32)  // Limit to 32 concurrent scans
///     .with_max_depth(10);   // Limit directory depth
///
/// let fop = Fop::new("src/**/*.rs");
/// let results = processor.process_one(fop).await;
/// ```
pub struct FsstreamProcessor {
    scanner: DirScanner,
    /// Semaphore to limit concurrent directory scans
    concurrency: Arc<Semaphore>,
}

impl FsstreamProcessor {
    /// Create a new FsstreamProcessor with default concurrency limit.
    pub fn new() -> Self {
        Self {
            scanner: DirScanner::new(),
            concurrency: Arc::new(Semaphore::new(DEFAULT_CONCURRENCY)),
        }
    }

    /// Set the concurrency limit for simultaneous directory scans.
    ///
    /// This prevents file descriptor exhaustion when processing many patterns
    /// concurrently. Default is 64.
    pub fn with_concurrency(mut self, limit: usize) -> Self {
        self.concurrency = Arc::new(Semaphore::new(limit));
        self
    }

    /// Set maximum directory depth for scanning.
    pub fn with_max_depth(mut self, depth: u32) -> Self {
        self.scanner = self.scanner.with_max_depth(depth);
        self
    }

    /// Set number of concurrent futures for scanning within each pattern.
    pub fn with_num_futures(mut self, num: usize) -> Self {
        self.scanner = self.scanner.with_num_futures(num);
        self
    }

    /// Check if pattern contains glob metacharacters.
    fn has_wildcards(pattern: &str) -> bool {
        pattern.contains(GLOB_METACHARACTERS)
    }

    /// Validate a glob pattern using globset.
    fn validate_pattern(pattern: &str) -> Result<(), ProcessorError> {
        Glob::new(pattern).map_err(|e| {
            ProcessorError::new("FsstreamProcessor", format!("Invalid glob pattern: {}", e))
        })?;
        Ok(())
    }

    /// Parse a glob pattern into (base_dir, relative_glob) using component-based analysis.
    ///
    /// This implements the "glob-to-walk-root" derivation: iterate path components
    /// and find the deepest directory that contains no glob metacharacters.
    ///
    /// # Algorithm
    ///
    /// 1. Split pattern into path components
    /// 2. Find first component containing a glob metacharacter
    /// 3. base_dir = all components before the first wildcard component
    /// 4. relative_glob = remaining components joined
    ///
    /// # Examples
    ///
    /// | Input | Base Dir | Relative Glob |
    /// |-------|----------|---------------|
    /// | `*.txt` | `.` | `*.txt` |
    /// | `src/**/*.rs` | `src` | `**/*.rs` |
    /// | `/usr/lib/**/*.so` | `/usr/lib` | `**/*.so` |
    /// | `src/foo*.rs` | `src` | `foo*.rs` |
    /// | `{src,lib}/**/*.rs` | `.` | `{src,lib}/**/*.rs` |
    fn parse_pattern(pattern: &str) -> (PathBuf, String) {
        // Handle empty pattern
        if pattern.is_empty() {
            return (PathBuf::from("."), String::new());
        }

        // Check if pattern is absolute (Unix or Windows)
        let is_absolute = pattern.starts_with('/')
            || (pattern.len() >= 2 && pattern.chars().nth(1) == Some(':'));

        // Split into components, preserving path structure
        let components: Vec<&str> = pattern.split(['/', '\\']).collect();

        // Find first component with glob metacharacters
        let first_wildcard_idx = components
            .iter()
            .position(|c| c.chars().any(|ch| GLOB_METACHARACTERS.contains(&ch)));

        match first_wildcard_idx {
            None => {
                // No wildcards - this is a literal path
                // Return "." as base since literal paths are handled specially
                (PathBuf::from("."), pattern.to_string())
            }
            Some(0) => {
                // First component has wildcard: `*.txt`, `{a,b}/**`
                (PathBuf::from("."), pattern.to_string())
            }
            Some(idx) => {
                // Build base_dir from components before wildcard
                let base_components = &components[..idx];
                let glob_components = &components[idx..];

                let base_dir = if is_absolute {
                    // For absolute paths, reconstruct with leading separator
                    if base_components.is_empty() || (base_components.len() == 1 && base_components[0].is_empty()) {
                        PathBuf::from("/")
                    } else {
                        let mut path = PathBuf::new();
                        for (i, comp) in base_components.iter().enumerate() {
                            if i == 0 && comp.is_empty() {
                                // Leading slash on Unix
                                path.push("/");
                            } else if !comp.is_empty() {
                                path.push(comp);
                            }
                        }
                        path
                    }
                } else {
                    // Relative path
                    base_components.iter().filter(|c| !c.is_empty()).collect()
                };

                let relative_glob = glob_components.join("/");

                // Ensure base_dir is not empty
                let base_dir = if base_dir.as_os_str().is_empty() {
                    PathBuf::from(".")
                } else {
                    base_dir
                };

                (base_dir, relative_glob)
            }
        }
    }
}

impl Default for FsstreamProcessor {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for FsstreamProcessor {
    fn clone(&self) -> Self {
        Self {
            scanner: self.scanner.clone(),
            concurrency: self.concurrency.clone(),
        }
    }
}

impl AsyncProcessor for FsstreamProcessor {
    fn name(&self) -> &'static str {
        "FsstreamProcessor"
    }

    async fn process_one(&self, fop: Fop) -> Vec<Fop> {
        let name = "FsstreamProcessor";
        let file_or_pattern = fop.file_or_pattern.clone();

        // Skip if filename already set (don't glob concrete files)
        if fop.filename.is_some() {
            return vec![fop];
        }

        // Fast path: literal file (no wildcards)
        if !Self::has_wildcards(&file_or_pattern) {
            let path = PathBuf::from(&*file_or_pattern);
            return if tokio::fs::try_exists(&path).await.unwrap_or(false) {
                let mut result = fop;
                result.filename = Some(path);
                vec![result]
            } else {
                // Match TinyGlobby behavior: no matches for non-existent literal paths
                vec![]
            };
        }

        // Validate pattern syntax using globset
        if let Err(err) = Self::validate_pattern(&file_or_pattern) {
            let mut error_fop = fop;
            error_fop.err = Some(err);
            return vec![error_fop];
        }

        // Parse the glob pattern using component-based analysis
        let (base_dir, glob_pattern) = Self::parse_pattern(&file_or_pattern);

        // Check if base directory exists (async)
        if !tokio::fs::try_exists(&base_dir).await.unwrap_or(false) {
            let err = ProcessorError::new(
                name,
                format!("Base directory does not exist: {}", base_dir.display()),
            );
            let mut error_fop = fop;
            error_fop.err = Some(err);
            return vec![error_fop];
        }

        // Acquire semaphore permit to limit concurrency
        let _permit = match self.concurrency.acquire().await {
            Ok(permit) => permit,
            Err(_) => {
                let mut error_fop = fop;
                error_fop.err = Some(ProcessorError::new(name, "Semaphore closed"));
                return vec![error_fop];
            }
        };

        // Create simple strategy with glob pattern
        let strategy = match self.scanner.clone().into_simple().include(glob_pattern.as_str()) {
            Ok(s) => s,
            Err(e) => {
                let err = ProcessorError::new(
                    name,
                    format!("Failed to build pattern matcher: {}", e),
                );
                let mut error_fop = fop;
                error_fop.err = Some(err);
                return vec![error_fop];
            }
        };

        // Use cancellation token for scan
        let cancel = CancellationToken::new();

        // Collect results from streaming scan
        let mut scan_handle = strategy.scan_streaming(&base_dir, cancel).await;
        let mut results = Vec::new();

        while let Some(path) = scan_handle.receiver.recv().await {
            let mut new_fop = fop.clone();
            new_fop.filename = Some(path);
            results.push(new_fop);
        }

        // Wait for scan task to complete and propagate errors
        match scan_handle.join_handle.await {
            Ok(Ok(())) => {}
            Ok(Err(e)) => {
                let mut error_fop = fop.clone();
                error_fop.err = Some(ProcessorError::new(name, format!("Scan error: {}", e)));
                if results.is_empty() {
                    return vec![error_fop];
                }
                results.push(error_fop);
            }
            Err(e) => {
                let mut error_fop = fop.clone();
                error_fop.err = Some(ProcessorError::new(name, format!("Join error: {}", e)));
                if results.is_empty() {
                    return vec![error_fop];
                }
                results.push(error_fop);
            }
        }

        // Add pattern to all successful results using Arc for cheap cloning
        if !results.is_empty() {
            let pattern_arc = Arc::new(Pattern::new(&*file_or_pattern));
            for fop in &mut results {
                if fop.err.is_none() {
                    fop.pattern = Some(pattern_arc.clone());
                }
            }
        }

        results
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_fsstream_processor() {
        let processor = FsstreamProcessor::new();
        assert_eq!(processor.name(), "FsstreamProcessor");
    }

    #[test]
    fn test_parse_pattern_component_based() {
        // Pattern starting with wildcard
        let (base, pattern) = FsstreamProcessor::parse_pattern("*.txt");
        assert_eq!(base, PathBuf::from("."));
        assert_eq!(pattern, "*.txt");

        // Pattern with wildcards after directory
        let (base, pattern) = FsstreamProcessor::parse_pattern("src/**/*.rs");
        assert_eq!(base, PathBuf::from("src"));
        assert_eq!(pattern, "**/*.rs");

        // Absolute path with wildcards
        let (base, pattern) = FsstreamProcessor::parse_pattern("/usr/lib/**/*.so");
        assert_eq!(base, PathBuf::from("/usr/lib"));
        assert_eq!(pattern, "**/*.so");

        // Wildcard in filename component only
        let (base, pattern) = FsstreamProcessor::parse_pattern("src/foo*.rs");
        assert_eq!(base, PathBuf::from("src"));
        assert_eq!(pattern, "foo*.rs");

        // No wildcards (literal path)
        let (base, pattern) = FsstreamProcessor::parse_pattern("src/lib.rs");
        assert_eq!(base, PathBuf::from("."));
        assert_eq!(pattern, "src/lib.rs");

        // Wildcard in first component
        let (base, pattern) = FsstreamProcessor::parse_pattern("foo*.txt");
        assert_eq!(base, PathBuf::from("."));
        assert_eq!(pattern, "foo*.txt");

        // Brace expansion in first component
        let (base, pattern) = FsstreamProcessor::parse_pattern("{src,lib}/**/*.rs");
        assert_eq!(base, PathBuf::from("."));
        assert_eq!(pattern, "{src,lib}/**/*.rs");

        // Nested directories before wildcard
        let (base, pattern) = FsstreamProcessor::parse_pattern("a/b/c/**/*.txt");
        assert_eq!(base, PathBuf::from("a/b/c"));
        assert_eq!(pattern, "**/*.txt");

        // Question mark wildcard
        let (base, pattern) = FsstreamProcessor::parse_pattern("src/file?.rs");
        assert_eq!(base, PathBuf::from("src"));
        assert_eq!(pattern, "file?.rs");

        // Character class
        let (base, pattern) = FsstreamProcessor::parse_pattern("src/[abc].rs");
        assert_eq!(base, PathBuf::from("src"));
        assert_eq!(pattern, "[abc].rs");
    }

    #[test]
    fn test_validate_pattern() {
        // Valid patterns
        assert!(FsstreamProcessor::validate_pattern("*.txt").is_ok());
        assert!(FsstreamProcessor::validate_pattern("src/**/*.rs").is_ok());
        assert!(FsstreamProcessor::validate_pattern("{a,b}/*.txt").is_ok());
        assert!(FsstreamProcessor::validate_pattern("[abc].txt").is_ok());

        // Invalid patterns
        assert!(FsstreamProcessor::validate_pattern("[invalid[.txt").is_err());
    }

    #[test]
    fn test_with_concurrency() {
        let processor = FsstreamProcessor::new().with_concurrency(8);
        assert_eq!(processor.concurrency.available_permits(), 8);
    }

    #[tokio::test]
    async fn test_async_fsstream_processor() {
        let processor = FsstreamProcessor::new();
        assert_eq!(processor.name(), "FsstreamProcessor");
    }

    #[tokio::test]
    async fn test_async_pattern_expansion() {
        let dir = tempdir().unwrap();
        let dir_path = dir.path();

        // Create test files
        fs::write(dir_path.join("test1.txt"), "content1").unwrap();
        fs::write(dir_path.join("test2.txt"), "content2").unwrap();
        fs::write(dir_path.join("other.rs"), "rust").unwrap();

        // Create glob pattern
        let pattern = dir_path.join("*.txt").to_str().unwrap().to_string();

        let processor = FsstreamProcessor::new();
        let fop = Fop::new(&pattern);

        let results = processor.process_one(fop).await;

        assert_eq!(results.len(), 2, "Expected 2 .txt files, got {:?}", results);
        assert!(results[0].pattern.is_some());
        assert!(results[0].filename.is_some());
    }

    #[tokio::test]
    async fn test_async_literal_file() {
        let dir = tempdir().unwrap();
        let dir_path = dir.path();

        // Create a literal file
        let file_path = dir_path.join("literal.txt");
        fs::write(&file_path, "content").unwrap();

        let processor = FsstreamProcessor::new();
        let fop = Fop::new(file_path.to_str().unwrap());

        let results = processor.process_one(fop).await;

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].filename.as_ref().unwrap(), &file_path);
    }

    #[tokio::test]
    async fn test_async_literal_file_nonexistent() {
        let processor = FsstreamProcessor::new();
        let fop = Fop::new("/nonexistent/literal/file.txt");

        let results = processor.process_one(fop).await;

        // No wildcards, file doesn't exist -> empty results (matches TinyGlobby)
        assert_eq!(results.len(), 0);
    }

    #[tokio::test]
    async fn test_async_skips_existing_filename() {
        let processor = FsstreamProcessor::new();
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
    async fn test_async_invalid_pattern() {
        let processor = FsstreamProcessor::new();
        let fop = Fop::new("[invalid[.txt");

        let results = processor.process_one(fop).await;

        assert_eq!(results.len(), 1);
        assert!(results[0].err.is_some());
        assert_eq!(
            results[0].err.as_ref().unwrap().processor,
            "FsstreamProcessor"
        );
    }

    #[tokio::test]
    async fn test_async_no_matches() {
        let processor = FsstreamProcessor::new();
        let fop = Fop::new("/nonexistent/path/*.txt");

        let results = processor.process_one(fop).await;

        assert_eq!(results.len(), 1);
        assert!(results[0].err.is_some());
    }

    #[tokio::test]
    async fn test_concurrency_limiting() {
        // Test that semaphore properly limits concurrent operations
        let processor = FsstreamProcessor::new().with_concurrency(2);

        // Clone the semaphore to check permits
        let sem = processor.concurrency.clone();
        assert_eq!(sem.available_permits(), 2);

        // Acquire permits manually to verify behavior
        let permit1 = sem.try_acquire().unwrap();
        assert_eq!(sem.available_permits(), 1);

        let permit2 = sem.try_acquire().unwrap();
        assert_eq!(sem.available_permits(), 0);

        // Third acquire should fail
        assert!(sem.try_acquire().is_err());

        // Release and verify
        drop(permit1);
        assert_eq!(sem.available_permits(), 1);

        drop(permit2);
        assert_eq!(sem.available_permits(), 2);
    }

    #[test]
    fn test_default() {
        let processor = FsstreamProcessor::default();
        assert_eq!(processor.name(), "FsstreamProcessor");
        assert_eq!(processor.concurrency.available_permits(), DEFAULT_CONCURRENCY);
    }

    #[test]
    fn test_clone() {
        let processor = FsstreamProcessor::new().with_concurrency(16);
        let cloned = processor.clone();

        // Clones share the same semaphore
        assert!(Arc::ptr_eq(&processor.concurrency, &cloned.concurrency));
    }
}
