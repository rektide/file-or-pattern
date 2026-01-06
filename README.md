# FILE-OR-PATTERN

File Or Pattern is a common CLI program argument technique that we want to reach for, and that this library provides.

Many CLI programs expect a file name, but we want to expedite the user experience & hasten their operational speed. We allowing them to provide a regex pattern instead. This is implicit, the user does not signal they are using a pattern. If there is no FILE (or other record of any other file type) found at the provided argument, we instead treat the argument as a pattern.

The output of File Or Pattern is, at the most basic level, an iterable of matched filenames.

## Patterns

- Implementation is as a PIPELINE of PROCESSORS that do one specific step.
- Iterability is the key principle. We want through - streaming behavior for FILE-OR-PATTERN.
- Inputs and outputs usually ought be Iterables (for input) or Iterators (for outputs). Implementation often comes in the form of generators.
- Pass through. Processors exercise a FLY-WEIGHT pattern, extending the existing object with additional fields and extending it's type.
- When we have multiple inputs, we do not want to block and wait for completion of a processing step. Any transformations / processors comprising the pipeline will eagerly consume their results, and KICK OFF WORK.
- We have two EXECUTION MODES.
  - By default we have a UNBOUNDED MODE, which immediately runs the scheduled work to kick off.
  - We also have TARN BOUNDED MODE, which uses a tarn.js resource pool to limit the available number of executors that are running. This is mainly useful for executability topics, or other more advanced pipelines.
- We have multiple BUILDER patterns for building pipelines and also sub-pipelines, that can be used to assemble a final pipeline, in a configured state, that actually accepts input. Don't over-implicate too much, but Builders usually don't actually have the FILE-OR-PATTERN as inputs to the pipelines they create: they produce a pipeline that DOES accept inputs, that creates an output stream.
- Outputters are a special class of builder, that help marshal a stream into a more directly consumable result. `FromOutput` is the most basic, returning a promise, using the Async-Iteration-Helper Array.fromAsync to gather the results.

## FOP Field Index

The fop (File Or Pattern) is a flyweight passed through the pipeline. Common fields are as follows.

| field | description |
| `fileOrPattern` | a field with the original user input |
| `filename` | a field set when we know we are dealing with a concrete existing file |
| `executable` | true/false indicating the filename is executable |
| `match` | a field set when we know there is no fileOrPattern, and which is the pattern matcher results |
| `pattern` | a field that represents the matcher that detected the match |
| `content` | resulting content of the fop |
| `encoding` | the file encoding we read from, or null for a byte collection nthing |
| `timestamp` | execution duration information for the fop |
| `err` | an error, ideally with a `processor` field on it to designate the processor where the error occured |

## Processors

### `ParserProcessor`

Turns user strings into a FileOrPattern object, which consists of an object with a `fileOrPattern` with the string as it's value. This is usually the first step in the pipeline, and creates the flyweight object. But it itself does not do any processing or consideration.

- _output:_ an object with `fileOrPattern`.
- `guard(true)`: if there is already an object, throw if it does not have a `fileOrPattern` on it already.

### `CheckExistProcessor`

Check Exists Strategy checks whether an input flyweight that it is processing exists.

- _ouput:_ `filename` is attached to the fop if the `fileOrPath` exists.

### `TinyGlobbyProcessor`

Use `tinyglobby` to find matching files. skips anything with a `filename` on it.

- _output:_

### `ReadContentProcessor`

Read File Strategy attempts to retrieves the file contents.

- _input:_ reads from `filename` or
- _output:_ `content` is attached to fop
- `encoding('utf8')` option specified encoding to read.
- `recordEncoding(false)` option specified to write a `encoding` field on the fop.

### `DoExecuteProcessor`

Do Execute Processor is a processor that checks whether a given `filename` is executable, and runs it, building content. Uses `tinyexec`.

- _input:_ either a `filename` if found, falling back to `fileOrPath`, which it will assume to be a `filename`. no globbing.
- _output:_ `content` attached with execution output. `err` will be attached if executable detected but failed.
- `executor(TinyExecutor)` option accepts a strategy for running execution, attached as `execution`
- `executionStamper(null)` option attaches a `executionStamp`, via a stamper. typically represents the running duration of the execution. if true defaults to `PerformanceMeasureTimestamper`
- `executionName('executionStamp')` option to pick the name to assign the time-stamp to.
- `failChecker()` option is a strategy to determine whether a run succeeded or failed. only called if execution is started.
- `expectExecution` option will create attach `err` and halt if `filename` is not an executable.

- note: all options in the constructor are passed through to tinyexec, for controlling things like stdio.

### `ZxExecuteProcessor`

A variant of `DoExecuteProcessor` that uses `zx` for execution, which has a nice user visible output by default.

### `GuardProcessor`

Throws if the fop has an `err`, else passes through.

### TarnBounderProcessor

A processor that uses Tarn.js to limit number of executing input items running at any given time.

- _output:_ the input fop
- `waitStamper(null)` option is a stamper that attaches a
- `waitName('waitStamp')` option sets the key to write the `waitStamp` to

## Stamper

Stampers generate supplemental execution information about the pipeline process they are stamping.

- _input:_ positional arguments,
  - _option_ object with optional arguments to take.
    - if processor is found, use that and skip processor
  - _processor_ argument with the processor running, omitted if option.processor exists
  - _fop_ argument with the fop we are running
- _output:_ a deferrable, a `Promise.withResolvers()` shaped output (`.promise`, `.resolve()`, `.reject()`).

note: perhaps _preceeded_ by optional arguments, to allow Rambda like data-last usage / being set up with .bind().

### PerformancMeasureTimestamp

A strategy to use for creating start and end timestamps. Defaults to a performance marker timestamper, that uses JavaScript's Performance API to get a PerformanceMeasure.

- _output:_ a `Performance.Mark` object.
- `startNamer` option, a strategy that generates a name to use for Performance Mark at the start of execution. fop passed as input. has a default implementation that uses the fileOrPath, prefixed.
- `endSuffixNamer` option, a strategy that generates a suffix for the Performance Measure at the end of execution. can also be a string literal. fop passed as input. ouput is appened to the start name, with a `-` inbetween.

### TrueTimestamper

A trivial timestamper that simply resolves `true` no matter what when resolved.

## Pipeline Builders

### EREbPipeline

Builder for a recommended pipeline.

- Named `E`/`R`/`E` `bounded`, execute/read/execute, bounded. Will not flood the system, gives user maximum power. Do What They Will-ist-most!
- Recommended and nifty.
- Shares a single bounded executor for file operations.
- Each piece builds the flyweight such

```
bounder = TarnBounderProcesor
ParserProcessor -> bounder(DoExecuteProcessor) -> TinyGlobbyProcesor -> bounder(DoExecuteProcessor) -> bounder(ReadContentProcessor)
```

### SimplePipeline

Explodes patterns but nothing else; what this library actually says on the tin.

```
ParserProcessor -> TinyGlobbyProcessor -> ReadContentProcessor
```

# TODO

- Critical:
- Environment variables for tuning components, common configuration tools.
  - `<NAME>_APP_NAME` configurability ala `NVIM_APPNAME`!
  - c12?
- Explore how we might instead model and build a solution to this problem atop EffectJS.
- Some kind of generally usable `namer` strategy.
- OTel instrumentation
