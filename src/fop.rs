//! Core types for the File Or Pattern library.

use std::path::PathBuf;

/// A flyweight object passed through the pipeline, accumulating fields as it's processed.
#[derive(Debug, Clone)]
pub struct Fop {
    /// Original user input
    pub file_or_pattern: String,
    /// Concrete existing file path
    pub filename: Option<PathBuf>,
    /// Whether filename is executable
    pub executable: Option<bool>,
    /// Pattern matcher results
    pub match_results: Option<Vec<PathBuf>>,
    /// The matcher that detected the match
    pub pattern: Option<Pattern>,
    /// Resulting content (bytes or string)
    pub content: Option<Content>,
    /// File encoding read from
    pub encoding: Option<String>,
    /// Execution duration information
    pub timestamp: Option<TimestampInfo>,
    /// Error with processor field
    pub err: Option<ProcessorError>,
}

impl Fop {
    /// Create a new Fop with the given file_or_pattern input.
    pub fn new(file_or_pattern: impl Into<String>) -> Self {
        Self {
            file_or_pattern: file_or_pattern.into(),
            filename: None,
            executable: None,
            match_results: None,
            pattern: None,
            content: None,
            encoding: None,
            timestamp: None,
            err: None,
        }
    }
}

/// Content of a Fop, either as raw bytes or text.
#[derive(Debug, Clone)]
pub enum Content {
    /// Raw bytes content
    Bytes(Vec<u8>),
    /// Text content
    Text(String),
}

/// Pattern matcher type that stores the glob pattern.
#[derive(Debug, Clone)]
pub struct Pattern {
    /// The glob pattern string
    pub pattern: String,
}

impl Pattern {
    /// Create a new Pattern from a glob pattern string.
    pub fn new(pattern: impl Into<String>) -> Self {
        Self {
            pattern: pattern.into(),
        }
    }
}

/// Execution duration information.
#[derive(Debug, Clone)]
pub struct TimestampInfo {
    /// Duration in milliseconds
    pub duration_ms: u64,
}

impl TimestampInfo {
    /// Create new TimestampInfo from duration in milliseconds.
    pub fn new(duration_ms: u64) -> Self {
        Self { duration_ms }
    }
}

/// Processor error with processor field.
#[derive(Debug, Clone)]
pub struct ProcessorError {
    /// Name of the processor that generated the error
    pub processor: String,
    /// Underlying error
    pub source: String,
}

impl std::fmt::Display for ProcessorError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "processor error in '{}': {}",
            self.processor, self.source
        )
    }
}

impl From<ProcessorError> for String {
    fn from(err: ProcessorError) -> Self {
        err.source
    }
}

impl std::error::Error for ProcessorError {}

impl ProcessorError {
    /// Create a new ProcessorError.
    pub fn new(processor: impl Into<String>, source: impl Into<String>) -> Self {
        Self {
            processor: processor.into(),
            source: source.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fop_creation() {
        let fop = Fop::new("test.txt");
        assert_eq!(fop.file_or_pattern, "test.txt");
        assert!(fop.filename.is_none());
        assert!(fop.content.is_none());
    }

    #[test]
    fn test_content_bytes() {
        let bytes = vec![1, 2, 3, 4];
        let content = Content::Bytes(bytes.clone());
        if let Content::Bytes(b) = content {
            assert_eq!(b, bytes);
        } else {
            panic!("Expected Bytes variant");
        }
    }

    #[test]
    fn test_content_text() {
        let text = "Hello, world!".to_string();
        let content = Content::Text(text.clone());
        if let Content::Text(t) = content {
            assert_eq!(t, text);
        } else {
            panic!("Expected Text variant");
        }
    }

    #[test]
    fn test_timestamp_info() {
        let info = TimestampInfo::new(100);
        assert_eq!(info.duration_ms, 100);
    }

    #[test]
    fn test_processor_error() {
        let err = ProcessorError::new("TestProcessor", "Something went wrong");
        assert_eq!(err.processor, "TestProcessor");
        assert_eq!(err.source, "Something went wrong");
    }
}
