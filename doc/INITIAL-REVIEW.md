# FILE-OR-PATTERN Rust Implementation: Initial Design Review

This document provides a comprehensive critique of the current Rust implementation, analyzing design consistency, ownership patterns, and proposing a path toward a modern async Rust design.

## Executive Summary

The library is currently **architecturally split-brain**: it presents a "streaming pipeline" API with synchronous `Iterator`-based processors, while simultaneously pulling in tokio/semaphores/oneshots that only make sense in an async context. The `SemaphoreBoundedProcessor` exists but is essentially a TODO—it can't actually do bounded async execution because the `Processor` trait is synchronous.

**Recommendation**: Embrace async end-to-end with `futures::Stream`, per-item async processors, and proper tokio integration. This matches the original JavaScript async-iterable mental model and unlocks the real requirements (I/O, subprocess execution, semaphore-based bounding).

---

## 1. Design Consistency Analysis

### 1.1 Pipeline Contract Ambiguity

The current processors have inconsistent cardinality semantics:

| Processor | Cardinality | Notes |
|-----------|-------------|-------|
| `CheckExistProcessor` | 1→1 | Always returns exactly one Fop |
| `ParserProcessor` | 1→1 | Passthrough with optional guard |
| `ReadContentProcessor` | 1→1 | Reads file, sets content |
| `GuardProcessor` | 1→0/1 | Filters out errored Fops |
| `TinyGlobbyProcessor` | 1→0..N | Fan-out on matches, **zero on no matches** |
| `DoExecuteProcessor` | 1→0/1 | Uses `filter_map`, inconsistent with others |

This inconsistency makes it hard to reason about pipeline behavior:
- Is "no glob matches" an error or legitimate empty output?
- Should downstream processors expect guaranteed input?
- Is `file_or_pattern` immutable identity or mutable scratch?

**Current violation**: `SemaphoreBoundedProcessor` mutates `file_or_pattern`:
```rust
// processor.rs:108-110
fop.file_or_pattern = format!("bounded_{}", fop.file_or_pattern);
```
This breaks the flyweight contract where `file_or_pattern` should be the original user input.

### 1.2 Naming Inconsistencies

| README/JS | Rust Code | Issue |
|-----------|-----------|-------|
| `fileOrPattern` | `file_or_pattern` | Fine (Rust convention) |
| `match` | `match_results` | `match` is a keyword, but docs should reflect actual field |
| `timestamp` | `timestamp: Option<TimestampInfo>` | Field only stores duration, not full perf mark semantics |

### 1.3 Error Handling is "JS-Flavored"

The current design embeds errors inside the Fop rather than using Rust's `Result`:

```rust
// fop.rs:25
pub err: Option<ProcessorError>,
```

This creates ambiguity:
- Some processors set `err` and continue (`ReadContentProcessor`)
- Some processors filter out errored Fops (`GuardProcessor`)  
- Some drop items entirely without setting `err` (`TinyGlobbyProcessor` on no matches)

**`ProcessorError` is underspecified**:
```rust
// fop.rs:86-91
pub struct ProcessorError {
    pub processor: String,
    pub source: String,  // Loses the original error type and backtrace
}
```

The `impl From<ProcessorError> for String` is odd—it throws away the `processor` field:
```rust
// fop.rs:103-106
impl From<ProcessorError> for String {
    fn from(err: ProcessorError) -> Self {
        err.source  // processor field discarded!
    }
}
```

### 1.4 Performance/Allocation Issues

**Unnecessary allocation roundtrips in TinyGlobbyProcessor**:
```rust
// basic/glob.rs:74, 82
results.into_iter().collect::<Vec<_>>().into_iter()
```
The `.collect::<Vec<_>>().into_iter()` is a no-op that allocates and immediately consumes.

**O(N²) cloning in glob expansion**:
```rust
// basic/glob.rs:67-71
if !matched.is_empty() {
    for fop in &mut results {
        fop.match_results = Some(matched.clone());  // clones entire Vec for each result!
        fop.pattern = Some(Pattern::new(pattern.clone()));
    }
}
```
For a glob matching 1000 files, this clones the match list 1000 times.

---

## 2. The Sync/Async Tension

### 2.1 Current State: Tokio Without Async

The crate has `tokio` as a dependency with `features = ["full"]`, but:

1. **Processor trait is synchronous**:
   ```rust
   // processor.rs:24-26
   fn process<'a, I>(&self, input: I) -> impl Iterator<Item = Fop> + 'a
   where
       I: Iterator<Item = Fop> + 'a;
   ```

2. **SemaphoreBoundedProcessor exists but is non-functional**:
   ```rust
   // processor.rs:49-53
   pub struct SemaphoreBoundedProcessor<P> {
       inner: P,
       semaphore: Arc<Semaphore>,  // tokio::sync::Semaphore
       // ...
   }
   ```
   But the implementation just mutates strings—there's no actual semaphore acquisition because you can't `.await` in a sync iterator.

3. **Stamper uses tokio::sync::oneshot but awkwardly**:
   ```rust
   // stamper.rs:21-24
   pub struct StamperHandle<T> {
       pub promise: tokio::sync::oneshot::Receiver<T>,
       resolve_tx: Option<tokio::sync::oneshot::Sender<T>>,
   }
   ```
   This Promise.withResolvers pattern from JS doesn't fit Rust idioms.

4. **PerformanceMeasureStamper spawns blocking tasks for no reason**:
   ```rust
   // stamper.rs:210-217
   let _ = tokio::task::spawn_blocking(move || {
       let end_time = minstant::Instant::now();
       let duration_ms = end_time.duration_since(start_time).as_millis() as u64;
       // ...
   });
   ```
   Computing elapsed time is not blocking I/O—this is unnecessary complexity.

### 2.2 Why This Matters

Your requirements demand async:
- **File I/O**: `std::fs::read` blocks the thread; `tokio::fs::read` is non-blocking
- **Subprocess execution**: `std::process::Command` blocks; `tokio::process::Command` is async
- **Bounded concurrency**: `tokio::sync::Semaphore` requires `.await` for acquisition
- **Streaming**: True streaming over async I/O requires `Stream`, not `Iterator`

Inside a tokio runtime, blocking I/O is actively harmful—it blocks the runtime's thread pool. The current `ReadContentProcessor` and `DoExecuteProcessor` both use blocking I/O.

---

## 3. Ownership Model Analysis

### 3.1 Current Ownership Pattern

Fop is passed by value through processors:
```rust
// processor.rs:24-26
fn process<'a, I>(&self, input: I) -> impl Iterator<Item = Fop> + 'a
where
    I: Iterator<Item = Fop> + 'a;
```

This is **idiomatic for dataflow transforms**—the processor takes ownership, transforms, and yields ownership to the next stage. No shared references, no lifetime complexity.

### 3.2 Where Cloning Becomes Expensive

**Fan-out processors** must clone to produce multiple outputs:

```rust
// basic/glob.rs:50-52
let mut new_fop = fop.clone();  // Full clone for each match
new_fop.filename = Some(path);
results.push(new_fop);
```

What gets cloned per Fop:
- `file_or_pattern: String` — heap allocation
- `match_results: Option<Vec<PathBuf>>` — nested heap allocations
- `pattern: Option<Pattern>` — contains another String
- `content: Option<Content>` — potentially large file contents

For a glob matching 1000 files with 10KB average content, you could be cloning 10MB of data.

### 3.3 The "Pipeline Owns Inputs" Model

Your intuition is correct: inputs to the pipeline are typically small and short-lived, so cloning is acceptable. The pipeline should own its data because:

1. No lifetime complexity—inputs don't need to outlive the pipeline
2. Processors can freely mutate without borrowing concerns
3. Thread-safety is trivial (owned data is `Send`)

### 3.4 Recommended Improvements

**Make identity data cheap to clone with Arc**:

```rust
pub struct Fop {
    /// Original user input - immutable identity, cheap to clone
    pub file_or_pattern: Arc<str>,
    
    /// Concrete existing file path
    pub filename: Option<PathBuf>,
    
    /// Pattern matcher results - shared across fan-out
    pub match_results: Option<Arc<[PathBuf]>>,
    
    /// The matcher pattern - shared across fan-out
    pub pattern: Option<Arc<Pattern>>,
    
    /// Content - NOT cloned in fan-out (only set later)
    pub content: Option<Content>,
    
    // ... rest unchanged
}
```

This makes the common case (fan-out cloning) cheap while keeping the API simple.

**Consider whether match_results belongs on every Fop**:

Currently, every expanded Fop carries the full match list. Alternative:
- Store only `match_count: Option<usize>` on individual Fops
- Provide the full match list via a separate accessor if needed

---

## 4. Recommended Async Architecture

### 4.1 Core Design: Per-Item Async Processor

Replace the iterator-based trait with an async per-item processor:

```rust
use std::future::Future;

/// A processor that transforms one Fop into 0..N Fops asynchronously.
/// 
/// - Returns `Vec<Fop>` to support fan-out (glob) and filtering (guard)
/// - Async to support file I/O, subprocess execution, semaphore acquisition
pub trait AsyncProcessor: Send + Sync {
    /// Processor name for debugging and error attribution
    fn name(&self) -> &'static str;

    /// Process a single Fop, potentially producing multiple outputs.
    /// 
    /// # Return semantics
    /// - Empty Vec: item filtered out (guard rejected, no glob matches)
    /// - Single item: 1:1 transformation
    /// - Multiple items: fan-out (glob expansion)
    fn process_one(&self, fop: Fop) -> impl Future<Output = Vec<Fop>> + Send;
}
```

This maps cleanly to JS async generators: each `yield` becomes an item in the returned Vec.

### 4.2 Stream-Based Pipeline

Use `futures::Stream` for the pipeline:

```rust
use futures::stream::{BoxStream, StreamExt};

/// Type alias for a boxed stream of Fops
pub type FopStream<'a> = BoxStream<'a, Fop>;

/// Apply a processor to transform a stream
pub fn apply_processor<P>(
    input: FopStream<'static>,
    processor: Arc<P>,
) -> FopStream<'static>
where
    P: AsyncProcessor + 'static,
{
    input
        .then(move |fop| {
            let proc = processor.clone();
            async move { proc.process_one(fop).await }
        })
        .flat_map(futures::stream::iter)  // Vec<Fop> -> stream items
        .boxed()
}
```

### 4.3 Bounded Execution That Actually Works

The semaphore-based bounding becomes straightforward:

```rust
use tokio::sync::Semaphore;

pub fn apply_bounded<P>(
    input: FopStream<'static>,
    processor: Arc<P>,
    max_concurrency: usize,
) -> FopStream<'static>
where
    P: AsyncProcessor + 'static,
{
    let semaphore = Arc::new(Semaphore::new(max_concurrency));
    
    input
        .map(move |fop| {
            let proc = processor.clone();
            let sem = semaphore.clone();
            async move {
                // Acquire permit before processing
                let _permit = sem.acquire().await.expect("semaphore closed");
                proc.process_one(fop).await
                // Permit released on drop
            }
        })
        .buffer_unordered(usize::MAX)  // Let semaphore control concurrency
        .flat_map(futures::stream::iter)
        .boxed()
}
```

Alternative: use `.buffered(n)` for ordered output or `.buffer_unordered(n)` directly without semaphore for simpler cases.

### 4.4 Migrated Processor Examples

**CheckExistProcessor (async metadata check)**:
```rust
pub struct CheckExistProcessor;

impl AsyncProcessor for CheckExistProcessor {
    fn name(&self) -> &'static str { "CheckExistProcessor" }

    async fn process_one(&self, mut fop: Fop) -> Vec<Fop> {
        if fop.filename.is_none() {
            let path = std::path::Path::new(&*fop.file_or_pattern);
            if let Ok(meta) = tokio::fs::metadata(path).await {
                if meta.is_file() {
                    fop.filename = Some(path.to_path_buf());
                }
            }
        }
        vec![fop]
    }
}
```

**ReadContentProcessor (non-blocking file read)**:
```rust
pub struct ReadContentProcessor {
    pub as_text: bool,
    pub record_encoding: bool,
}

impl AsyncProcessor for ReadContentProcessor {
    fn name(&self) -> &'static str { "ReadContentProcessor" }

    async fn process_one(&self, mut fop: Fop) -> Vec<Fop> {
        let Some(ref path) = fop.filename else { return vec![fop] };
        
        match tokio::fs::read(path).await {
            Ok(bytes) => {
                if self.as_text {
                    match String::from_utf8(bytes) {
                        Ok(text) => {
                            fop.content = Some(Content::Text(text));
                            if self.record_encoding {
                                fop.encoding = Some("utf8".into());
                            }
                        }
                        Err(e) => {
                            fop.content = Some(Content::Bytes(e.into_bytes()));
                            if self.record_encoding {
                                fop.encoding = Some("binary".into());
                            }
                        }
                    }
                } else {
                    fop.content = Some(Content::Bytes(bytes));
                }
            }
            Err(e) => {
                fop.err = Some(ProcessorError::new(
                    self.name(),
                    format!("read {}: {}", path.display(), e),
                ));
            }
        }
        vec![fop]
    }
}
```

**DoExecuteProcessor (async subprocess)**:
```rust
pub struct DoExecuteProcessor {
    pub expect_execution: bool,
}

impl AsyncProcessor for DoExecuteProcessor {
    fn name(&self) -> &'static str { "DoExecuteProcessor" }

    async fn process_one(&self, mut fop: Fop) -> Vec<Fop> {
        let path = fop.filename.clone()
            .unwrap_or_else(|| fop.file_or_pattern.to_string().into());
        
        // Executable check (could be made async too)
        if !Self::is_executable(&path) {
            if self.expect_execution {
                fop.err = Some(ProcessorError::new(
                    self.name(),
                    format!("not executable: {}", path.display()),
                ));
            }
            return vec![fop];
        }
        
        match tokio::process::Command::new(&path).output().await {
            Ok(output) if output.status.success() => {
                fop.content = Some(Content::Text(
                    String::from_utf8_lossy(&output.stdout).into_owned()
                ));
                fop.executable = Some(true);
            }
            Ok(output) => {
                fop.err = Some(ProcessorError::new(
                    self.name(),
                    format!("exit {}: {}", output.status, 
                            String::from_utf8_lossy(&output.stderr)),
                ));
                fop.executable = Some(true);
            }
            Err(e) => {
                fop.err = Some(ProcessorError::new(
                    self.name(),
                    format!("exec {}: {}", path.display(), e),
                ));
            }
        }
        vec![fop]
    }
}
```

**TinyGlobbyProcessor (blocking glob wrapped in spawn_blocking)**:
```rust
pub struct TinyGlobbyProcessor;

impl AsyncProcessor for TinyGlobbyProcessor {
    fn name(&self) -> &'static str { "TinyGlobbyProcessor" }

    async fn process_one(&self, fop: Fop) -> Vec<Fop> {
        // Skip if already resolved to a concrete file
        if fop.filename.is_some() {
            return vec![fop];
        }
        
        let pattern = fop.file_or_pattern.clone();
        let base_fop = fop;
        
        // glob crate is sync and hits filesystem—wrap in spawn_blocking
        let result = tokio::task::spawn_blocking(move || {
            let mut outputs = Vec::new();
            
            match glob::glob(&pattern) {
                Ok(paths) => {
                    for entry in paths.flatten() {
                        let mut new_fop = base_fop.clone();
                        new_fop.filename = Some(entry);
                        new_fop.pattern = Some(Pattern::new(pattern.clone()));
                        outputs.push(new_fop);
                    }
                }
                Err(e) => {
                    let mut err_fop = base_fop;
                    err_fop.err = Some(ProcessorError::new(
                        "TinyGlobbyProcessor",
                        format!("invalid pattern: {}", e),
                    ));
                    outputs.push(err_fop);
                }
            }
            outputs
        }).await;
        
        match result {
            Ok(outputs) => outputs,
            Err(e) => {
                let mut err_fop = Fop::new(String::new()); // fallback
                err_fop.err = Some(ProcessorError::new(
                    self.name(),
                    format!("task join error: {}", e),
                ));
                vec![err_fop]
            }
        }
    }
}
```

---

## 5. The "Flyweight" Pattern: Is It Accurate?

### 5.1 GoF Flyweight Definition

The Gang of Four Flyweight pattern is about **sharing immutable intrinsic state** to reduce memory usage when creating many similar objects. The classic example is character glyphs in a text editor—you don't store font/style data on every character; you share it.

### 5.2 Current Fop Is Not Really a Flyweight

Your Fop:
- Is owned (not shared)
- Gets cloned frequently (especially in glob expansion)
- Accumulates **mutable extrinsic state** (filename, content, etc.)
- Has no shared intrinsic state

This is more accurately described as:
- An **envelope** or **context record** flowing through a pipeline
- A **data enrichment pattern** where each stage adds fields
- Similar to the **Builder pattern** but for data accumulation

### 5.3 Making It Actually Flyweight-ish

If you wanted flyweight semantics:

```rust
pub struct Fop {
    /// Shared immutable identity (intrinsic state)
    pub identity: Arc<FopIdentity>,
    
    /// Per-instance enrichment (extrinsic state)
    pub filename: Option<PathBuf>,
    pub content: Option<Content>,
    // ...
}

pub struct FopIdentity {
    pub file_or_pattern: String,
    pub original_index: usize,  // position in original input
}
```

Now cloning for fan-out is cheap—only the extrinsic fields are cloned; identity is reference-counted.

---

## 6. Trait Design Critique

### 6.1 Current Processor Trait Issues

```rust
pub trait Processor: Send + Sync {
    fn process<'a, I>(&self, input: I) -> impl Iterator<Item = Fop> + 'a
    where
        I: Iterator<Item = Fop> + 'a;
    
    fn name(&self) -> &str;
}
```

**Problems**:

1. **Not object-safe**: `impl Iterator` return type prevents `dyn Processor`
2. **Complex lifetime bounds**: The `'a` lifetime is tricky to work with
3. **Cannot be async**: No way to `.await` inside process
4. **Hard to compose dynamically**: Can't store `Vec<Box<dyn Processor>>`

### 6.2 Recommended Async Trait Design

For a library with configurable pipelines, prefer an object-safe design:

```rust
use futures::stream::BoxStream;

/// Object-safe stream processor
pub trait StreamProcessor: Send + Sync {
    /// Processor name for debugging
    fn name(&self) -> &'static str;
    
    /// Transform a stream of Fops
    fn process(&self, input: BoxStream<'static, Fop>) -> BoxStream<'static, Fop>;
}
```

This enables:
```rust
// Dynamic pipeline composition
let processors: Vec<Arc<dyn StreamProcessor>> = vec![
    Arc::new(CheckExistProcessor),
    Arc::new(TinyGlobbyProcessor),
    Arc::new(ReadContentProcessor::new()),
];

let mut stream = initial_stream;
for proc in processors {
    stream = proc.process(stream);
}
```

### 6.3 Current Stamper Trait Issues

```rust
pub trait Stamper: Send + Sync {
    fn start(
        &self,
        options: &StamperOptions,
        processor_name: &str,
        fop: &Fop,
    ) -> StamperHandle<Box<dyn std::any::Any + Send + Sync>>;
}
```

**Problems**:

1. **Uses `Any` for type erasure**: Caller must downcast, losing type safety
2. **Promise/Resolver pattern is JS-ish**: Rust prefers RAII or token patterns
3. **StamperHandle has awkward ownership**: Receiver and optional Sender together
4. **Unclear integration**: Who awaits the promise? Who writes to Fop?

### 6.4 Recommended Stamper Design

Use a typed token pattern:

```rust
/// Token returned from start(), consumed by end()
pub trait Stamper: Send + Sync {
    type Token: Send;
    
    /// Start timing
    fn start(&self, processor: &'static str, fop: &Fop) -> Self::Token;
    
    /// End timing and return the measurement
    fn end(&self, token: Self::Token) -> TimestampInfo;
}

/// High-precision performance stamper
pub struct PerfStamper;

impl Stamper for PerfStamper {
    type Token = minstant::Instant;
    
    fn start(&self, _processor: &'static str, _fop: &Fop) -> Self::Token {
        minstant::Instant::now()
    }
    
    fn end(&self, token: Self::Token) -> TimestampInfo {
        TimestampInfo::new(token.elapsed().as_millis() as u64)
    }
}
```

Usage in a processor:
```rust
async fn process_one(&self, mut fop: Fop) -> Vec<Fop> {
    let token = self.stamper.start(self.name(), &fop);
    
    // ... do work ...
    
    fop.timestamp = Some(self.stamper.end(token));
    vec![fop]
}
```

Benefits:
- Typed (no `Any`)
- Zero heap allocation for timing
- No channels or tasks
- Clear ownership: token moves from start to end

---

## 7. Migration Path

### Phase 1: Async Foundation
1. Create `AsyncProcessor` trait alongside existing `Processor`
2. Add `apply_processor` and `apply_bounded` stream combinators
3. Migrate one processor (e.g., `ReadContentProcessor`) as proof of concept
4. Add tests using `#[tokio::test]`

### Phase 2: Core Processor Migration
1. Migrate remaining processors to `AsyncProcessor`
2. Wrap blocking operations (`glob`) in `spawn_blocking`
3. Update `Fop` with `Arc<str>` for cheap cloning
4. Remove or deprecate sync `Processor` trait

### Phase 3: Pipeline Builders
1. Implement `SimplePipeline` and `EREbPipeline` using streams
2. Add pipeline builder API for custom composition
3. Implement outputters (`collect`, `for_each`, etc.)

### Phase 4: Cleanup
1. Rework Stamper to typed token pattern
2. Remove `tokio::sync::oneshot` from stamper
3. Clean up `ProcessorError` to use proper error types
4. Update documentation

---

## 8. Risks and Guardrails

### Backpressure
Fan-out processors (glob) can explode output. Ensure:
- Downstream consumers can keep up
- Consider optional caps on match count
- Monitor memory for large globbing operations

### Ordering
`buffer_unordered` reorders outputs. If order matters:
- Use `buffered(n)` instead (slower, preserves order)
- Attach sequence numbers and sort downstream
- Document the ordering guarantee (or lack thereof)

### Blocking in Async
Any sync filesystem or CPU-heavy work inside async context should use `spawn_blocking`:
- `glob::glob` iterates the filesystem
- Large string processing
- Compression/decompression

### Error Strategy
Decide and document one of:
- **Errors embedded in Fop**: Pipeline always continues, errors collected
- **Errors returned via Result**: Pipeline can short-circuit

Current design uses embedded errors but inconsistently. Pick one and enforce it.

---

## 9. Quick Wins (Low-Effort Improvements)

Without a full async migration, these can be done now:

1. **Fix the allocation roundtrip** in TinyGlobbyProcessor:
   ```rust
   // Before
   results.into_iter().collect::<Vec<_>>().into_iter()
   // After  
   results.into_iter()
   ```

2. **Don't mutate file_or_pattern** in SemaphoreBoundedProcessor

3. **Use Arc<str> for file_or_pattern**:
   ```rust
   pub file_or_pattern: Arc<str>,
   ```

4. **Share match_results across fan-out**:
   ```rust
   pub match_results: Option<Arc<[PathBuf]>>,
   ```

5. **Fix ProcessorError::from discarding processor field**:
   ```rust
   impl std::fmt::Display for ProcessorError {
       fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
           write!(f, "[{}] {}", self.processor, self.source)
       }
   }
   // Remove the broken From<ProcessorError> for String impl
   ```

---

## 10. Conclusion

The current implementation shows solid understanding of the problem domain and has good bones—the processor pipeline concept, flyweight data accumulation, and separation of concerns are all appropriate. However, the sync/async mismatch is fundamental and needs resolution.

The recommended path:
1. **Embrace async fully** with `futures::Stream` and `AsyncProcessor`
2. **Use `Arc<str>`** for cheap identity cloning
3. **Simplify stamping** with typed tokens instead of channels
4. **Clarify error handling** by picking one strategy and enforcing it

This will result in a design that:
- Matches the original JS async-iterable mental model
- Enables proper bounded concurrency with semaphores
- Uses non-blocking I/O throughout
- Is idiomatic Rust with clear ownership
