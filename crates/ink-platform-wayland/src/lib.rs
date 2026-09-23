//! Wayland adapter.
//!
//! T002 scope: connect to the compositor, list the advertised globals, and read
//! the outputs. That is all. No surface is created, so this crate cannot and
//! does not report overlay, pass-through, or input capabilities as available;
//! proving those is T006 (layer-shell) and T007 (GNOME).
//!
//! The distinction matters: an advertised protocol is evidence that a route is
//! worth trying, not evidence that the route works (constitution §4).

#![forbid(unsafe_code)]

use ink_platform::{
    Capability, CapabilityFinding, CapabilityReport, CapabilityState, OutputInfo, PlatformError,
};
use wayland_client::globals::{GlobalListContents, registry_queue_init};
use wayland_client::protocol::{wl_output, wl_registry};
use wayland_client::{Connection, Dispatch, Proxy, QueueHandle, WEnum};

/// The highest `wl_output` version this probe understands. Version 4 is the one
/// that added the `name` and `description` events, which is why it is the
/// ceiling: binding higher would gain nothing here.
const WL_OUTPUT_MAX_VERSION: u32 = 4;

/// One protocol global as the compositor advertised it.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct AdvertisedGlobal {
    pub interface: String,
    pub version: u32,
}

/// What one probe run observed.
#[derive(Clone, Debug)]
pub struct WaylandProbe {
    /// Every advertised global, sorted by interface name.
    pub globals: Vec<AdvertisedGlobal>,
    /// One entry per `wl_output`, in the order the registry advertised them.
    pub outputs: Vec<OutputInfo>,
}

impl WaylandProbe {
    /// Whether an interface was advertised, and at which version.
    pub fn version_of(&self, interface: &str) -> Option<u32> {
        self.globals
            .iter()
            .find(|g| g.interface == interface)
            .map(|g| g.version)
    }

    /// Whether the wlroots layer-shell protocol is advertised.
    ///
    /// True here means "worth attempting in T006", never "overlay works".
    pub fn advertises_layer_shell(&self) -> bool {
        self.version_of("zwlr_layer_shell_v1").is_some()
    }
}

#[derive(Default)]
struct ProbeState {
    outputs: Vec<OutputInfo>,
}

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for ProbeState {
    fn event(
        _state: &mut Self,
        _registry: &wl_registry::WlRegistry,
        _event: wl_registry::Event,
        _data: &GlobalListContents,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        // A probe takes one snapshot. Hotplug during the run is T027's problem.
    }
}

impl Dispatch<wl_output::WlOutput, usize> for ProbeState {
    fn event(
        state: &mut Self,
        _output: &wl_output::WlOutput,
        event: wl_output::Event,
        index: &usize,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        let Some(info) = state.outputs.get_mut(*index) else {
            return;
        };
        match event {
            wl_output::Event::Geometry { transform, .. } => {
                info.transform = Some(match transform {
                    WEnum::Value(t) => format!("{t:?}"),
                    WEnum::Unknown(raw) => format!("unknown({raw})"),
                });
            }
            wl_output::Event::Mode {
                flags,
                width,
                height,
                refresh,
            } => {
                let is_current =
                    matches!(flags, WEnum::Value(f) if f.contains(wl_output::Mode::Current));
                if is_current {
                    info.resolution = Some((width, height));
                    info.refresh_mhz = Some(refresh);
                }
            }
            wl_output::Event::Scale { factor } => info.scale = Some(factor),
            wl_output::Event::Name { name } => info.name = Some(name),
            wl_output::Event::Description { description } => info.description = Some(description),
            _ => {}
        }
    }
}

/// Connects to the compositor named by the environment and reads the registry.
///
/// Returns [`PlatformError::Disconnected`] when no compositor answers; the
/// caller decides what that means for the session, because this crate must not
/// claim to know about X11.
pub fn probe() -> Result<WaylandProbe, PlatformError> {
    let conn = Connection::connect_to_env().map_err(|e| {
        PlatformError::disconnected(format!("could not connect to the compositor: {e}"))
    })?;

    let (globals, mut queue) = registry_queue_init::<ProbeState>(&conn)
        .map_err(|e| PlatformError::disconnected(format!("the registry could not be read: {e}")))?;

    let mut advertised: Vec<AdvertisedGlobal> = globals.contents().with_list(|list| {
        list.iter()
            .map(|g| AdvertisedGlobal {
                interface: g.interface.clone(),
                version: g.version,
            })
            .collect()
    });
    advertised.sort();

    // Bind every advertised wl_output so its geometry events arrive. The bound
    // proxies are held until the roundtrips finish.
    let output_names: Vec<(u32, u32)> = globals.contents().with_list(|list| {
        list.iter()
            .filter(|g| g.interface == wl_output::WlOutput::interface().name)
            .map(|g| (g.name, g.version))
            .collect()
    });

    let mut state = ProbeState::default();
    let qh = queue.handle();
    let mut bound = Vec::with_capacity(output_names.len());
    for (index, (name, version)) in output_names.iter().enumerate() {
        state.outputs.push(OutputInfo::default());
        let output: wl_output::WlOutput =
            globals
                .registry()
                .bind(*name, (*version).min(WL_OUTPUT_MAX_VERSION), &qh, index);
        bound.push(output);
    }

    // Two roundtrips: the first delivers the bind requests, the second collects
    // the burst of geometry/mode/scale/name events they trigger.
    for _ in 0..2 {
        queue.roundtrip(&mut state).map_err(|e| {
            PlatformError::disconnected(format!("the compositor stopped responding: {e}"))
        })?;
    }

    for output in bound {
        output.release();
    }

    Ok(WaylandProbe {
        globals: advertised,
        outputs: state.outputs,
    })
}

/// Turns protocol observations into capability findings.
///
/// Almost everything stays `Unknown`, with the reason naming the task that will
/// settle it. That is the honest state: this probe created no surface, so it
/// observed no overlay, no input region, and no focus behaviour.
pub fn capabilities(probe: &WaylandProbe) -> CapabilityReport {
    let mut report = CapabilityReport::new();
    let mut record = |capability, state| {
        report.record(CapabilityFinding::new(capability, state, "wayland"));
    };

    record(
        Capability::OutputEnumeration,
        if probe.outputs.is_empty() {
            CapabilityState::unavailable("the compositor advertised no wl_output")
        } else {
            CapabilityState::available(format!(
                "wl_output enumerated {} output(s) over the protocol",
                probe.outputs.len()
            ))
        },
    );

    let overlay_reason = match probe.version_of("zwlr_layer_shell_v1") {
        Some(version) => format!(
            "zwlr_layer_shell_v1 v{version} is advertised, but no surface has been created; \
             T006 must demonstrate stacking and drawing before this can be called available"
        ),
        None => "zwlr_layer_shell_v1 is not advertised; the standalone xdg-shell route is \
                 unproven and is the subject of T007 and ADR-002"
            .to_owned(),
    };
    record(
        Capability::LiveOverlay,
        CapabilityState::unknown(overlay_reason.clone()),
    );
    record(
        Capability::DrawPointerCapture,
        CapabilityState::unknown(format!(
            "no input region has been set on any surface. {overlay_reason}"
        )),
    );
    record(
        Capability::VisiblePassthrough,
        CapabilityState::unknown(format!(
            "an empty input region must be observed on a live surface. {overlay_reason}"
        )),
    );
    record(
        Capability::KeyboardRelease,
        CapabilityState::unknown(
            "keyboard interactivity is a layer-surface property; no surface exists yet (T006)",
        ),
    );
    record(
        Capability::GlobalShortcut,
        CapabilityState::unknown(
            "the org.freedesktop.portal.GlobalShortcuts interface was not queried: no D-Bus \
             client dependency has been chosen yet (T012)",
        ),
    );
    record(
        Capability::SimultaneousOutputs,
        CapabilityState::unknown("a V1 concern (FR-021); no surface exists on any output yet"),
    );
    record(
        Capability::FullscreenOverlay,
        CapabilityState::unknown("requires a native run against a fullscreen application (T006)"),
    );
    record(
        Capability::WorkspaceOverlay,
        CapabilityState::unknown("requires a native run across workspaces (T006)"),
    );
    record(
        Capability::Capture,
        CapabilityState::unknown(
            "not probed on purpose: capture is a later feature (FR-026) and drawing must never \
             depend on it",
        ),
    );
    record(
        Capability::CaptureExclusion,
        CapabilityState::unknown("not probed: belongs with capture (FR-027)"),
    );

    report
}

#[cfg(test)]
mod tests {
    use super::*;

    fn probe_with(globals: &[(&str, u32)], outputs: usize) -> WaylandProbe {
        WaylandProbe {
            globals: globals
                .iter()
                .map(|(i, v)| AdvertisedGlobal {
                    interface: (*i).to_owned(),
                    version: *v,
                })
                .collect(),
            outputs: vec![OutputInfo::default(); outputs],
        }
    }

    #[test]
    fn advertised_layer_shell_is_still_not_a_working_overlay() {
        let report = capabilities(&probe_with(&[("zwlr_layer_shell_v1", 4)], 1));

        let state = report.state_of(Capability::LiveOverlay);
        assert!(
            !state.is_available(),
            "advertising a protocol is not evidence it works"
        );
        assert_eq!(state.label(), "unknown");
        assert!(
            state.detail().contains("T006"),
            "the reason must name what would settle it"
        );
    }

    #[test]
    fn a_session_without_layer_shell_points_at_the_gnome_decision() {
        let report = capabilities(&probe_with(&[("xdg_wm_base", 6)], 1));

        let state = report.state_of(Capability::LiveOverlay);
        assert_eq!(state.label(), "unknown");
        assert!(state.detail().contains("ADR-002"));
    }

    #[test]
    fn enumerating_outputs_is_the_one_thing_this_probe_actually_proves() {
        let report = capabilities(&probe_with(&[("wl_output", 4)], 2));

        let state = report.state_of(Capability::OutputEnumeration);
        assert!(state.is_available());
        assert!(state.detail().contains('2'));
    }

    #[test]
    fn capture_is_never_probed_by_the_drawing_path() {
        let report = capabilities(&probe_with(&[], 1));

        assert_eq!(report.state_of(Capability::Capture).label(), "unknown");
        assert!(!report.state_of(Capability::Capture).is_available());
    }
}
