//! Stamper trait and implementations.
//!
//! Stampers generate supplemental execution information about pipeline processing.

use crate::fop::{Fop, TimestampInfo};

/// Options passed to stamper start method.
#[derive(Debug, Clone, Default)]
pub struct StamperOptions<'a> {
    /// Name of processor to use for stamping (if set, skips processor argument)
    pub processor: Option<&'a str>,
}

/// Promise-like handle for deferred completion of timestamping.
///
/// This provides a Promise.withResolvers-shaped interface with a promise
/// and methods to resolve or reject it.
#[derive(Debug)]
pub struct StamperHandle<T> {
    /// The promise/future for this stamper operation
    pub promise: tokio::sync::oneshot::Receiver<T>,
    /// Sender used to resolve promise
    resolve_tx: Option<tokio::sync::oneshot::Sender<T>>,
}

impl<T> StamperHandle<T> {
    /// Create a new StamperHandle with a promise and resolver.
    pub fn new() -> Self {
        let (resolve_tx, promise) = tokio::sync::oneshot::channel();
        Self {
            promise,
            resolve_tx: Some(resolve_tx),
        }
    }

    /// Resolve promise with a value.
    ///
    /// Returns Ok(()) if promise was successfully resolved,
    /// or Err(value) if receiver was already dropped.
    pub fn resolve(&mut self, value: T) -> Result<(), T> {
        if let Some(tx) = self.resolve_tx.take() {
            tx.send(value)
        } else {
            Err(value)
        }
    }

    /// Check if promise has been resolved or rejected.
    pub fn is_resolved(&self) -> bool {
        self.resolve_tx.is_none()
    }

    /// Try to receive the value without blocking.
    pub fn try_recv(&mut self) -> Result<T, tokio::sync::oneshot::error::TryRecvError> {
        self.promise.try_recv()
    }
}

impl<T> Default for StamperHandle<T> {
    fn default() -> Self {
        Self::new()
    }
}

/// Trait for generating supplemental execution information.
///
/// Stampers are called to measure operations and generate metadata
/// about pipeline processing. They support deferred completion through
/// StamperHandle promise-like interface.
pub trait Stamper: Send + Sync {
    /// Start a stamper operation.
    ///
    /// Returns a StamperHandle with a promise that can be resolved
    /// or rejected when the operation completes.
    ///
    /// # Arguments
    ///
    /// * `options` - Options including optional processor name override
    /// * `processor_name` - The name of the processor running
    /// * `fop` - The fop being processed
    ///
    /// # Returns
    ///
    /// A StamperHandle with a promise-like interface for deferred completion.
    fn start(
        &self,
        options: &StamperOptions,
        processor_name: &str,
        fop: &Fop,
    ) -> StamperHandle<Box<dyn std::any::Any + Send + Sync>>;
}

/// Strategy for generating start mark names.
pub trait StartNamer: Send + Sync + std::fmt::Debug {
    /// Generate a start mark name for the given fop.
    fn name(&self, fop: &Fop) -> String;
}

/// Default start namer using fop.file_or_pattern.
#[derive(Debug, Clone, Default)]
pub struct DefaultStartNamer;

impl StartNamer for DefaultStartNamer {
    fn name(&self, fop: &Fop) -> String {
        format!("fop-{}", fop.file_or_pattern)
    }
}

/// Strategy for generating end measure suffixes.
pub trait EndSuffixNamer: Send + Sync + std::fmt::Debug {
    /// Generate an end suffix for the given fop.
    fn suffix(&self, fop: &Fop) -> String;
}

/// Default end suffix namer using "end" suffix.
#[derive(Debug, Clone, Default)]
pub struct DefaultEndSuffixNamer;

impl EndSuffixNamer for DefaultEndSuffixNamer {
    fn suffix(&self, _fop: &Fop) -> String {
        "end".to_string()
    }
}

/// Literal string suffix namer.
#[derive(Debug, Clone)]
pub struct LiteralSuffixNamer {
    suffix: String,
}

impl LiteralSuffixNamer {
    pub fn new(suffix: impl Into<String>) -> Self {
        Self {
            suffix: suffix.into(),
        }
    }
}

impl EndSuffixNamer for LiteralSuffixNamer {
    fn suffix(&self, _fop: &Fop) -> String {
        self.suffix.clone()
    }
}

/// A no-op stamper for testing scenarios.
///
/// This stamper always creates handles that resolve immediately to true.
#[derive(Debug, Clone)]
pub struct TrueStamper;

impl Stamper for TrueStamper {
    fn start(
        &self,
        _options: &StamperOptions,
        _processor_name: &str,
        _fop: &Fop,
    ) -> StamperHandle<Box<dyn std::any::Any + Send + Sync>> {
        let mut handle = StamperHandle::new();
        let _ = handle.resolve(Box::new(true) as Box<dyn std::any::Any + Send + Sync>);
        handle
    }
}

/// Performance measure stamper using minstant for high-precision timing.
///
/// Records start time and calculates duration when resolved.
#[derive(Debug)]
pub struct PerformanceMeasureStamper {
    start_namer: Box<dyn StartNamer>,
    end_suffix_namer: Box<dyn EndSuffixNamer>,
}

impl PerformanceMeasureStamper {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_start_namer(mut self, namer: impl StartNamer + 'static) -> Self {
        self.start_namer = Box::new(namer);
        self
    }

    pub fn with_end_suffix_namer(mut self, namer: impl EndSuffixNamer + 'static) -> Self {
        self.end_suffix_namer = Box::new(namer);
        self
    }
}

impl Default for PerformanceMeasureStamper {
    fn default() -> Self {
        Self {
            start_namer: Box::new(DefaultStartNamer),
            end_suffix_namer: Box::new(DefaultEndSuffixNamer),
        }
    }
}

impl Stamper for PerformanceMeasureStamper {
    fn start(
        &self,
        _options: &StamperOptions,
        _processor_name: &str,
        fop: &Fop,
    ) -> StamperHandle<Box<dyn std::any::Any + Send + Sync>> {
        let start_time = minstant::Instant::now();
        let _start_name = self.start_namer.name(fop);
        let _end_suffix = self.end_suffix_namer.suffix(fop);

        let (resolve_tx, promise) = tokio::sync::oneshot::channel();

        let _ = tokio::task::spawn_blocking(move || {
            let end_time = minstant::Instant::now();
            let duration_ms = end_time.duration_since(start_time).as_millis() as u64;
            resolve_tx
                .send(Box::new(TimestampInfo::new(duration_ms))
                    as Box<dyn std::any::Any + Send + Sync>)
        });

        StamperHandle {
            promise,
            resolve_tx: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stamper_options_default() {
        let options = StamperOptions::default();
        assert!(options.processor.is_none());
    }

    #[test]
    fn test_stamper_options_with_processor() {
        let options = StamperOptions {
            processor: Some("TestProcessor"),
        };
        assert!(options.processor.is_some());
        assert_eq!(options.processor.unwrap(), "TestProcessor");
    }

    #[test]
    fn test_stamper_handle_creation() {
        let handle: StamperHandle<i32> = StamperHandle::new();
        assert!(!handle.is_resolved());
    }

    #[test]
    fn test_stamper_handle_default() {
        let handle: StamperHandle<String> = StamperHandle::default();
        assert!(!handle.is_resolved());
    }

    #[test]
    fn test_stamper_handle_resolve() {
        let mut handle = StamperHandle::new();
        let result = handle.resolve(42);
        assert!(result.is_ok());
        assert!(handle.is_resolved());
    }

    #[test]
    fn test_stamper_handle_resolve_twice() {
        let mut handle = StamperHandle::new();
        let result1 = handle.resolve(42);
        assert!(result1.is_ok());

        let result2 = handle.resolve(99);
        assert!(result2.is_err());
        assert_eq!(result2.unwrap_err(), 99);
    }

    #[test]
    fn test_stamper_handle_try_recv_empty() {
        let mut handle: StamperHandle<i32> = StamperHandle::new();
        let result = handle.try_recv();
        assert!(matches!(
            result,
            Err(tokio::sync::oneshot::error::TryRecvError::Empty)
        ));
    }

    #[test]
    fn test_stamper_handle_resolved() {
        let mut handle = StamperHandle::<i32>::new();
        let _ = handle.resolve(42);
        assert!(handle.is_resolved());
    }

    #[tokio::test]
    async fn test_stamper_handle_await() {
        let mut handle = StamperHandle::new();
        let _ = handle.resolve(42);

        let result = handle.promise.await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 42);
    }

    #[test]
    fn test_true_stamper() {
        let stamper = TrueStamper;
        let fop = Fop::new("test.txt");
        let options = StamperOptions::default();

        let mut handle = stamper.start(&options, "MockProcessor", &fop);
        assert!(handle.is_resolved());

        let result = handle.try_recv();
        assert!(result.is_ok());
        let value = result.unwrap();
        if let Some(bool_value) = value.downcast_ref::<bool>() {
            assert!(*bool_value);
        } else {
            panic!("Expected bool value");
        }
    }

    #[test]
    fn test_true_stamper_with_options() {
        let stamper = TrueStamper;
        let fop = Fop::new("test.txt");

        let options = StamperOptions {
            processor: Some("CustomProcessor"),
        };

        let handle = stamper.start(&options, "MockProcessor", &fop);
        assert!(handle.is_resolved());
    }

    #[test]
    fn test_stamper_options_lifetime() {
        let name = String::from("TestProcessor");
        let options = StamperOptions {
            processor: Some(&name),
        };
        assert_eq!(options.processor.unwrap(), "TestProcessor");
    }

    #[test]
    fn test_default_start_namer() {
        let namer = DefaultStartNamer;
        let fop = Fop::new("test.txt");
        assert_eq!(namer.name(&fop), "fop-test.txt");
    }

    #[test]
    fn test_default_end_suffix_namer() {
        let namer = DefaultEndSuffixNamer;
        let fop = Fop::new("test.txt");
        assert_eq!(namer.suffix(&fop), "end");
    }

    #[test]
    fn test_literal_suffix_namer() {
        let namer = LiteralSuffixNamer::new("custom-suffix");
        let fop = Fop::new("test.txt");
        assert_eq!(namer.suffix(&fop), "custom-suffix");
    }

    #[tokio::test]
    async fn test_performance_measure_stamper_default() {
        let stamper = PerformanceMeasureStamper::new();
        let fop = Fop::new("test.txt");
        let options = StamperOptions::default();

        let handle = stamper.start(&options, "TestProcessor", &fop);
        let _ = handle.promise.await;
    }

    #[tokio::test]
    async fn test_performance_measure_stamper_with_custom_namers() {
        #[derive(Debug)]
        struct CustomStartNamer;

        impl StartNamer for CustomStartNamer {
            fn name(&self, fop: &Fop) -> String {
                format!("custom-{}", fop.file_or_pattern)
            }
        }

        #[derive(Debug)]
        struct CustomEndNamer;

        impl EndSuffixNamer for CustomEndNamer {
            fn suffix(&self, _fop: &Fop) -> String {
                "custom-end".to_string()
            }
        }

        let stamper = PerformanceMeasureStamper::new()
            .with_start_namer(CustomStartNamer)
            .with_end_suffix_namer(CustomEndNamer);

        let fop = Fop::new("test.txt");
        let options = StamperOptions::default();

        let handle = stamper.start(&options, "TestProcessor", &fop);
        let _ = handle.promise.await;
    }

    #[tokio::test]
    async fn test_performance_measure_stamper_timing() {
        let stamper = PerformanceMeasureStamper::new();
        let fop = Fop::new("test.txt");
        let options = StamperOptions::default();

        let handle = stamper.start(&options, "TestProcessor", &fop);

        tokio::time::sleep(std::time::Duration::from_millis(10)).await;

        let result = handle.promise.await;
        assert!(result.is_ok());

        let value = result.unwrap();
        if let Some(timestamp) = value.downcast_ref::<TimestampInfo>() {
            assert!(timestamp.duration_ms >= 0);
        } else {
            panic!("Expected TimestampInfo");
        }
    }

    #[tokio::test]
    async fn test_performance_measure_stamper_literal_suffix() {
        let stamper = PerformanceMeasureStamper::new()
            .with_end_suffix_namer(LiteralSuffixNamer::new("execution"));

        let fop = Fop::new("test.txt");
        let options = StamperOptions::default();

        let handle = stamper.start(&options, "TestProcessor", &fop);
        let _ = handle.promise.await;
    }
}
