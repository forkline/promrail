# ADR-002: Automatic Review Artifacts For Multi-Source Promotion

## Status

Accepted

## Context

`prl` already supports multi-source promotion, but the original implementation copied whole files from sources to the destination. That works for straightforward cases, but it breaks down when:

- a new component or application is introduced from one source and may contain environment-specific configuration
- the same file exists in multiple sources with different content
- version-bearing files such as `values.yaml` or `kustomization.yaml` should promote common version changes without overwriting destination-specific configuration

The expected workflow is:

1. run `prl`
2. if the change set is obviously safe, promote immediately
3. if the change set contains ambiguous or environment-sensitive additions, emit a machine-readable artifact instead of applying
4. let opencode/LLM classify the ambiguous items
5. run `prl` again and have it automatically consume the classification

The user explicitly asked to avoid a user-authored plan file and preferred automatic artifact consumption on the second run.

## Decision

Promrail stores internal review artifacts under `.promrail/review/` for multi-source promotions.

Each artifact is keyed by the source/destination route and includes:

- route metadata (`sources`, `dest`, `filters`)
- a fingerprint of the current source and destination content
- grouped review items for new components and conflicting non-version files
- per-item decisions (`promote` or `skip`) and an optional `selected_source`
- lifecycle status (`pending`, `classified`, `applied`)

### Promotion behavior

For multi-source promotion, `prl` now behaves as follows:

1. analyze the candidate change set before applying
2. auto-copy files that are safe to promote directly
3. preserve known version-managed files for structured version application instead of whole-file copying
4. stop and write a review artifact when it detects:
   - new components missing from the destination
   - conflicting non-version files across sources
5. on a later run, automatically consume the artifact when:
   - the artifact status is `classified`
   - the artifact fingerprint still matches the live repo state
   - the artifact item set still matches the current analysis
6. after a successful classified promotion, mark the artifact as `applied`

### Structured version merge

For existing components, known version-managed files are not copied directly when a destination file already exists. Instead, `prl`:

1. extracts versions from all sources
2. merges them using existing promotion rules
3. applies the merged versions to the destination using the structured version updater

This preserves destination-specific configuration while still promoting common version changes.

## Consequences

### Positive

- `prl` remains the primary entrypoint; the workflow stays `prl -> opencode -> prl`
- the LLM step is only required when new components or conflicting non-version files are detected
- version-only promotions can complete without review
- artifacts are machine-readable, diffable, and tied to the exact repo state through fingerprints
- automatic second-run consumption keeps the workflow low-friction

### Negative

- generic YAML three-way merge is still not implemented for arbitrary configuration keys
- structured preservation currently focuses on version-managed files and component/file selection, not arbitrary field-level env preservation
- stale artifacts must be regenerated when the underlying repo state changes

## Alternatives Considered

### User-authored plan file

Rejected because it adds too much manual workflow and was explicitly disliked.

### Inline LLM calls from `prl`

Rejected for the initial implementation because it would add provider, auth, networking, and retry concerns to the CLI. The chosen design keeps `prl` deterministic and lets opencode own the classification step.

### Generic YAML path-level merge for all config

Deferred. This is the long-term direction, but it is significantly more complex than file-level review plus structured version application.

## Implementation Notes

- artifacts live at `.promrail/review/<route-key>.yaml`
- multi-source snapshots record the review artifact path when an artifact-backed promotion is applied
- review items are grouped by component to keep the artifact readable for LLM-assisted editing
- if the artifact no longer matches the current fingerprint, `prl` overwrites it with a fresh `pending` artifact and requires a new review pass
