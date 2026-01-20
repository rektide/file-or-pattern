//! CheckExistProcessor implementation.

use crate::fop::Fop;
use crate::processor::{AsyncProcessor, Processor};
use std::path::Path;

/// Processor for checking file existence.
///
/// Checks if the file_or_pattern exists in the filesystem and adds
/// the filename field if found.
pub struct CheckExistProcessor;

impl CheckExistProcessor {
    /// Create a new CheckExistProcessor.
    pub fn new() -> Self {
        Self
    }
}

impl Default for CheckExistProcessor {
    fn default() -> Self {
        Self::new()
    }
}

impl Processor for CheckExistProcessor {
    fn process<'a, I>(&self, input: I) -> impl Iterator<Item = Fop> + 'a
    where
        I: Iterator<Item = Fop> + 'a,
    {
        input.map(|mut fop| {
            if fop.filename.is_none() {
                let path = Path::new(&*fop.file_or_pattern);
                if path.exists() {
                    fop.filename = Some(path.to_path_buf());
                }
            }
            fop
        })
    }

    fn name(&self) -> &str {
        "CheckExistProcessor"
    }
}

impl AsyncProcessor for CheckExistProcessor {
    fn name(&self) -> &'static str {
        "CheckExistProcessor"
    }

    async fn process_one(&self, mut fop: Fop) -> Vec<Fop> {
        if fop.filename.is_none() {
            let path = Path::new(&*fop.file_or_pattern);
            if tokio::fs::metadata(&path).await.is_ok() {
                fop.filename = Some(path.to_path_buf());
            }
        }
        vec![fop]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;
    use tempfile::tempdir;

    #[test]
    fn test_check_exist_processor() {
        let processor = CheckExistProcessor::new();
        assert_eq!(Processor::name(&processor), "CheckExistProcessor");
    }

    #[test]
    fn test_check_exist_existing_file() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.txt");
        fs::write(&file_path, "content").unwrap();

        let processor = CheckExistProcessor::new();
        let fop = Fop::new(file_path.to_str().unwrap());

        let results: Vec<_> = processor.process(vec![fop].into_iter()).collect();

        assert_eq!(results.len(), 1);
        assert!(results[0].filename.is_some());
        assert_eq!(results[0].filename.as_ref().unwrap(), &file_path);
    }

    #[test]
    fn test_check_exist_nonexistent_file() {
        let processor = CheckExistProcessor::new();
        let fop = Fop::new("/nonexistent/file.txt");

        let results: Vec<_> = processor.process(vec![fop].into_iter()).collect();

        assert_eq!(results.len(), 1);
        assert!(results[0].filename.is_none());
    }

    #[test]
    fn test_check_exist_respects_existing_filename() {
        let processor = CheckExistProcessor::new();
        let mut fop = Fop::new("test.txt");
        fop.filename = Some(PathBuf::from("/some/other/path"));

        let results: Vec<_> = processor.process(vec![fop].into_iter()).collect();

        assert_eq!(results.len(), 1);
        assert_eq!(
            results[0].filename.as_ref().unwrap(),
            &PathBuf::from("/some/other/path")
        );
    }

    #[test]
    fn test_default() {
        let processor = CheckExistProcessor::default();
        assert_eq!(Processor::name(&processor), "CheckExistProcessor");
    }

    #[tokio::test]
    async fn test_async_check_exist_processor() {
        let processor = CheckExistProcessor::new();
        assert_eq!(AsyncProcessor::name(&processor), "CheckExistProcessor");
    }

    #[tokio::test]
    async fn test_async_check_exist_existing_file() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.txt");
        fs::write(&file_path, "content").unwrap();

        let processor = CheckExistProcessor::new();
        let fop = Fop::new(file_path.to_str().unwrap());

        let results = processor.process_one(fop).await;

        assert_eq!(results.len(), 1);
        assert!(results[0].filename.is_some());
        assert_eq!(results[0].filename.as_ref().unwrap(), &file_path);
    }

    #[tokio::test]
    async fn test_async_check_exist_nonexistent_file() {
        let processor = CheckExistProcessor::new();
        let fop = Fop::new("/nonexistent/file.txt");

        let results = processor.process_one(fop).await;

        assert_eq!(results.len(), 1);
        assert!(results[0].filename.is_none());
    }

    #[tokio::test]
    async fn test_async_check_exist_respects_existing_filename() {
        let processor = CheckExistProcessor::new();
        let mut fop = Fop::new("test.txt");
        fop.filename = Some(PathBuf::from("/some/other/path"));

        let results = processor.process_one(fop).await;

        assert_eq!(results.len(), 1);
        assert_eq!(
            results[0].filename.as_ref().unwrap(),
            &PathBuf::from("/some/other/path")
        );
    }
}
