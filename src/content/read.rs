//! ReadContentProcessor implementation.

use crate::fop::{Content, Fop, ProcessorError};
use crate::processor::Processor;
use std::fs;
use std::io::Read;

/// Processor for reading file contents.
///
/// Reads from filename field and adds content to Fop.
pub struct ReadContentProcessor {
    encoding: Option<String>,
    record_encoding: bool,
}

impl ReadContentProcessor {
    /// Create a new ReadContentProcessor with UTF-8 encoding.
    pub fn new() -> Self {
        Self {
            encoding: Some("utf8".to_string()),
            record_encoding: false,
        }
    }

    /// Set the encoding to use for reading files.
    ///
    /// Reads as text (Content::Text) using the specified encoding.
    pub fn with_encoding(mut self, encoding: impl Into<String>) -> Self {
        self.encoding = Some(encoding.into());
        self
    }

    /// Read as raw bytes instead of text.
    ///
    /// Reads as bytes (Content::Bytes) without encoding.
    pub fn as_binary(mut self) -> Self {
        self.encoding = None;
        self
    }

    /// Set whether to record the encoding to the fop.encoding field.
    pub fn record_encoding(mut self, record: bool) -> Self {
        self.record_encoding = record;
        self
    }
}

impl Default for ReadContentProcessor {
    fn default() -> Self {
        Self::new()
    }
}

impl Processor for ReadContentProcessor {
    fn process<'a, I>(&self, input: I) -> impl Iterator<Item = Fop> + 'a
    where
        I: Iterator<Item = Fop> + 'a,
    {
        let encoding = self.encoding.clone();
        let record_encoding = self.record_encoding;
        let name = self.name().to_string();
        input.map(move |mut fop| {
            // Only process if filename is set
            if let Some(filename) = &fop.filename {
                match fs::File::open(filename) {
                    Ok(mut file) => {
                        let mut buffer = Vec::new();

                        if let Err(e) = file.read_to_end(&mut buffer) {
                            let err = ProcessorError::new(
                                name.as_str(),
                                format!("Failed to read file {}: {}", filename.display(), e),
                            );
                            fop.err = Some(err);
                            return fop;
                        }

                        // Set content based on encoding
                        if let Some(enc) = &encoding {
                            // Try to decode as text
                            match std::str::from_utf8(&buffer) {
                                Ok(text) => {
                                    fop.content = Some(Content::Text(text.to_string()));
                                    if record_encoding {
                                        fop.encoding = Some(enc.clone());
                                    }
                                }
                                Err(_) => {
                                    // Invalid UTF-8, store as bytes
                                    fop.content = Some(Content::Bytes(buffer));
                                    if record_encoding {
                                        fop.encoding = Some("binary".to_string());
                                    }
                                }
                            }
                        } else {
                            // No encoding specified, store as bytes
                            fop.content = Some(Content::Bytes(buffer));
                            if record_encoding {
                                fop.encoding = Some("binary".to_string());
                            }
                        }
                    }
                    Err(e) => {
                        let err = ProcessorError::new(
                            name.as_str(),
                            format!("Failed to open file {}: {}", filename.display(), e),
                        );
                        fop.err = Some(err);
                    }
                }
            }
            fop
        })
    }

    fn name(&self) -> &str {
        "ReadContentProcessor"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_read_content_processor() {
        let processor = ReadContentProcessor::new();
        assert_eq!(processor.name(), "ReadContentProcessor");
        assert_eq!(processor.encoding, Some("utf8".to_string()));
        assert!(!processor.record_encoding);
    }

    #[test]
    fn test_encoding_option() {
        let processor = ReadContentProcessor::new().with_encoding("latin1");
        assert_eq!(processor.encoding, Some("latin1".to_string()));
    }

    #[test]
    fn test_record_encoding_option() {
        let processor = ReadContentProcessor::new().record_encoding(true);
        assert!(processor.record_encoding);
    }

    #[test]
    fn test_read_text_content() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.txt");
        fs::write(&file_path, "Hello, world!").unwrap();

        let processor = ReadContentProcessor::new();
        let mut fop = Fop::new(file_path.to_str().unwrap());
        fop.filename = Some(file_path.clone());

        let results: Vec<_> = processor.process(vec![fop].into_iter()).collect();

        assert_eq!(results.len(), 1);
        assert!(results[0].content.is_some());
        if let Some(Content::Text(text)) = &results[0].content {
            assert_eq!(text, "Hello, world!");
        } else {
            panic!("Expected Text content");
        }
    }

    #[test]
    fn test_read_binary_content() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.bin");
        let binary_data: Vec<u8> = vec![0x00, 0xFF, 0x7F, 0x80, 0x01];
        fs::write(&file_path, &binary_data).unwrap();

        let processor = ReadContentProcessor::new().as_binary();
        let mut fop = Fop::new(file_path.to_str().unwrap());
        fop.filename = Some(file_path.clone());

        let results: Vec<_> = processor.process(vec![fop].into_iter()).collect();

        assert_eq!(results.len(), 1);
        assert!(results[0].content.is_some());
        if let Some(Content::Bytes(data)) = &results[0].content {
            assert_eq!(data, &binary_data);
        } else {
            panic!("Expected Bytes content");
        }
    }

    #[test]
    fn test_read_record_encoding() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.txt");
        fs::write(&file_path, "test").unwrap();

        let processor = ReadContentProcessor::new().record_encoding(true);
        let mut fop = Fop::new(file_path.to_str().unwrap());
        fop.filename = Some(file_path.clone());

        let results: Vec<_> = processor.process(vec![fop].into_iter()).collect();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].encoding.as_ref().unwrap(), "utf8");
    }

    #[test]
    fn test_read_no_filename() {
        let processor = ReadContentProcessor::new();
        let fop = Fop::new("test.txt");

        let results: Vec<_> = processor.process(vec![fop].into_iter()).collect();

        assert_eq!(results.len(), 1);
        assert!(results[0].content.is_none());
    }

    #[test]
    fn test_read_file_not_found() {
        let processor = ReadContentProcessor::new();
        let mut fop = Fop::new("nonexistent.txt");
        fop.filename = Some("/nonexistent/file.txt".into());

        let results: Vec<_> = processor.process(vec![fop].into_iter()).collect();

        assert_eq!(results.len(), 1);
        assert!(results[0].err.is_some());
        assert_eq!(results[0].err.as_ref().unwrap().processor, "ReadContentProcessor");
    }

    #[test]
    fn test_default() {
        let processor = ReadContentProcessor::default();
        assert_eq!(processor.name(), "ReadContentProcessor");
    }
}
