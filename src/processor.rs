//! Processor trait and related types.

use crate::fop::Fop;
use crate::stamper::Stamper;
use std::future::Future;
use std::sync::Arc;
use tokio::sync::Semaphore;

/// Trait for processors that transform Fop objects.
///
/// # Deprecated
///
/// This trait is deprecated. Use `AsyncProcessor` instead.
///
/// Processors accept an iterator of Fop objects and return an iterator
/// of transformed Fop objects. This enables streaming behavior through
/// the pipeline without blocking on completion.
#[deprecated(since = "0.1.0", note = "Use AsyncProcessor instead")]
pub trait Processor: Send + Sync {
    /// Process an iterator of Fop objects and return an iterator of transformed Fop objects.
    ///
    /// # Type Parameters
    ///
    /// * `'a` - Lifetime for the iterator
    /// * `I` - Input iterator type
    ///
    /// # Returns
    ///
    /// An iterator that yields processed Fop objects.
    fn process<'a, I>(&self, input: I) -> impl Iterator<Item = Fop> + 'a
    where
        I: Iterator<Item = Fop> + 'a;

    /// Get the name of this processor.
    ///
    /// This is used for error reporting and debugging.
    fn name(&self) -> &str;
}

/// Async trait for processors that transform Fop objects.
///
/// Processors accept a single Fop and return a Future that yields 0..N Fops.
/// This enables asynchronous, non-blocking processing with proper tokio integration.
pub trait AsyncProcessor: Send + Sync {
    /// Processor name for debugging and error attribution.
    fn name(&self) -> &'static str;

    /// Process a single Fop, potentially producing multiple outputs.
    ///
    /// # Return Semantics
    /// - Empty Vec: item filtered out (guard rejected, no glob matches)
    /// - Single item: 1:1 transformation
    /// - Multiple items: fan-out (glob expansion)
    fn process_one(&self, fop: Fop) -> impl Future<Output = Vec<Fop>> + Send;
}

/// Trait for processors that can process items concurrently.
///
/// This trait is implemented by processors that support bounded
/// execution modes using resource pools.
pub trait BoundedProcessor: Processor {
    /// Get the bound limit for this processor.
    ///
    /// Returns None if unbounded.
    fn bound_limit(&self) -> Option<usize>;
}

/// A bounded processor that limits concurrent execution using a semaphore.
///
/// Wraps an inner processor and limits to number of concurrent
/// executions to a configured pool size using a semaphore.
pub struct SemaphoreBoundedProcessor<P> {
    /// Inner processor being wrapped
    inner: P,
    /// Semaphore for limiting concurrent executions
    semaphore: Arc<Semaphore>,
    /// Optional stamper for measuring wait time
    wait_stamper: Option<Box<dyn Stamper>>,
    /// Field name for wait timestamp
    wait_name: String,
    /// Name of this processor
    name: String,
}

impl<P> SemaphoreBoundedProcessor<P>
where
    P: Processor + Clone + 'static,
{
    /// Create a new bounded processor wrapping the given inner processor.
    ///
    /// # Arguments
    ///
    /// * `inner` - The inner processor to wrap
    /// * `pool_size` - Maximum number of concurrent executions allowed
    pub fn new(inner: P, pool_size: usize) -> Self {
        let name = format!("Bounded({})", inner.name());
        Self {
            inner: inner.clone(),
            semaphore: Arc::new(Semaphore::new(pool_size)),
            wait_stamper: None,
            wait_name: "waitStamp".to_string(),
            name,
        }
    }

    /// Set a stamper for measuring wait time.
    pub fn with_wait_stamper(mut self, stamper: impl Stamper + 'static) -> Self {
        self.wait_stamper = Some(Box::new(stamper));
        self
    }

    /// Set the field name for wait timestamp.
    pub fn with_wait_name(mut self, name: impl Into<String>) -> Self {
        self.wait_name = name.into();
        self
    }
}

impl<P> Processor for SemaphoreBoundedProcessor<P>
where
    P: Processor + Clone + 'static,
{
    /// Process items with semaphore-based bounding.
    ///
    /// Note: Full implementation is TODO pending resolution of
    /// async/sync complexity with iterator lifetimes.
    fn process<'a, I>(&self, input: I) -> impl Iterator<Item = Fop> + 'a
    where
        I: Iterator<Item = Fop> + 'a,
    {
        input
    }

    fn name(&self) -> &str {
        &self.name
    }
}

impl<P> BoundedProcessor for SemaphoreBoundedProcessor<P>
where
    P: Processor + Clone + 'static,
{
    fn bound_limit(&self) -> Option<usize> {
        Some(self.semaphore.available_permits())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stamper::TrueStamper;

    #[derive(Clone)]
    struct TestProcessor;

    impl Processor for TestProcessor {
        fn process<'a, I>(&self, input: I) -> impl Iterator<Item = Fop> + 'a
        where
            I: Iterator<Item = Fop> + 'a,
        {
            input
        }

        fn name(&self) -> &str {
            "TestProcessor"
        }
    }

    #[test]
    fn test_processor_trait() {
        let processor = TestProcessor;
        assert_eq!(processor.name(), "TestProcessor");

        let inputs = vec![Fop::new("test1"), Fop::new("test2")];
        let results: Vec<_> = processor.process(inputs.into_iter()).collect();

        assert_eq!(results.len(), 2);
        assert_eq!(&*results[0].file_or_pattern, "test1");
        assert_eq!(&*results[1].file_or_pattern, "test2");
    }

    #[test]
    fn test_processor_empty_input() {
        let processor = TestProcessor;
        let inputs: Vec<Fop> = vec![];
        let results: Vec<_> = processor.process(inputs.into_iter()).collect();

        assert!(results.is_empty());
    }

    #[test]
    fn test_bounded_processor_trait() {
        struct TestBoundedProcessor {
            limit: Option<usize>,
        }

        impl Processor for TestBoundedProcessor {
            fn process<'a, I>(&self, input: I) -> impl Iterator<Item = Fop> + 'a
            where
                I: Iterator<Item = Fop> + 'a,
            {
                input
            }

            fn name(&self) -> &str {
                "TestBoundedProcessor"
            }
        }

        impl BoundedProcessor for TestBoundedProcessor {
            fn bound_limit(&self) -> Option<usize> {
                self.limit
            }
        }

        let bounded = TestBoundedProcessor { limit: Some(10) };
        assert_eq!(bounded.bound_limit(), Some(10));

        let unbounded = TestBoundedProcessor { limit: None };
        assert_eq!(unbounded.bound_limit(), None);
    }

    #[test]
    fn test_semaphore_bounded_processor_creation() {
        let inner = TestProcessor;
        let bounded = SemaphoreBoundedProcessor::new(inner, 3);

        assert_eq!(bounded.name(), "Bounded(TestProcessor)");
        assert_eq!(bounded.bound_limit(), Some(3));
    }

    #[test]
    fn test_semaphore_bounded_processor_with_wait_stamper() {
        let inner = TestProcessor;
        let bounded = SemaphoreBoundedProcessor::new(inner, 3).with_wait_stamper(TrueStamper);

        assert_eq!(bounded.name(), "Bounded(TestProcessor)");
    }

    #[test]
    fn test_semaphore_bounded_processor_with_wait_name() {
        let inner = TestProcessor;
        let bounded = SemaphoreBoundedProcessor::new(inner, 3).with_wait_name("customWait");

        assert_eq!(bounded.name(), "Bounded(TestProcessor)");
    }
}
