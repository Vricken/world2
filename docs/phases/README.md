# Planet Runtime Implementation Phases

This folder is a full checklist split from the master plan. Each phase is designed to be completed and tested before moving to the next phase.

## Project Context

We are building a highly performant, high-fidelity static planet runtime using Godot and Rust (godot-rust).

The target outcome is:

- A visually strong planet that holds up from orbit to near-surface views.
- A server-driven chunk architecture that avoids scene-tree bottlenecks.
- Stable, deterministic streaming with horizon culling, frustum culling, and chunked LOD.
- Warm-path render updates through reusable pooled resources and reusable Godot-owned packed staging buffers.
- A collision and asset system that stays gameplay-correct while remaining budget-aware.

These phase documents are intended to keep implementation, validation, and architecture decisions aligned with production goals.

## Implementation Reality This Plan Enforces

This plan is implementation-oriented, not aspirational. Each phase should be considered incomplete until code behavior, local docs, and test notes all match.

Additional runtime constraints that must remain explicit in every phase:

- The render hot path must be built around reusable Godot-owned packed staging buffers.
- Rust to Godot buffer transfer must be treated as copy-possible unless docs explicitly guarantee otherwise.
- Do not rely on undocumented zero-copy adoption of arbitrary Rust-owned allocations.
- Reuse per-worker Rust scratch memory for mesh and packing work to avoid repeated large allocations.
- Keep visibility and budget controls (horizon culling, active-set diffs, commit budgets, upload budgets) as first-class systems.

## Primary Technical Direction

- Use cube-sphere terrain with 6 face quadtrees.
- Keep chunk identity and lifecycle in Rust data, not in per-chunk nodes.
- Drive rendering through `RenderingServer` and collision through `PhysicsServer3D`.
- Separate cold-create and warm-reuse paths.
- Treat Rust to Godot buffer transfer as copy-possible unless explicitly documented otherwise.
- Keep local docs synchronized with code reality as features are implemented.

## Documentation and Validation Discipline

Every phase document should include:

- The Godot and godot-rust references that must be verified before implementation.
- A checklist for recording what was tested and what the result was.
- Any deviation notes when implementation differs from the original plan.

If implementation changes architecture contracts, update the affected phase docs in the same change set.

## How To Use These Phase Docs

These files are intentionally checklist-driven, but they are not meant to be checklist-only. Treat each phase file as a working engineering brief:

1. Read the narrative detail first to understand why the phase exists and what constraints matter.
2. Use the checklist to track completion and prevent missed requirements.
3. Fill the test record with what actually happened, including deviations and tradeoffs.

The expected implementation style is conservative and explicit around Godot server APIs and the Rust to Godot FFI boundary.

For steady-state streaming performance, the runtime should assume:

- visibility and active-set control are the biggest performance levers,
- warm-path reuse is preferred over create and free churn,
- reusable Godot-owned staging arrays are preferred over transient conversion objects,
- reusable Rust scratch buffers are preferred over per-job large allocations.

When in doubt, choose behavior explicitly documented by Godot and godot-rust, then measure.

## Phase Files

- [x] [Phase 01 - Recommended Project Shape](phase-01-project-setup.md)
- [x] [Phase 02 - Data Model](phase-02-planet-math-foundations.md)
- [x] [Phase 03 - Face Basis and Chunk-Local Coordinates](phase-03-chunk-keys-and-neighbor-mapping.md)
- [x] [Phase 04 - Chunk Keys and Deterministic Neighbor Mapping Across Faces](phase-04-topology-seams-mesh-generation.md)
- [x] [Phase 05 - Visible Grid, Border Ring, and Stitch Variants](phase-05-metadata-visibility-lod-selection.md)
- [x] [Phase 06 - Visibility Selection and LOD](phase-06-cold-render-commit-path.md)
- [x] [Phase 07 - Mesh Generation Pipeline](phase-07-warm-pools-and-ffi-staging-path.md)
- [x] [Phase 08 - Server-Side Render and Collision Commit Pattern](phase-08-physics-and-asset-streaming.md)
- [x] [Phase 09 - Threading Model in godot-rust](phase-09-threading-precision-final-hardening.md)
- [x] [Phase 10 - Precision Strategy](phase-10-threading-model-godot-rust.md)
- [x] [Phase 11 - Seam Handling Rules](phase-11-precision-and-origin-strategy.md)
- [x] [Phase 12 - Asset Placement](phase-12-seam-handling-rules.md)
- [x] [Phase 13 - Default Numbers I Would Start With](phase-13-asset-placement.md)
- [x] [Phase 14 - Build Order](phase-14-default-numbers-and-tuning.md)
- [ ] [Phase 15 - One Important Refinement](phase-15-build-order-and-strategy-refinement.md)

## Big Plan Coverage Map

This is the authoritative one-to-one mapping from `bigplan.md` sections to phase files:

1. Recommended Project Shape -> Phase 01
2. Data Model -> Phase 02
3. Face Basis and Chunk-Local Coordinates -> Phase 03
4. Chunk Keys and Deterministic Neighbor Mapping Across Faces -> Phase 04
5. Visible Grid, Border Ring, and Stitch Variants -> Phase 05
6. Visibility Selection and LOD -> Phase 06
7. Mesh Generation Pipeline -> Phase 07
8. Server-Side Render and Collision Commit Pattern -> Phase 08
9. Threading Model in godot-rust -> Phase 09
10. Precision Strategy -> Phase 10
11. Seam Handling Rules -> Phase 11
12. Asset Placement -> Phase 12
13. Default Numbers I Would Start With -> Phase 13
14. Build Order -> Phase 14
15. One Important Refinement -> Phase 15

## Continuity Contract Across Author Groups

Phases 11-15 must continue naturally from phases 01-10 by preserving these dependencies:

- Phase 11 hardens seam behavior using Phase 03 coordinate rules, Phase 05 stitch topology, and Phase 08 warm-path compatibility checks.
- Phase 12 adds deterministic asset residency on top of Phase 06 active-set selection and Phase 08 server ownership.
- Phase 13 sets initial runtime controls for budgets, pool watermarks, and tuning boundaries defined by phases 06-12.
- Phase 14 records the end-to-end dependency order and the handoff from completed to pending work.
- Phase 15 formalizes strategy seams without breaking ownership and FFI contracts already established in phases 01-14.

Any future edits to phases 11-15 that break these continuity rules must be documented as explicit deviations.

### Filename Mapping Note

Some markdown filenames reflect earlier sequencing names. Treat the phase number and heading text inside each file as the source of truth.

- `phase-10-threading-model-godot-rust.md` contains "Phase 10 - Precision Strategy".
- `phase-11-precision-and-origin-strategy.md` contains "Phase 11 - Seam Handling Rules".
- `phase-12-seam-handling-rules.md` contains "Phase 12 - Asset Placement".
- `phase-13-asset-placement.md` contains "Phase 13 - Default Numbers I Would Start With".
- `phase-14-default-numbers-and-tuning.md` contains "Phase 14 - Build Order".

## Global Rules (Apply in Every Phase)

- [ ] Keep the scene tree minimal. Do not represent chunk identity with terrain nodes.
- [ ] Keep chunk identity in Rust (`ChunkKey`, metadata, payload state, RID state).
- [ ] Keep render state in `RenderingServer` resources and scenario-bound instance RIDs.
- [ ] Keep collision state in `PhysicsServer3D` body and shape RIDs in the physics space.
- [ ] Keep horizon culling before frustum culling and LOD selection.
- [ ] Keep cold create path separate from warm pooled region-update path.
- [ ] Keep reusable Godot-owned `PackedByteArray` staging buffers for hot-path updates.
- [ ] Do not design around undocumented zero-copy adoption of arbitrary Rust buffers.
- [ ] Reuse Rust worker scratch storage for mesh and packing work.
- [ ] Enforce max neighbor LOD delta of 1 and fine-to-coarse stitching only.
- [ ] Keep per-class compatibility checks strict for pool reuse (`format`, counts, stitch/index class, material contract).
- [ ] Keep commit and upload budgets enforced even when pool reuse is working.
- [ ] Keep docs and checklist status synchronized with code behavior after each phase.

## Completion Criteria

- [ ] All phase checklists are complete.
- [ ] All phase test sections have been executed and results recorded.
- [ ] Runtime metrics are stable under camera stress tests and long streaming sessions.
- [ ] FFI boundary assumptions in docs match the actual Rust and gdext APIs used in code.

## Documentation Discipline

- [ ] Before implementing a feature, verify behavior against current Godot and godot-rust docs.
- [ ] During implementation, record any important API constraints and assumptions.
- [ ] After implementation, update the relevant local phase or design docs in the same change.
- [ ] If code and docs diverge, prioritize bringing docs back in sync immediately.
