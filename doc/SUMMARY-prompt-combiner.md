# SUMMARY: Prompt Combiner for FILE-OR-PATTERN

## High-Level Overview

**PROMPT-COMBINER** is a demonstration tool that showcases the FILE-OR-PATTERN library's capabilities by combining multiple prompt files into a unified document. The tool uses FILE-OR-PATTERN's pipeline architecture and async generator patterns to process files from a `prompt/` directory, normalizing content (adding markdown headers and identification lines) and merging everything into a single output.

**Why this matters:** This project serves as both a practical utility and a comprehensive example of FILE-OR-PATTERN's design patterns, making the library easier to understand and use.

## Core Architecture

The system is built around four main components that work together in a streaming pipeline:

### **PROMPT-COMBINER** (fop-kyg.5)
Main orchestrator module that:
- Accepts user input (file paths and/or glob patterns)
- Uses FILE-OR-PATTERN pipeline for file discovery
- Coordinates the entire workflow from discovery to output

### **PROMPT-STREAM** (fop-kyg.6)
Async generator system that:
- `prompts()` - yields list of discovered file paths
- `promptsWithContent()` - yields `{name, content}` objects
- Built using FILE-OR-PATTERN processors for through-processing

### **PROMPT-TRANSFORMER** (fop-kyg.7)
Content normalization logic that:
- Detects existing markdown headers (`#`)
- Adds missing headers based on filename (e.g., `my-prompt.md` → `# My Prompt`)
- Adds identification line: `This is the prompt called <filename>`
- Preserves original content structure

### **COMBINED-WRITER** (fop-kyg.8)
Output generation that:
- Consumes transformed prompt stream
- Writes combined output with file separators
- Manages file ordering and formatting
- Supports multiple output targets (file or stdout)

## How It Works: Data Flow

```
User Input (files/patterns)
    ↓
PROMPT-COMBINER (orchestrates)
    ↓
FILE-OR-PATTERN Pipeline:
  ParserProcessor → CheckExistProcessor → 
  TinyGlobbyProcessor → ReadContentProcessor
    ↓
PROMPT-STREAM (async generator yields {name, content})
    ↓
PROMPT-TRANSFORMER (per-file: add headers & IDs)
    ↓
COMBINED-WRITER (merge with separators)
    ↓
Output Document
```

## Implementation Strategy

The project follows a four-phase approach:

**Phase 1: Foundation** (fop-kyg.1, fop-kyg.2)
- Research FILE-OR-PATTERN API and async generator patterns
- Set up basic project structure
- Create entry point script

**Phase 2: Core Implementation** (fop-kyg.3, fop-kyg.5, fop-kyg.6)
- Implement PROMPT-STREAM async generators using FILE-OR-PATTERN pipeline
- Build content transformation logic
- Create basic combining functionality

**Phase 3: Refinement** (fop-kyg.4, fop-kyg.7, fop-kyg.8, fop-kyg.9)
- Implement PROMPT-TRANSFORMER with header generation
- Build COMBINED-WRITER with separators
- Design and implement CLI interface

**Phase 4: Quality** (fop-kyg.10, fop-kyg.11)
- Add error handling and validation
- Test main use cases
- Document usage patterns

## Key Research Needs

### FILE-OR-PATTERN Integration (fop-kyg.1)
- How to properly construct and use `SimplePipeline`
- Exact method signatures for pipeline builders
- Error handling patterns within the pipeline
- Using existing processors vs. creating custom `PromptTransformProcessor`

### Async Generator Patterns (fop-kyg.2)
- Integrating async generators with FILE-OR-PATTERN's streaming behavior
- Memory-efficient handling of large file sets
- Proper yield patterns for through-processing

### Content Transformation (fop-kyg.3)
- Robust header detection and generation
- Handling edge cases (empty files, existing headers, unusual filenames)
- Preservation of original formatting

### CLI Design (fop-kyg.4)
- Output format options (file vs. stdout)
- Verbose/debug modes
- Separator customization
- File ordering controls

## Technical Decisions

**FILE-OR-PATTERN Pipeline:** Start with `SimplePipeline` (Parser → Globby → Read), evaluate whether custom processor needed later.

**Async Generators:** Use for all streaming operations to maintain FILE-OR-PATTERN's through-processing pattern and support memory-efficient handling of large file sets.

**Output Format:** Markdown with `---` separators between files, preserve alphabetical file order by default.

**Error Handling:** Follow FILE-OR-PATTERN's error pattern (set `err` field, let downstream processors decide).

## Success Criteria

- Successfully discovers files from `prompt/` directory using FILE-OR-PATTERN
- Handles both explicit file paths and glob patterns
- Reads and transforms file contents (headers + IDs)
- Combines content into single output document
- Uses async generators for streaming behavior
- Properly uses FILE-OR-PATTERN pipeline architecture
- Includes error handling and validation
- Documented with clear examples
- Tests covering main use cases

## Relationship to Full Plan

This summary provides the high-level view. For detailed implementation guidance, see:
- `/doc/PLAN-prompt-combiner-for-file-or-pattern.md` - Complete technical specification
- Beads tickets `fop-kyg.x` - Task breakdown and progress tracking

## Next Steps

1. **fop-kyg.1**: Research FILE-OR-PATTERN API in depth
2. **fop-kyg.2**: Study async generator patterns for PROMPT-STREAM
3. **fop-kyg.5**: Create initial proof-of-concept with basic pipeline
4. **fop-kyg.6**: Implement PROMPT-STREAM async generators
5. **fop-kyg.7**: Build PROMPT-TRANSFORMER content logic
6. **fop-kyg.8**: Create COMBINED-WRITER output generation
7. **fop-kyg.9**: Implement CLI interface
8. **fop-kyg.10**: Add error handling and validation
9. **fop-kyg.11**: Test and document usage patterns
