# REXX Variable Pool Design

This document describes how the REXX variable pool stages (STEM, VAR, VARSET, VARFETCH, REXXVARS, VARLOAD) work in Portable Pipelines.

## The Core Challenge

On IBM CMS and OS/2, a REXX program can invoke the external PIPE command, and pipeline stages like STEM and VAR can access the calling REXX program's variable pool. This works because of the host operating system's architecture (CMS program stacks, OS/2 REXX variable pool API).

On Windows, macOS, and Linux, an external process (the `pipe` executable) cannot access another process's memory, so it cannot reach the calling REXX program's variables.

## The Solution

Variable pool stages work when pipelines run **in-process** with the REXX interpreter. This happens when:

1. A REXX program uses **ADDRESS PIPELINE** to run a pipeline specification
2. The pipeline runs through the Regina bridge (`pipe-rexx`)
3. Regina is loaded and available on the system

When these conditions are met, the pipeline stages share the same Regina interpreter instance as the calling REXX program, so they can access its variable pool via Regina's RexxVariablePool API.

## Usage Pattern

Instead of calling the external `pipe` command:

```rexx
/* This CANNOT access REXX variables (external process) */
'pipe literal hello | stem output RESULT.'
```

Use ADDRESS PIPELINE from within the REXX program:

```rexx
/* This CAN access REXX variables (in-process) */
ADDRESS PIPELINE 'literal hello | stem output RESULT.'
```

Or use CALLPIPE from within a pipeline-aware REXX stage:

```rexx
'CALLPIPE literal hello | stem output RESULT.'
```

## Implementation Details

The variable pool stages use a hook mechanism to avoid circular dependencies:

1. `pipe-stages` defines the stage logic and declares a `VarPoolHook` type
2. `pipe-rexx` implements the actual Regina variable pool access in `varpool.rs`
3. When `pipe-rexx::install_hook()` is called, it registers the varpool hook with `pipe-stages`
4. Variable pool stages call through the hook to access Regina

This design allows the stages to be part of the standard registry while keeping the Regina dependency optional (when Regina is not available, the stages return a clear error message).

## Stages

All variable pool stages require Regina and must be called from within a REXX program via ADDRESS PIPELINE.

### STEM

```rexx
STEM [INPUT] name     /* Default: read records, populate stem */
STEM OUTPUT name      /* Read stem, write records */
```

- INPUT: Reads records and stores them in stem.1, stem.2, etc., sets stem.0 to the count
- OUTPUT: Reads stem.0 for the count, then writes stem.1, stem.2, etc. as records
- The stem name is normalized (trailing dot is optional: `photos.` = `photos`)

### VAR

```rexx
VAR [INPUT] name      /* Default: read records, store in variable */
VAR OUTPUT name       /* Read variable, write as records */
```

- INPUT: Reads all records and stores them in the variable (newline-separated)
- OUTPUT: Splits the variable value on newlines and writes as records

### VARSET

```rexx
VARSET name [delimitedString]
```

Sets the named variable to the delimited string value (empty if omitted). Copies primary input records through (filter stage).

### VARFETCH

```rexx
VARFETCH name
```

Must be first stage. Writes the value of the named variable as a single record.

### REXXVARS

```rexx
REXXVARS
```

Must be first stage. Writes one record per variable in the REXX pool, in the form `name value`. This is a snapshot of variables visible at the time the stage runs.

### VARLOAD

```rexx
VARLOAD
```

Reads records in the form `name value` (variable name, whitespace, value) and sets each variable. Copies records through (filter stage). Records with only a name set the variable to empty.

## Differences from CMS/OS/2

1. **Must use ADDRESS PIPELINE**: The external `pipe` command cannot access REXX variables. You must use ADDRESS PIPELINE from within the REXX program.

2. **Regina only**: These stages require Regina REXX. The subset interpreter does not support variable pool access.

3. **No VARDROP yet**: The VARDROP stage is not implemented.

4. **No CLIST pool**: Only REXX variable pools are supported (CLIST is MVS-specific).

## Example Workflow

See `examples/photo_workflow.rexx` for a complete working example that demonstrates:

- Loading a file into a REXX stem
- Feeding the stem back into a pipeline
- Using LOOKUP within the pipeline
- Storing results in REXX variables
- Using all six variable pool stages

Run it with:

```bash
regina examples/photo_workflow.rexx
```
