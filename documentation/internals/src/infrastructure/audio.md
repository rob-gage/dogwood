# Audio boundary

`engine_audio` is present in the workspace and re-exported by
`dogwood_engine::audio`, but it is not currently part of the runtime update or
render pipeline. Its current source contains form-specific acoustics marker
types rather than an active device, mixer, asset, or scene-audio ownership
system. Treat it as an extension boundary, not as an implemented audio
subsystem.

This distinction matters for architecture work: adding audio behavior will
need a clear owner for device resources, frame/update scheduling, and game
lifetime before it can be described as a runtime stage. No existing simulation
stage depends on it.

### Relevant implementation

- `engine_audio/src/lib.rs` — crate module boundary and current exports.
- `engine_audio/src/material_acoustics_cellular_static.rs` — static-cell
  acoustics marker.
- `engine_audio/src/material_acoustics_cellular_dynamic.rs` — dynamic-cell
  acoustics marker.
- `engine_audio/src/material_acoustics_fluid.rs` — fluid acoustics marker.
- `engine_audio/src/material_acoustics_gas.rs` — gas acoustics marker.
- `engine/src/lib.rs` — facade re-export under `audio`.
