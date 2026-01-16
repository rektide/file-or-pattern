//! GuardProcessor implementation.

use crate::fop::Fop;
use crate::processor::Processor;

/// Processor that throws if FOP has error.
///
/// Stops propagation of FOPs with errors.
pub struct GuardProcessor;

impl GuardProcessor {
    pub fn new() -> Self {
        Self
    }
}

impl Default for GuardProcessor {
    fn default() -> Self {
        Self::new()
    }
}

impl Processor for GuardProcessor {
    fn process<'a, I>(&self, input: I) -> impl Iterator<Item = Fop> + 'a
    where
        I: Iterator<Item = Fop> + 'a,
    {
        input.filter(|fop| fop.err.is_none())
    }

    fn name(&self) -> &str {
        "GuardProcessor"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fop::Fop;
    use crate::fop::ProcessorError;

    #[test]
    fn test_guard_processor() {
        let processor = GuardProcessor::new();
        assert_eq!(processor.name(), "GuardProcessor");
    }

    #[test]
    fn test_guard_with_no_error() {
        let processor = GuardProcessor::new();
        let fop = Fop::new("test.txt");

        let results: Vec<_> = processor.process(vec![fop].into_iter()).collect();

        assert_eq!(results.len(), 1);
        assert!(results[0].err.is_none());
    }

    #[test]
    fn test_guard_with_error() {
        let processor = GuardProcessor::new();
        let mut fop = Fop::new("test.txt");
        fop.err = Some(ProcessorError::new("SomeProcessor", "test error"));

        let results: Vec<_> = processor.process(vec![fop].into_iter()).collect();

        assert_eq!(results.len(), 0);
    }

    #[test]
    fn test_multiple_with_one_error() {
        let processor = GuardProcessor::new();
        let mut fop1 = Fop::new("test1.txt");
        fop1.err = Some(ProcessorError::new("SomeProcessor", "error 1"));

        let fop2 = Fop::new("test2.txt");

        let results: Vec<_> = processor.process(vec![fop1, fop2].into_iter()).collect();

        // First FOP has error, should be filtered out
        // Second FOP passes through
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].file_or_pattern, "test2.txt");
    }

    #[test]
    fn test_default() {
        let processor = GuardProcessor::default();
        assert_eq!(processor.name(), "GuardProcessor");
    }
}
