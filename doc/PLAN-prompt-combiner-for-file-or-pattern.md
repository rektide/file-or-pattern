# PLAN: Prompt Combiner for FILE-OR-PATTERN

## Context & Problem Statement

The FILE-OR-PATTERN library provides a flexible pipeline system for processing CLI arguments that can be either file names or patterns. We need to build a demonstration tool that showcases this library's capabilities while solving a practical problem: combining multiple prompt files from a `prompt/` directory into a single unified document.

The example prompt describes building a tool that:
- Searches a `prompt/` directory for files
- Creates an async generator to list files
- Creates an async generator to output name and content
- Creates a writeCombined consumer to merge everything
- Automatically adds markdown headers to files that don't have them
- Adds descriptive first lines to identify each prompt

## Research Phase

### File-Or-Pattern Library Understanding

From the README, FILE-OR-PATTERN provides:

**Core Concepts:**
- **FOP (File Or Pattern)**: A flyweight object passed through pipeline processors
- **Pipeline Architecture**: Chain of processors that transform FOP objects
- **Execution Modes**: Unbounded (default) and Tarn-bounded (resource-limited)
- **Pass-Through Pattern**: Processors extend FOP objects with additional fields
- **Streaming Behavior**: Inputs and outputs are Iterables/Iterators for through-processing

**Key FOP Fields:**
- `fileOrPattern`: Original user input string
- `filename`: Concrete file path when file exists
- `match`: Pattern matcher results when no file found
- `pattern`: The matcher that detected the match
- `content`: File contents or execution output
- `encoding`: File encoding or null for bytes
- `timestamp`: Execution duration info
- `err`: Error with optional `processor` field

**Key Processors:**
1. **ParserProcessor**: Creates FOP object from input string
2. **CheckExistProcessor**: Checks if file exists, sets `filename`
3. **TinyGlobbyProcessor**: Glob pattern matching (tinyglobby)
4. **ReadContentProcessor**: Reads file contents with encoding support
5. **DoExecuteProcessor**: Executes executable files using tinyexec
6. **GuardProcessor**: Error checking, throws if `err` present
7. **TarnBounderProcessor**: Resource limiting using tarn.js

**Pipeline Builders:**
- **EREbPipeline**: Recommended pipeline - Execute/Read/Execute with bounded execution
- **SimplePipeline**: Basic pattern explosion (Parser → Globby → Read)

### Prompt Combiner Requirements Analysis

**Input Specification:**
- Directory: `prompt/`
- Pattern matching support (e.g., `prompt/**/*.md`)
- Individual file arguments
- Accepts both file paths and glob patterns via FILE-OR-PATTERN

**Processing Requirements:**
- List all matching files
- Read each file's content
- Check for markdown header (`#`) at start
- If no header: add header based on filename
- Add first line: "this is the prompt called <filename>"
- Combine all content into single output

**Output Requirements:**
- Single combined document
- Preserve file boundaries (probably with separators)
- Maintain file order (probably alphabetical or sorted)

## System Architecture

### Component Design

**PROMPT-COMBINER** - Main orchestrator module
- Entry point for CLI usage
- Uses FILE-OR-PATTERN pipeline
- Coordinates file discovery, reading, and combining

**PROMPT-STREAM** - Async generator system
- `prompts()` - yields list of file paths
- `promptsWithContent()` - yields `{name, content}` objects
- Built using FILE-OR-PATTERN processors

**PROMPT-TRANSFORMER** - Content normalization
- Detects markdown headers
- Adds missing headers based on filename
- Adds identification lines
- Preserves original formatting

**COMBINED-WRITER** - Output generation
- Consumes transformed prompt stream
- Writes combined output
- Manages separators and formatting

### Data Flow

```
User Input (files/patterns)
    ↓
PROMPT-COMBINER
    ↓
FILE-OR-PATTERN Pipeline:
  ParserProcessor → CheckExistProcessor → 
  TinyGlobbyProcessor → ReadContentProcessor
    ↓
PROMPT-STREAM (async generator)
    ↓
PROMPT-TRANSFORMER (per file)
    ↓
COMBINED-WRITER
    ↓
Output Document
```

### Technical Implementation Strategy

**Phase 1: Foundation**
- Set up basic project structure
- Create entry point script
- Initialize FILE-OR-PATTERN pipeline
- Implement basic file discovery

**Phase 2: Content Processing**
- Implement prompt transformation logic
- Handle header detection and addition
- Add identification lines
- Preserve existing content structure

**Phase 3: Output Generation**
- Implement COMBINED-WRITER
- Handle separators between files
- Manage file ordering
- Support multiple output formats

**Phase 4: Refinement**
- Error handling and validation
- Performance optimization
- Documentation
- Testing

## Implementation Details

### Using FILE-OR-PATTERN

The tool should leverage existing pipeline builders:

**Option A: SimplePipeline**
```
ParserProcessor → TinyGlobbyProcessor → ReadContentProcessor
```
- Simple, direct pattern expansion
- Good for basic file discovery
- Limited execution capabilities

**Option B: Custom Pipeline**
Build custom pipeline for prompt-specific needs:
```
ParserProcessor → CheckExistProcessor → 
TinyGlobbyProcessor → ReadContentProcessor → 
PromptTransformProcessor
```
- Adds custom transformation processor
- More control over processing
- Fits our specific use case

**Decision**: Start with SimplePipeline, evaluate if custom processor needed

### Async Generator Implementation

**prompts() Generator:**
```typescript
async function* prompts(input: string[]): AsyncGenerator<string> {
  for (const item of input) {
    const pipeline = SimplePipeline.build();
    for await (const fop of pipeline(item)) {
      if (fop.filename) yield fop.filename;
      if (fop.match) {
        for (const match of fop.match) yield match;
      }
    }
  }
}
```

**promptsWithContent() Generator:**
```typescript
async function* promptsWithContent(input: string[]): 
  AsyncGenerator<{name: string, content: string}> {
  const pipeline = SimplePipeline.build();
  for await (const fop of pipeline(...input)) {
    if (fop.content && fop.filename) {
      yield { name: fop.filename, content: fop.content };
    }
  }
}
```

### Content Transformation Logic

**Header Detection:**
```typescript
function hasMarkdownHeader(content: string): boolean {
  return content.trimStart().startsWith('#');
}
```

**Header Generation:**
```typescript
function generateHeader(filename: string): string {
  const name = path.basename(filename, path.extname(filename));
  const header = name.split(/[-_]/)
    .map(word => word.charAt(0).toUpperCase() + word.slice(1))
    .join(' ');
  return `# ${header}`;
}
```

**Transformation:**
```typescript
function transformPrompt(name: string, content: string): string {
  const lines = content.split('\n');
  if (hasMarkdownHeader(content)) {
    return content;
  }
  
  const header = generateHeader(name);
  const identifier = `This is the prompt called ${path.basename(name)}`;
  
  return [header, identifier, '', content].join('\n');
}
```

### Write Combined Implementation

```typescript
async function writeCombined(
  prompts: AsyncGenerator<{name: string, content: string}>,
  output: WritableStream | string
): Promise<void> {
  const writer = typeof output === 'string' 
    ? fs.createWriteStream(output)
    : output;
  
  for await (const {name, content} of prompts) {
    const transformed = transformPrompt(name, content);
    await writer.write(transformed);
    await writer.write('\n\n---\n\n');
  }
  
  if (typeof output === 'string') {
    writer.close();
  }
}
```

## Open Questions & Research Needs

1. **FILE-OR-PATTERN API Clarity**
   - How to properly construct and use SimplePipeline?
   - What are the exact method signatures?
   - How to handle errors in the pipeline?

2. **Output Format**
   - What separator style should we use between files?
   - Should we preserve file order or sort alphabetically?
   - How to handle duplicate content?

3. **Error Handling**
   - What happens when a file can't be read?
   - How to report missing files?
   - Should we skip or fail on errors?

4. **CLI Interface**
   - What CLI arguments should we support?
   - Output file vs stdout?
   - Verbose/debug modes?

5. **Performance Considerations**
   - Large file handling?
   - Memory usage with many files?
   - Streaming vs buffering?

## Success Criteria

- [ ] Successfully discovers files from `prompt/` directory
- [ ] Handles both explicit file paths and glob patterns
- [ ] Reads file contents using FILE-OR-PATTERN
- [ ] Adds markdown headers to files without them
- [ ] Adds identification lines to all files
- [ ] Combines content into single output document
- [ ] Uses async generators for streaming behavior
- [ ] Properly uses FILE-OR-PATTERN pipeline architecture
- [ ] Includes error handling and validation
- [ ] Documented with clear examples
- [ ] Tests covering main use cases

## Next Steps

1. Research FILE-OR-PATTERN API in depth
2. Create initial proof-of-concept
3. Build out async generator system
4. Implement content transformation
5. Create output generation
6. Add CLI interface
7. Test and refine
8. Document usage patterns
