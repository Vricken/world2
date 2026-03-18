# Phase 07 - Mesh Generation Pipeline

## Goal

Restore complete stage-by-stage pipeline detail (A through G), including Rust packing, Godot staging ownership, and warm-path readiness.

## Stage A: Build Metadata Tree

At load:

```text
for each face:
    for lod in 0..=MAX_LOD:
        for each chunk on that face/lod:
            - compute same-lod neighbors
            - compute bounds skeleton
            - compute angular radius / horizon metadata
            - compute surface class
            - allocate metadata slot
```

Metadata precompute covers all LODs. Payload precompute does not. Default policy is `PAYLOAD_PRECOMPUTE_MAX_LOD = 5`; higher LOD payloads are generated on demand and bounded by runtime residency budgets.

## Stage B: Sample Scalar Fields

For each resident chunk:

```text
for each sample in (QUADS_PER_EDGE + 3)^2:
    1. map sample to face-space (with border ring)
    2. cube -> sphere projection
    3. evaluate height from 3D noise
    4. evaluate biome masks / moisture / temperature / rockness
    5. store height + masks in chunk-local sample grid
```

Sample POD buffer:

```rust
pub struct Sample {
    pub unit_dir: DVec3,
    pub height: f32,
    pub biome0: f32,
    pub biome1: f32,
    pub slope_hint: f32,
}
```

## Stage C: Derive Mesh Buffers

From sample grid:

1. Build visible positions.
2. Compute normals from sampled global field.
3. Compute tangents if material requires.
4. Compute color/mask channels.
5. Select stitch index buffer.
6. Emit `CpuMeshBuffers`.

Normals should come from the sampled global field, not only local triangle winding. Border ring data is what keeps neighboring chunks consistent.

## Stage D: Optional Rust Byte Packing

```text
1. read chunk surface class / format mask
2. determine vertex / attribute / index strides
3. pack positions into vertex region
4. pack normals/tangents/uvs/colors into attribute region
5. pack indices into index region
6. store PackedMeshRegions beside CpuMeshBuffers
```

Do not assume one arbitrary interleaved blob format. Godot exposes separate region updates and stride helpers; packing must match the actual surface format class.

## Stage E: Fill Reusable Godot Staging Buffers

```text
1. fetch compatible pooled render entry or class staging template
2. ensure PackedByteArray buffers resized to required byte counts
3. fill vertex_region in place via as_mut_slice()
4. fill attribute_region in place via as_mut_slice()
5. fill index_region in place via as_mut_slice()
6. avoid fresh PackedByteArray construction from transient Vec<u8>
```

This is the key FFI-focused stage. Hot path should mutate reusable Godot-owned buffers in place.

## Stage F: Build Commit Payloads

Package:

1. `CpuMeshBuffers`
2. optional `PackedMeshRegions`
3. optional staging handles (when warm/update compatible)
4. collision payload (if physics-eligible)
5. asset transforms
6. final render-relative transform
7. RID lifecycle command

Workers emit Rust payloads and desired server transitions, not scene nodes.

## Stage G: Create/Update Server Objects

```text
1. try compatible pooled render reuse first
2. on reuse:
   - update vertex region
   - update attribute region
   - update index region if needed
   - refresh base/scenario/transform/material state
3. on no reuse:
   - create mesh resource
   - create render instance RID
   - attach render instance to scenario
4. create/update physics shape/body if needed
5. create/update asset instances
6. pool inactive state when possible; free only by policy/watermark
```

Steady-state should favor reuse, region updates, reusable staging, and reusable worker scratch memory.

## Checklist

- [ ] Implement all seven stages with explicit boundaries.
- [ ] Keep packing aligned to real class format/stride rules.
- [ ] Use in-place staging fills for warm path.
- [ ] Keep workers node-free and payload-driven.
- [ ] Route incompatibilities to fallback with metrics.
- [ ] Verify stage counts and byte-size integrity in tests.
- [ ] Keep metadata-all-LODs and payload-window policy explicit in runtime config.

## Prerequisites

- [ ] Phase 06 active-set selection and metadata prerequisites completed.

## Ordered Build Steps

1. [ ] Implement Stage A metadata tree build.
2. [ ] Implement Stage B scalar field sampling with border ring.
3. [ ] Implement Stage C CPU mesh derivation.
4. [ ] Implement Stage D optional Rust region packing.
5. [ ] Implement Stage E reusable Godot staging fill path.
6. [ ] Implement Stage F commit payload assembly.
7. [ ] Implement Stage G create/update server transition handling.

## Validation and Test Gates

- [ ] Stage-by-stage counters match expected chunk counts.
- [ ] Packed byte lengths match class stride expectations.
- [ ] Warm staging arrays are reused across repeated updates.
- [ ] Compatibility mismatch correctly falls back to cold path.

## Definition of Done

- [ ] End-to-end A-G pipeline is deterministic.
- [ ] Hot path does not rely on transient staging conversion churn.
- [ ] Worker output remains data/lifecycle-command based.

## Test Record (Fill In)

- [ ] Date:
- [ ] Result summary:
- [ ] Staging reuse notes:
- [ ] Follow-up actions:

## References

- [RenderingServer - Godot docs (stable)](https://docs.godotengine.org/en/stable/classes/class_renderingserver.html)
- [godot-rust built-in types (packed arrays)](https://godot-rust.github.io/book/godot-api/builtins.html)
