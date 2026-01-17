//! DoExecuteProcessor implementation.

use crate::fop::{Content, Fop, ProcessorError};
use crate::processor::Processor;
use std::path::Path;
use std::process::Command;

pub struct DoExecuteProcessor {
    expect_execution: bool,
}

impl DoExecuteProcessor {
    pub fn new() -> Self {
        Self {
            expect_execution: false,
        }
    }

    pub fn expect_execution(mut self, value: bool) -> Self {
        self.expect_execution = value;
        self
    }

    #[cfg(unix)]
    fn is_executable(path: &Path) -> bool {
        use std::os::unix::fs::PermissionsExt;
        path.metadata()
            .ok()
            .map(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
            .unwrap_or(false)
    }

    #[cfg(windows)]
    fn is_executable(path: &Path) -> bool {
        path.extension()
            .and_then(|e| e.to_str())
            .map(|e| matches!(e, "exe" | "bat" | "cmd" | "ps1") && path.is_file())
            .unwrap_or(false)
    }

    #[cfg(not(any(unix, windows)))]
    fn is_executable(path: &Path) -> bool {
        path.is_file()
    }
}

impl Default for DoExecuteProcessor {
    fn default() -> Self {
        Self::new()
    }
}

impl Processor for DoExecuteProcessor {
    fn process<'a, I>(&self, input: I) -> impl Iterator<Item = Fop> + 'a
    where
        I: Iterator<Item = Fop> + 'a,
    {
        let expect_execution = self.expect_execution;
        let name = "DoExecuteProcessor".to_string();

        input.filter_map(move |mut fop| {
            let file_or_pattern = fop.file_or_pattern.clone();
            let filename_opt = fop.filename.take();
            let path = filename_opt
                .as_ref()
                .map(Path::new)
                .unwrap_or_else(|| Path::new(&*file_or_pattern));

            if !Self::is_executable(&path) {
                if expect_execution {
                    let err = ProcessorError::new(
                        name.as_str(),
                        format!("File is not executable: {}", path.display()),
                    );
                    fop.err = Some(err);
                }
                return Some(fop);
            } else {
                let output = Command::new(&path).output().map_err(|e| {
                    ProcessorError::new(
                        name.as_str(),
                        format!("Failed to execute {}: {}", path.display(), e),
                    )
                });

                match output {
                    Ok(o) if !o.status.success() => {
                        let stderr = String::from_utf8_lossy(&o.stderr);
                        let err = ProcessorError::new(
                            name.as_str(),
                            format!("Command exited with status {}: {}", o.status, stderr),
                        );
                        fop.err = Some(err);
                        fop.executable = Some(true);
                        Some(fop)
                    }
                    Ok(o) => {
                        let stdout = String::from_utf8_lossy(&o.stdout).to_string();
                        fop.content = Some(Content::Text(stdout));
                        fop.executable = Some(true);
                        Some(fop)
                    }
                    Err(e) => {
                        let err = ProcessorError::new(name.as_str(), e);
                        fop.err = Some(err);
                        fop.executable = Some(true);
                        Some(fop)
                    }
                }
            }
        })
    }

    fn name(&self) -> &str {
        "DoExecuteProcessor"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_do_execute_processor() {
        let p = DoExecuteProcessor::new();
        assert_eq!(p.name(), "DoExecuteProcessor");
        assert!(!p.expect_execution);
    }

    #[test]
    fn test_expect_execution() {
        let p = DoExecuteProcessor::new().expect_execution(true);
        assert!(p.expect_execution);
    }

    #[test]
    fn test_non_executable_file() {
        let p = DoExecuteProcessor::new();
        let mut fop = Fop::new("notexec.txt");
        fop.filename = Some("/some/file.txt".into());

        let results: Vec<_> = p.process(vec![fop].into_iter()).collect();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].executable, None);
        assert!(results[0].content.is_none());
    }

    #[test]
    fn test_expect_execution_non_executable() {
        let p = DoExecuteProcessor::new().expect_execution(true);
        let mut fop = Fop::new("notexec.txt");
        fop.filename = Some("/some/file.txt".into());

        let results: Vec<_> = p.process(vec![fop].into_iter()).collect();
        assert_eq!(results.len(), 1);
        assert!(results[0].err.is_some());
        assert_eq!(
            results[0].err.as_ref().unwrap().processor,
            "DoExecuteProcessor"
        );
    }

    #[test]
    fn test_default() {
        let p = DoExecuteProcessor::default();
        assert_eq!(p.name(), "DoExecuteProcessor");
    }
}
