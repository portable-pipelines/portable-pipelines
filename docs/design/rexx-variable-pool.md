# REXX Variable Pool Design

This document describes how the REXX variable pool stages (STEM, VAR, VARSET, VARFETCH, REXXVARS, VARLOAD, VARDROP) work in Portable Pipelines.

## The Core Challenge

On IBM CMS and OS/2, a REXX program can invoke the external PIPE command, and pipeline stages like STEM and VAR can access the calling REXX program's variable pool. This works because of the host operating system's architecture (CMS program stacks, OS/2 REXX variable pool API).

On Windows, macOS, and Linux, an external process (the `pipe` executable) cannot access another process's memory, so it cannot reach the calling REXX program's variables.

## The Solution

Variable pool stages work when pipelines run **in-process** with the REXX interpreter. That happens when:

1. You name a `.rexx` / `.rex` program as a pipeline stage (the usual way to run REXX here)
2. The program uses **CALLPIPE** (ADDRESS `PIPELINE` is the Regina subcommand environment behind that)
3. Regina is loaded on the machine

The nested pipeline then shares the same Regina interpreter as the calling program, so STEM and friends can use Regina's RexxVariablePool API.

## Usage Pattern

The external `pipe` command cannot see another process's REXX variables:

```rexx
/* This cannot access REXX variables (separate process) */
ADDRESS SYSTEM 'pipe literal hello | stem RESULT.'
```

Run the REXX program as a stage, and CALLPIPE from inside it:

```rexx
/* photo_workflow.rexx, invoked as: pipe "examples/photo_workflow.rexx | console" */
'CALLPIPE literal hello | stem RESULT.'
SAY RESULT.0 RESULT.1
```

## Implementation Details

The variable pool stages use a hook mechanism to avoid circular dependencies:

1. `pipe-stages` defines the stage logic and declares a `VarPoolHook` type
2. `pipe-rexx` implements the actual Regina variable pool access in `varpool.rs`
3. When `pipe_rexx::install_hook()` is called, it registers the varpool hook with `pipe-stages`
4. Variable pool stages call through the hook to access Regina

When Regina is not available, or the stage is not running inside an active Regina program, the stages return a clear error.

Regina CALLPIPE uses a buffered host (RexxStart is not interleaved with dispatcher coroutines). Nested specifications still go through the same labeled multi-pipeline graph as the CLI, so LOOKUP with `(end ?)` and stage labels works from CALLPIPE.

## Stages

All six require Regina and an in-process REXX program. Syntax is position-based, like CONSOLE and DISK: first stage is a device driver, not-first is a sink. There are no INPUT / OUTPUT keywords.

### STEM

```rexx
STEM name
```

- First stage: reads `name.0` for the count (unset or empty means 0), then writes `name.1` .. `name.n`
- Not first: reads records into `name.1`, `name.2`, … and sets `name.0` to the count
- A trailing dot on the name is optional (`photos.` is the same as `photos`)
- Compound tails work (`FOO.BAR.` writes `FOO.BAR.0`, `FOO.BAR.1`, …)

### VAR

```rexx
VAR name
```

- First stage: splits the variable on newline and writes each piece as a record
- Not first: joins records with newline and stores the result in the variable

### VARSET

```rexx
VARSET name [delimitedString]
```

Sets the named variable to the delimited string value (empty if omitted). Copies primary input records through (filter stage).

### VARFETCH

```rexx
VARFETCH name
```

Must be first stage. Writes the value of the named variable as a single record (newlines stay inside that record).

### REXXVARS

```rexx
REXXVARS
```

Must be first stage. Writes one record per variable in the REXX pool, in the form `name value`. This is a snapshot of variables visible at the time the stage runs.

### VARLOAD

```rexx
VARLOAD
```

Reads records in the form `name value` (variable name, whitespace, value) and sets each variable. Copies records through when an output is connected; still sets variables when it is last. Records with only a name set the variable to empty. Blank records are ignored.

### VARDROP

```rexx
VARDROP [name...]
```

Drops each operand name, then drops the variable named by the first word of each input record. A name ending in `.` drops that stem. Copies records through when an output is connected. Dropping a name that is not set is not an error.

## Differences from CMS/OS/2

1. **Must run in-process**: The external `pipe` command cannot access REXX variables. Name the `.rexx` program as a stage and use CALLPIPE inside it.

2. **Regina only**: These stages require Regina REXX. The subset interpreter does not support variable pool access.

3. **No CLIST pool**: Only REXX variable pools are supported (CLIST is MVS-specific).

## Example Workflow

See `examples/photo_workflow.rexx` for a complete working example that:

- Creates sample photo-name files
- Loads them into REXX stems
- Runs LOOKUP with labeled streams inside CALLPIPE
- Stores leftover names in a second stem
- Prints the command that would process each leftover

Run it with:

```text
pipe "examples/photo_workflow.rexx | console"
```
