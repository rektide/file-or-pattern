# FsstreamProcessor Implementation Plan

## Context

The current `TinyGlobbyProcessor` uses `spawn_blocking` to offload sync glob operations to tokio's blocking thread pool. This approach has significant limitations:

### Problems with spawn_blocking Approach

1. **Limited blocking pool**: Tokio's default blocking pool size equals CPU count (often 8-16)
2. **Thread exhaustion**: Processing 1000 glob patterns simultaneously blocks all pool threads
3. **No true async**: Each glob blocks a thread for the entire duration of filesystem traversal
4. **Scalability bottleneck**: High concurrency scenarios (e.g., CLI tools processing thousands of paths) hit hard limits

### Example Failure Scenario

```rust
// Process 1000 glob patterns concurrently
let fops: Vec<_> = patterns.iter().map(|p| Fop::new(p)).collect();
let processor = TinyGlobbyProcessor::new();

// Each spawn_blocking consumes a thread from pool (e.g., 8 threads)
// First 8 complete, rest wait... creates bottleneck
let results = stream::iter(fops)
    .map(|fop| processor.process_one(fop))
    .buffer_unordered(1000)  // Will stall!
    .collect();
```

## fsstream Alternative

The `fsstream` crate (v0.1.0) provides true async directory scanning using `tokio::fs::read_dir()`.

### Key Advantages

1. **Non-blocking**: Uses async I/O primitives from tokio - no blocking threads
2. **Streaming**: Returns results via `ScanHandle` with async channel receiver
3. **Configurable concurrency**: `num_futures` parameter controls parallelism
4. **Depth control**: `max_depth` prevents runaway traversals
5. **Pattern matching**: Uses `globset` for efficient glob pattern matching
6. **Cancellation support**: Works with `CancellationToken` for graceful shutdown
7. **Include/exclude patterns**: Flexible filtering via `SimpleStrategy`

## Architecture

### Pattern Parsing

FsstreamProcessor must parse glob patterns into:
- `base_dir`: Directory to start scanning from
- `glob_pattern`: Pattern to match files against

**Parsing Rules:**

| Pattern Type | Example | Base Dir | Glob Pattern |
|--------------|---------|-----------|---------------|
| Wildcard start | `*.txt` | `.` | `*.txt` |
| Relative path | `src/**/*.rs` | `src` | `**/*.rs` |
| Absolute path | `/usr/lib/*.so` | `/usr` | `lib/*.so` |
| Nested wildcards | `src/**/test/*.txt` | `src` | `**/test/*.txt` |

**Parsing Algorithm:**
1. Find first wildcard position (`*`, `?`, `[`, `{`)
2. Extract text before first wildcard
3. Find last path separator in that prefix
4. Everything before separator = base directory
5. Everything after separator = glob pattern
6. If no separator = current directory (`.`)

### Async Flow

```
process_one(fop)
    │
    ├─► Skip if fop.filename already set (return fop unchanged)
    │
    ├─► Parse pattern → (base_dir, glob_pattern)
    │
    ├─► Check base_dir.exists()
    │
    ├─► Create SimpleStrategy with glob_pattern
    │
    ├─► Call strategy.scan_streaming(base_dir, cancel_token)
    │   └─► Returns ScanHandle { receiver, join_handle }
    │
    ├─► While receiver.recv():
    │   └─► For each matched path:
    │       ├─► Clone fop
    │       ├─► Set fop.filename = Some(path)
    │       └─► Add to results vec
    │
    ├─► Wait for join_handle to complete
    │
    ├─► If !results.is_empty():
    │   └─► Create Arc<Pattern> and add to all results
    │       (preserves pattern sharing across fan-out)
    │
    └─► Return results
```

### Integration with Existing Pipeline

FsstreamProcessor implements `AsyncProcessor` trait:
- `name()`: Returns "FsstreamProcessor"
- `process_one(fop)`: Async function returning `Vec<Fop>`

Works seamlessly with existing stream combinators:
```rust
use file_or_pattern::{FsstreamProcessor, apply_processor, apply_bounded};

let processor = FsstreamProcessor::new();
let fops = vec![
    Fop::new("src/**/*.rs"),
    Fop::new("test/*.txt"),
];

// Sequential processing
let results = stream::iter(fops)
    .then(|fop| processor.process_one(fop))
    .collect::<Vec<_>>()
    .await;

// Bounded concurrency (max 4 globs at once)
let results = stream::iter(fops)
    .map(|fop| processor.process_one(fop))
    .buffer_unordered(4)
    .collect::<Vec<_>>()
    .await;
```

## Implementation Details

### Dependencies

Added to `Cargo.toml`:
```toml
[dependencies]
fsstream = "0.1"
tokio-util = "0.7"  # For CancellationToken
```

### File Structure

- `src/basic/fsstream.rs`: New processor implementation
- `src/basic/mod.rs`: Export FsstreamProcessor
- `src/lib.rs`: Export from crate root

### Key Functions

#### `parse_pattern(pattern: &str) -> Result<(PathBuf, String), ProcessorError>`

Extracts base directory and glob pattern from input.

**Edge Cases:**
- Empty pattern: Returns `(., *)`
- Pattern starting with `/`: Base is root or explicit path
- No wildcards: Base is entire pattern, glob is `*`
- Invalid UTF-8: Returns ProcessorError

#### `AsyncProcessor::process_one(fop: Fop) -> Vec<Fop>`

Main async processing function.

**Error Handling:**
- Pattern parsing error → Returns single Fop with `.err` set
- Base directory missing → Returns single Fop with `.err` set
- Pattern build error → Returns single Fop with `.err` set
- Scan errors logged but don't return Fop (continue scan)

**Cancellation:**
- Uses `CancellationToken::new()` for each scan
- Currently no external cancellation (future enhancement)

## Testing Strategy

### Unit Tests

1. **Pattern parsing**: Verify correct (base_dir, glob_pattern) extraction
2. **Pattern expansion**: Create temp dir with files, verify matches
3. **Non-blocking**: No spawn_blocking used (compile-time guarantee)
4. **Error handling**: Invalid patterns, missing directories
5. **Concrete files**: Skip expansion when `.filename` already set

### Integration Tests

Reuse existing `TinyGlobbyProcessor` tests to verify compatibility:

```rust
#[tokio::test]
async fn test_fsstream_pattern_expansion() {
    let dir = tempdir().unwrap();
    // ... create test files ...

    let processor = FsstreamProcessor::new();
    let fop = Fop::new(dir.path().join("*.txt").to_str().unwrap());

    let results = processor.process_one(fop).await;
    assert!(results.len() >= 2);
    assert!(results[0].pattern.is_some());
}
```

### Performance Tests

Compare FsstreamProcessor vs TinyGlobbyProcessor:

```rust
#[tokio::test]
async fn test_concurrent_glob_performance() {
    let start = Instant::now();

    // Process 100 patterns concurrently
    let processor = FsstreamProcessor::new();
    let results = stream::iter(0..100)
        .map(|_| Fop::new("src/**/*.rs"))
        .then(|fop| processor.process_one(fop))
        .buffer_unordered(50)
        .collect::<Vec<_>>()
        .await;

    let duration = start.elapsed();
    // FsstreamProcessor should be non-blocking, fast
    assert!(duration.as_millis() < 1000);
}
```

## Migration Path

### Phase 1: Implementation (Current)
- [x] Add fsstream dependency
- [x] Create FsstreamProcessor struct
- [x] Implement pattern parsing logic
- [x] Implement AsyncProcessor trait
- [x] Add unit tests
- [ ] All tests passing

### Phase 2: Integration
- [ ] Pass all TinyGlobbyProcessor compatibility tests
- [ ] Update documentation examples
- [ ] Add benchmark comparing to spawn_blocking approach

### Phase 3: Adoption
- [ ] Update CLI examples to use FsstreamProcessor
- [ ] Deprecate TinyGlobbyProcessor (add notice)
- [ ] Consider migration in Phase 2 (fop-async-migrate-core)

## Trade-offs

### Pros of FsstreamProcessor

1. **True async**: No thread pool exhaustion
2. **Scalable**: Handles 1000+ concurrent glob patterns
3. **Configurable**: Fine-grained control over concurrency
4. **Better cancellation**: Token-based cancellation support
5. **Streaming API**: Can process results incrementally

### Cons of FsstreamProcessor

1. **Additional dependency**: Requires `fsstream` + `tokio-util` (2 crates)
2. **More complex**: Pattern parsing logic is non-trivial
3. **Different behavior**: Minor differences from glob crate edge cases
4. **New code**: Less battle-tested than glob crate

### When to Use Each

**Use FsstreamProcessor when:**
- Processing many patterns concurrently (10+)
- Building CLI tools with glob support
- Need fine-grained concurrency control
- Working with async-first architecture

**Use TinyGlobbyProcessor when:**
- Processing single pattern or few patterns
- Minimal dependencies preferred
- Need exact glob crate compatibility
- Simple scripts/tools

## Conclusion

FsstreamProcessor provides a scalable, truly async solution for glob expansion. The trade-off of an additional dependency is justified by the significant performance and scalability improvements in high-concurrency scenarios.
