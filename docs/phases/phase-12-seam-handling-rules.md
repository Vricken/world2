# Phase 12 - Asset Placement

## Goal

Restore deterministic chunk-local asset placement rules and streaming ownership behavior with full narrative detail.

## Continuity From Phases 01-11

This phase depends on:

- deterministic chunk keys and topology from Phases 03-05
- render/physics residency selection from Phase 06
- payload assembly and commit ownership from Phases 07-08
- seam-safe terrain interpretation from Phase 11

Phase 12 adds deterministic placement and residency policy on top of those established systems.

## Deterministic Placement Pipeline

After terrain sampling per chunk:

```text
1. divide chunk into placement cells
2. hash (planet_seed, chunk_key, cell_id, family_id)
3. generate candidate point(s)
4. project candidate to terrain
5. reject by:
   - biome
   - slope
   - curvature
   - altitude
   - mask textures
   - exclusion radius
6. store accepted transforms in chunk payload
```

Keep assets attached to owning chunk identity for deterministic streaming and straightforward invalidation.

## MultiMesh Guidance

Use `MultiMesh` for repeated assets, but honor its culling trade-off: instances inside one multimesh are not culled independently.

Recommendations:

- not one multimesh for the entire planet
- one multimesh per `(chunk_group, asset_family)` or similarly compact spatial grouping
- near: optional higher-quality handling for important/interactable assets
- mid: multimesh
- far: impostor, billboard, or none

Because chunk visibility is server-driven, asset residency should follow chunk lifecycle rather than scene-node visibility callbacks.

## Pooling Priority

Asset multimesh pooling/reuse can be added later, but only after terrain chunk pooling and staging reuse path is stable.

## Checklist

- [ ] Implement deterministic placement hash inputs exactly.
- [ ] Keep all reject filters deterministic and documented.
- [ ] Attach placement ownership to chunk lifecycle state.
- [ ] Keep multimesh groups spatially compact.
- [ ] Avoid global multimesh sets for planet-wide instances.
- [ ] Add deterministic replay checks for same seed/path.

## Prerequisites

- [ ] Phase 11 seam-handling rules completed.

## Ordered Build Steps

1. [ ] Implement deterministic placement-cell hashing.
2. [ ] Implement candidate projection onto terrain and deterministic reject filters.
3. [ ] Store accepted transforms in chunk payload ownership.
4. [ ] Add chunk-group and asset-family multimesh grouping policy.
5. [ ] Bind asset residency to chunk active-set lifecycle.
6. [ ] Add deterministic replay validation for fixed seed and camera path.

## Validation and Test Gates

- [ ] Placement replay is identical for fixed seed/chunk/camera path.
- [ ] Asset residency diffing follows chunk lifecycle deterministically.
- [ ] Spatial grouping avoids one-global-multimesh anti-pattern.

## Definition of Done

- [ ] Asset placement is deterministic and chunk-owned.
- [ ] Grouping strategy is culling-aware and scalable.
- [ ] Placement behavior is reproducible under test.

## Test Record (Fill In)

- [ ] Date:
- [ ] Result summary:
- [ ] Replay scenarios tested:
- [ ] Follow-up actions:

## References

- [RenderingServer - Godot docs (stable)](https://docs.godotengine.org/en/stable/classes/class_renderingserver.html)