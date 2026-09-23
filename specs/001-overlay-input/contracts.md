# 001: proposed Rust interface boundaries

The following is an interface sketch, not compiled application code. Adapt it to the selected compatible crates after the platform spike. Omitted rendering and service interfaces are deliberate; avoid a large premature abstraction.

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverlayMode {
    Hidden,
    Draw,
    PassThrough,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CapabilityState {
    Unknown,
    Available,
    NeedsUserAction { reason: String },
    Unavailable { reason: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct OutputId(pub u64); // Runtime-local ID; not a stable hardware identity.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TransitionId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LogicalPoint {
    pub x: f64,
    pub y: f64,
}

#[derive(Debug)]
pub enum PlatformRequest {
    ApplyMode {
        transition: TransitionId,
        output: OutputId,
        mode: OverlayMode,
    },
    EmergencyWithdraw,
}

#[derive(Debug)]
pub enum PlatformFailure {
    Unsupported { capability: String },
    PermissionDenied { capability: String },
    ShortcutConflict,
    Disconnected,
    SurfaceLost,
    InvalidData { reason: String },
}

#[derive(Debug)]
pub enum PlatformEvent {
    ModeApplied {
        transition: TransitionId,
        output: OutputId,
        mode: OverlayMode,
    },
    ModeFailed {
        transition: TransitionId,
        error: PlatformFailure,
    },
    AllOverlaysWithdrawn,
    OutputRemoved(OutputId),
}
```

The platform driver owns its native event loop and consumes requests. It emits completion/failure and normalized input events. Do not impose Send/Sync on native UI handles just to make an abstraction compile. Portal callbacks/workers use message passing to the correct thread.

A production design must also define capability records, OutputDescriptor, normalized PointerEvent/KeyboardEvent, InputCanceled, output configure/scale changes, and surface lifetime boundaries. Add those when their tests are written, not as unused placeholders.

## Invariants to test

An old TransitionId cannot override a newer effective state. PassThrough is not announced before input ownership is released. An error must not leave a supposedly Hidden surface interactive. One native pointer event becomes at most one normalized event. Emergency withdrawal does not wait on storage or capture.
