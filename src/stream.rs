//! Stream combinators for async pipeline processing.

use crate::fop::Fop;
use crate::processor::AsyncProcessor;
use futures::stream::{BoxStream, StreamExt};
use std::sync::Arc;
use tokio::sync::Semaphore;

/// Type alias for a boxed stream of Fops.
pub type FopStream<'a> = BoxStream<'a, Fop>;

/// Type alias for static lifetime FopStream.
pub type FopStreamStatic = BoxStream<'static, Fop>;

/// Apply a processor to transform a stream.
///
/// Each Fop in the input stream is processed asynchronously through
/// the processor's `process_one` method. The result (Vec<Fop>) is
/// flattened back into the output stream.
///
/// # Example
///
/// ```rust,no_run
/// use file_or_pattern::fop::Fop;
/// use file_or_pattern::stream::{apply_processor, FopStreamStatic};
/// use file_or_pattern::processor::AsyncProcessor;
/// use file_or_pattern::content::ReadContentProcessor;
/// use futures::stream;
/// use futures::StreamExt;
/// use std::sync::Arc;
///
/// # async fn example() {
/// let processor = Arc::new(ReadContentProcessor::new());
/// let input: FopStreamStatic = stream::iter(vec![Fop::new("test")]).boxed();
/// let output = apply_processor(input, processor);
/// let results: Vec<Fop> = output.collect().await;
/// # }
/// ```
pub fn apply_processor<P>(
    input: FopStream<'static>,
    processor: Arc<P>,
) -> FopStreamStatic
where
    P: AsyncProcessor + 'static,
{
    input
        .then(move |fop| {
            let proc = processor.clone();
            async move { proc.process_one(fop).await }
        })
        .flat_map(futures::stream::iter)
        .boxed()
}

/// Apply a processor with bounded concurrency using a semaphore.
///
/// Limits concurrent processing to `max_concurrency` operations at a time.
/// Each processing operation acquires a permit from the semaphore before
/// executing and releases it automatically on completion.
///
/// # Example
///
/// ```rust,no_run
/// use file_or_pattern::fop::Fop;
/// use file_or_pattern::stream::{apply_bounded, FopStreamStatic};
/// use file_or_pattern::processor::AsyncProcessor;
/// use file_or_pattern::content::ReadContentProcessor;
/// use futures::stream;
/// use futures::StreamExt;
/// use std::sync::Arc;
///
/// # async fn example() {
/// let processor = Arc::new(ReadContentProcessor::new());
/// let input: FopStreamStatic = stream::iter(vec![Fop::new("test")]).boxed();
/// let output = apply_bounded(input, processor, 10);
/// let results: Vec<Fop> = output.collect().await;
/// # }
/// ```
pub fn apply_bounded<P>(
    input: FopStream<'static>,
    processor: Arc<P>,
    max_concurrency: usize,
) -> FopStreamStatic
where
    P: AsyncProcessor + 'static,
{
    let semaphore = Arc::new(Semaphore::new(max_concurrency));

    input
        .map(move |fop| {
            let proc = processor.clone();
            let sem = semaphore.clone();
            async move {
                let _permit = sem
                    .acquire()
                    .await
                    .expect("semaphore should not be closed");
                proc.process_one(fop).await
            }
        })
        .buffer_unordered(usize::MAX)
        .flat_map(futures::stream::iter)
        .boxed()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::processor::AsyncProcessor;
    use crate::fop::Fop;
    use futures::StreamExt;

    #[derive(Clone)]
    struct TestAsyncProcessor;

    impl AsyncProcessor for TestAsyncProcessor {
        fn name(&self) -> &'static str {
            "TestAsyncProcessor"
        }

        async fn process_one(&self, fop: Fop) -> Vec<Fop> {
            vec![fop]
        }
    }

    #[tokio::test]
    async fn test_apply_processor() {
        let processor = Arc::new(TestAsyncProcessor);
        let inputs = vec![Fop::new("test1"), Fop::new("test2")];
        let stream: FopStream<'static> = futures::stream::iter(inputs).boxed();
        let output = apply_processor(stream, processor);
        let results: Vec<Fop> = output.collect().await;

        assert_eq!(results.len(), 2);
        assert_eq!(&*results[0].file_or_pattern, "test1");
        assert_eq!(&*results[1].file_or_pattern, "test2");
    }

    #[tokio::test]
    async fn test_apply_processor_empty_input() {
        let processor = Arc::new(TestAsyncProcessor);
        let inputs: Vec<Fop> = vec![];
        let stream: FopStream<'static> = futures::stream::iter(inputs).boxed();
        let output = apply_processor(stream, processor);
        let results: Vec<Fop> = output.collect().await;

        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn test_apply_bounded() {
        let processor = Arc::new(TestAsyncProcessor);
        let inputs: Vec<_> = (0..5).map(|i| Fop::new(format!("test{}", i))).collect();
        let stream: FopStream<'static> = futures::stream::iter(inputs).boxed();
        let output = apply_bounded(stream, processor, 2);
        let results: Vec<Fop> = output.collect().await;

        assert_eq!(results.len(), 5);
    }
}
