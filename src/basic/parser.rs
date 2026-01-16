//! ParserProcessor implementation.

use crate::fop::{Fop, ProcessorError};
use crate::processor::Processor;

/// Processor for converting user strings into Fop objects.
///
/// This is usually the first step in the pipeline, creating the flyweight object.
pub struct ParserProcessor {
    guard: bool,
}

impl ParserProcessor {
    /// Create a new ParserProcessor with guard disabled.
    pub fn new() -> Self {
        Self { guard: false }
    }

    /// Set whether to validate existing objects have file_or_pattern field.
    ///
    /// If guard is true and an object is passed in that doesn't have
    /// file_or_pattern field (which shouldn't happen as this is the first processor),
    /// it will throw an error.
    pub fn guard(mut self, value: bool) -> Self {
        self.guard = value;
        self
    }
}

impl Default for ParserProcessor {
    fn default() -> Self {
        Self::new()
    }
}

impl Processor for ParserProcessor {
    fn process<'a, I>(&self, input: I) -> impl Iterator<Item = Fop> + 'a
    where
        I: Iterator<Item = Fop> + 'a,
    {
        let guard = self.guard;
        let name = self.name().to_string();
        input.map(move |fop| {
            if guard && fop.file_or_pattern.is_empty() {
                let err = ProcessorError::new(
                    name.as_str(),
                    "Invalid Fop: file_or_pattern field is empty",
                );
                let mut result = fop;
                result.err = Some(err);
                result
            } else {
                fop
            }
        })
    }

    fn name(&self) -> &str {
        "ParserProcessor"
    }
}

/// Helper function to convert strings into Fop objects for ParserProcessor.
///
/// This is a convenience function for the common case where you have
/// user strings and want to create Fop objects from them.
pub fn parse_strings(strings: impl IntoIterator<Item = impl Into<String>>) -> Vec<Fop> {
    strings
        .into_iter()
        .map(|s| Fop::new(s.into()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parser_processor() {
        let processor = ParserProcessor::new();
        assert_eq!(processor.name(), "ParserProcessor");
        assert!(!processor.guard);
    }

    #[test]
    fn test_parser_with_guard() {
        let processor = ParserProcessor::new().guard(true);
        assert!(processor.guard);
    }

    #[test]
    fn test_parser_process() {
        let processor = ParserProcessor::new();
        let fops = vec![Fop::new("test.txt"), Fop::new("file.rs")];

        let results: Vec<_> = processor.process(fops.into_iter()).collect();

        assert_eq!(results.len(), 2);
        assert_eq!(results[0].file_or_pattern, "test.txt");
        assert_eq!(results[1].file_or_pattern, "file.rs");
    }

    #[test]
    fn test_parse_strings() {
        let strings = vec!["test.txt", "file.rs", "data.json"];
        let fops = parse_strings(strings);

        assert_eq!(fops.len(), 3);
        assert_eq!(fops[0].file_or_pattern, "test.txt");
        assert_eq!(fops[1].file_or_pattern, "file.rs");
        assert_eq!(fops[2].file_or_pattern, "data.json");
    }

    #[test]
    fn test_default() {
        let processor = ParserProcessor::default();
        assert_eq!(processor.name(), "ParserProcessor");
        assert!(!processor.guard);
    }
}
