# FOP Implementation

## Overview

Implement a Rust library that provides FILE-OR-PATTERN functionality for CLI programs. The library accepts either a file path or a pattern (implicitly determined), and returns an iterator of matched filenames. The system is built as a pipeline of processors with streaming behavior.

## Architecture

### Core Concepts

- **FOP (File Or Pattern)**: A flyweight object passed through the pipeline, accumulating fields as it's processed
- **PROCESSOR**: Single-step transformation units that operate on FOPs
- **PIPELINE**: A chain of processors that transform FOPs through multiple stages
- **STREAMING**: Iteration-based, non-blocking pipeline execution
- **EXECUTION MODES**: UNBOUNDED (immediate) and TARN-BOUNDED (resource-limited)
- **BUILDER**: Pattern for constructing configured pipelines that accept inputs

### FOP Field Index

The FOP is a flyweight structure that accumulates fields through the pipeline:

| field | type | description |
|-------|------|-------------|
| `file_or_pattern` | `String` | Original user input |
| `filename` | `Option<PathBuf>` | Concrete existing file path |
| `executable` | `Option<bool>` | Whether filename is executable |
| `match_results` | `Option<Vec<PathBuf>>` | Pattern matcher results |
| `pattern` | `Option<Pattern>` | The matcher that detected the match |
| `content` | `Option<Content>` | Resulting content (bytes or string) |
| `encoding` | `Option<String>` | File encoding read from |
| `timestamp` | `Option<TimestampInfo>` | Execution duration information |
| `err` | `Option<ProcessorError>` | Error with processor field |

### Data Flow

```
User Input → ParserProcessor → CheckExistProcessor → TinyGlobbyProcessor → ReadContentProcessor/DoExecuteProcessor
                ↓                    ↓                      ↓                              ↓
            Creates FOP         Adds filename        Explands patterns         Reads content or executes
```

### Processor Pipeline

#### Recommended Pipeline: EREbPipeline

```
ParserProcessor → TarnBounder(DoExecuteProcessor) → TinyGlobbyProcessor → TarnBounder(DoExecuteProcessor) → TarnBounder(ReadContentProcessor)
```

- Execute/Read/Execute, bounded
- Prevents system flooding
- Maximum power to user

#### SimplePipeline

```
ParserProcessor → TinyGlobbyProcessor → ReadContentProcessor
```

- Pattern expansion only
- What the library advertises

## Components

### Core Types

#### `Fop` Struct
```rust
pub struct Fop {
    pub file_or_pattern: String,
    pub filename: Option<PathBuf>,
    pub executable: Option<bool>,
    pub match_results: Option<Vec<PathBuf>>,
    pub pattern: Option<Pattern>,
    pub content: Option<Content>,
    pub encoding: Option<String>,
    pub timestamp: Option<TimestampInfo>,
    pub err: Option<ProcessorError>,
}

pub enum Content {
    Bytes(Vec<u8>),
    Text(String),
}
```

### Processors

#### `ParserProcessor`
- **Input**: `Iterator<Item = String>` (user strings)
- **Output**: `Iterator<Item = Fop>`
- Creates FOP objects with `file_or_pattern` field
- Optionally validates existing objects have `file_or_pattern`

#### `CheckExistProcessor`
- **Input**: `Iterator<Item = Fop>`
- **Output**: `Iterator<Item = Fop>` with `filename` added if path exists
- Checks filesystem existence of `file_or_pattern`

#### `TinyGlobbyProcessor`
- **Input**: `Iterator<Item = Fop>`
- **Output**: `Iterator<Item = Fop>` with `match_results` and `pattern` added
- Skips FOPs that already have `filename`
- Uses glob pattern matching

#### `ReadContentProcessor`
- **Input**: `Iterator<Item = Fop>`
- **Output**: `Iterator<Item = Fop>` with `content` added
- Reads from `filename` field
- Options: encoding, record_encoding

#### `DoExecuteProcessor`
- **Input**: `Iterator<Item = Fop>`
- **Output**: `Iterator<Item = Fop>` with `content` from execution
- Checks if `filename` is executable, runs it
- Uses `tinyexec` crate equivalent
- Options: executor strategy, execution_stamper, fail_checker, expect_execution

#### `GuardProcessor`
- **Input**: `Iterator<Item = Fop>`
- **Output**: `Iterator<Item = Fop>`
- Throws if FOP has `err`, otherwise passes through

#### `TarnBounderProcessor`
- **Input**: `Iterator<Item = Fop>`
- **Output**: `Iterator<Item = Fop>`
- Uses resource pool (tarn.js equivalent) to limit concurrent executions
- Options: wait_stamper, wait_name

### Stampers

Stampers generate supplemental execution information.

#### `Stamper` Trait
```rust
pub trait Stamper: Send + Sync {
    fn start(&self, options: &StamperOptions, processor: &Processor, fop: &Fop) -> StamperHandle;
}
```

#### `PerformanceMeasureTimestamper`
- Uses Rust's `std::time::Instant` for duration measurement
- Options: start_namer, end_suffix_namer
- Outputs timestamp info

#### `TrueTimestamper`
- Resolves to `true` regardless of input
- Trivial stamper for testing

### Builders

#### `PipelineBuilder`
Generic builder pattern for constructing pipelines:
- Configure processors with options
- Build pipeline that accepts input iterator
- Return output iterator

#### `Outputter`
Special builder for marshaling streams:
- `FromOutput`: Uses `TryFrom` trait to collect results into `Vec`
- Returns `Result<Vec<Fop>, Error>`

## Rust-Specific Considerations

### Crate Structure

```
file-or-pattern/
├── Cargo.toml
├── src/
│   ├── lib.rs           # Public API exports
│   ├── fop.rs           # FOP core types
│   ├── processor.rs     # Processor trait and implementations
│   ├── stamper.rs       # Stamper trait and implementations
│   ├── builder.rs       # Pipeline builders
│   ├── error.rs         # Error types
│   └── pipelines/
│       ├── mod.rs
│       ├── ereb.rs      # EREbPipeline
│       └── simple.rs    # SimplePipeline
└── bin/
    └── fop-demo.rs      # Demo CLI program
```

### Key Dependencies

- `glob` or `globset` for pattern matching
- `tokio` for async/await (if async execution needed)
- `anyhow` or `thiserror` for error handling
- `itertools` for iterator utilities
- Resource pooling crate (tarn equivalent in Rust)

### Iteration Model

- Use `Iterator<Item = Fop>` for input
- Use `impl Iterator<Item = Fop>` for output
- Generator-style processors using `std::iter::from_fn`
- Streaming behavior: processors consume eagerly but don't block

### Error Handling

- `ProcessorError` with `processor` field
- Use `Result<T, ProcessorError>` pattern
- Err attached to FOP `err` field, propagates through pipeline

## Demo Program

### Features
- Accept FILE-OR-PATTERN arguments from command line
- Use EREbPipeline by default
- Show execution modes (bounded/unbounded)
- Display match results
- Show content read or execution output

### Usage Examples

```bash
# Single file
fop-demo Cargo.toml

# Pattern
fop-demo "**/*.rs"

# Multiple inputs
fop-demo Cargo.toml src/**/*.rs

# With bounded mode
fop-demo --bounded "**/*.rs"
```

## Implementation Phases

### Phase 1: Core Infrastructure
- Define `Fop` struct and `Content` enum
- Define `Processor` trait
- Implement error types
- Set up crate structure and dependencies

### Phase 2: Basic Processors
- `ParserProcessor`
- `CheckExistProcessor`
- `TinyGlobbyProcessor` (glob integration)

### Phase 3: Content and Execution
- `ReadContentProcessor`
- `DoExecuteProcessor`
- `GuardProcessor`

### Phase 4: Bounding and Timing
- `TarnBounderProcessor` (resource pool)
- `Stamper` trait
- `PerformanceMeasureTimestamper`
- `TrueTimestamper`

### Phase 5: Builders and Pipelines
- `PipelineBuilder`
- `Outputter` trait
- `SimplePipeline`
- `EREbPipeline`

### Phase 6: Demo Program
- CLI argument parsing
- Pipeline integration
- Output formatting

## Research Areas

### Pattern Matching Libraries
- `glob` crate (simple, widely used)
- `globset` (faster, supports advanced patterns)
- `ignore` crate (glob patterns with gitignore support)

### Execution Libraries
- `tokio::process::Command` for async execution
- `std::process::Command` for sync execution
- Check executability: `Path::is_file()` + permissions check

### Resource Pooling
- `tokio::sync::Semaphore` for async bounded execution
- Custom thread pool for sync bounded execution
- `tarn` crate if available for Rust

### Timestamping
- `std::time::Instant` for duration
- `std::time::SystemTime` for absolute timestamps
- Optional: integration with metrics/tracing crates

## Testing Strategy

- Unit tests for each processor
- Integration tests for pipelines
- Property tests using `proptest` for pattern matching
- Demo program acceptance tests
- Benchmark tests for streaming performance

## Configuration

### Environment Variables
- `FOP_APP_NAME` for app-specific configuration (like `NVIM_APPNAME`)
- `FOP_BOUND_LIMIT` for bounded mode default limit
- `FOP_DEFAULT_ENCODING` for encoding defaults

### Config Loading
- Use `directories` crate for XDG config paths
- Optional: `config` crate or custom config parser

## Future Enhancements

- EffectJS alternative (explore using Rust effect systems if available)
- Generally usable `namer` strategy for generating names
- OpenTelemetry instrumentation
- Additional processors for common operations
- Async-only pipeline variant
