//! File Or Pattern - A library for handling file or pattern arguments in CLI programs.

pub mod basic;
pub mod builder;
pub mod content;
pub mod fop;
pub mod pipelines;
pub mod processor;
pub mod stamper;

pub use basic::{CheckExistProcessor, ParserProcessor, TinyGlobbyProcessor};
pub use content::{DoExecuteProcessor, GuardProcessor, ReadContentProcessor};
pub use fop::{Content, Fop, Pattern, ProcessorError, TimestampInfo};
pub use processor::{BoundedProcessor, Processor, SemaphoreBoundedProcessor};
pub use stamper::{
    DefaultEndSuffixNamer, DefaultStartNamer, EndSuffixNamer, LiteralSuffixNamer,
    PerformanceMeasureStamper, Stamper, StamperHandle, StamperOptions, StartNamer, TrueStamper,
};
