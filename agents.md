# Agent Instructions for This Repository

## Mission

Build and maintain a highly performant, visually high-quality static planet runtime in Godot + Rust while keeping repository documentation continuously aligned with actual code behavior.

## Non-Negotiable Rules

1. Always consult documentation first.
2. Always check official Godot documentation for engine APIs and constraints before coding.
3. Always check godot-rust documentation for binding behavior, FFI details, and API semantics before coding.
4. Do not rely on undocumented behavior. If a behavior is not documented, be sure you really really want to use it.
5. Keep local docs in sync with implementation at all times.

## Documentation Sources to Prioritize

1. Godot Engine official docs (stable when possible; note when latest is used).
2. godot-rust official book/API docs for gdext behavior.
3. Project-local phase and architecture docs in this repository.

## Required Workflow

### Before Coding

1. Identify feature scope and impacted subsystems.
2. Read relevant Godot and godot-rust docs.
3. Capture key constraints and assumptions from docs.

### During Coding

1. Implement to documented API contracts.
2. Avoid undocumented shortcuts, especially around memory ownership and buffer transfer assumptions.
3. Add or update local implementation notes when design decisions are made.

### After Coding

1. Update local docs in the same change set as code.
2. Update the relevant phase checklist and test steps to reflect what was actually implemented.
3. Record deviations from original plan and why they were needed.
4. Record what was tested and test outcomes.

## Definition of Done for Any Feature

1. Code compiles and feature behavior matches expectations.
2. Tests or validation steps were executed and recorded.
3. Local docs now describe current reality, not intended future behavior.
4. Referenced Godot and godot-rust documentation was checked for the implemented API usage.

## Enforcement Guidance

1. If docs are missing or ambiguous, pause and flag uncertainty explicitly.
2. Prefer conservative, documented behavior over speculative optimizations.
3. If implementation changes architecture contracts, update the phase docs and architecture notes immediately.
