//! File Or Pattern - A library for handling file or pattern arguments in CLI programs.

pub mod fop;
pub mod processor;

pub use fop::{Content, Fop, Pattern, ProcessorError, TimestampInfo};
pub use processor::{BoundedProcessor, Processor};
