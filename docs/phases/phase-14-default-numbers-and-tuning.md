# Phase 14 - Build Order

## Goal

Restore the complete implementation order with explicit separation between correctness stages and optimization stages.

## Build Sequence

```text
1. face basis + chunk key + neighbor mapping
2. default modified / spherified cube projection
3. cube-face sample coordinates
4. 3D noise displacement on sphere
5. border ring + normal generation
6. base chunk mesh generation
7. same-LOD neighbor validation across face edges
8. stitch index buffers
9. metadata tree + bounds + angular radius + surface class
10. horizon culling
11. frustum culling
12. projected-error LOD selection
13. render/physics active-set separation
14. cold server-side render commit path
15. warm pooled render path
16. Rust byte-region packing helpers
17. reusable Godot packed staging buffers
18. in-place staging fills via resize() + as_mut_slice()
19. byte-region vertex / attribute / index updates
20. server-side physics commit path
21. chunk-group asset multimesh path
22. worker scratch reuse
23. commit budgeting / upload budgeting / pool watermarks / hysteresis / caching polish
```

This order validates hard geometry/math early, then backend correctness, then optimization layers. It cleanly separates:

- cold creation path
- warm pooled reuse path
- Rust packing path
- Godot staging-buffer path
- bounded-churn runtime controls

That separation is intentional. Correctness-first systems must exist before optimization complexity is added.

## Natural Handoff From Phases 01-10 Into 11-15

- Phases 01-10 cover build-order steps 1-20.
- Phase 11 hardens seam correctness over steps 5, 8, and 19.
- Phase 12 introduces chunk-group asset residency for step 21.
- Phase 09 threading plus worker-scratch policy handles step 22.
- Phase 13 default numbers and back-pressure controls harden step 23.
- Phase 15 adds strategy-layer refinement across all steps without changing ownership contracts.

This is the intended continuity path between the two author groups.

## Checklist

- [ ] Follow the sequence unless a hard dependency blocks progress.
- [ ] Document any ordering deviations and reasons.
- [ ] Verify each stage before moving to next.
- [ ] Keep cold path working before warm-path optimization.
- [ ] Keep budget controls as explicit final hardening stage.

## Prerequisites

- [ ] Phase 13 default-number policy completed.

## Validation and Test Gates

- [ ] Sequencing dependencies are respected in implementation plans.
- [ ] Any out-of-order work includes a documented dependency reason.
- [ ] Handoff from completed to pending steps is explicit in tracking notes.

## Definition of Done

- [ ] Build order and dependency policy are explicit and enforceable.
- [ ] The handoff from phases 01-10 to phases 11-15 is documented and unambiguous.

## References

- [All prior phase docs](./README.md)