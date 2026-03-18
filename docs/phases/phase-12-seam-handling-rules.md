# Phase 12 - Asset Placement

## Goal

Restore deterministic chunk-local asset placement rules and streaming ownership behavior with full narrative detail.

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

## References

- [RenderingServer - Godot docs (stable)](https://docs.godotengine.org/en/stable/classes/class_renderingserver.html)
