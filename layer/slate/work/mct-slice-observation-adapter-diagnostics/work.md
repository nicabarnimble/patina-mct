# Add adapter diagnostic observation helpers

## Story
Add shared observation constructors for adapter diagnostics so common runtime and transport failures are canonical `MctObservation` facts rather than local log strings.

## Why
Existing Mother has practical adapter diagnostics spread through runtime code. MCT should improve by making the diagnostic shape explicit and reusable: adapters perform effects, then report structured facts. The kernel domain owns the observation shape, not the adapter implementation.

## Direction
- One explicit input record.
- Small constructors for common failure classes.
- No exporter/logging abstraction.
- No adapter-specific authority logic.
