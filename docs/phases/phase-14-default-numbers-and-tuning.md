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

## Checklist

- [ ] Follow the sequence unless a hard dependency blocks progress.
- [ ] Document any ordering deviations and reasons.
- [ ] Verify each stage before moving to next.
- [ ] Keep cold path working before warm-path optimization.
- [ ] Keep budget controls as explicit final hardening stage.

## References

- [All prior phase docs](./README.md)
