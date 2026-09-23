//! Object appearance.

/// An opaque colour channel triple. Transparency lives in [`Opacity`], so a
/// colour and the strength it is drawn at stay independently editable.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Rgb {
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }
}

/// Stroke width in logical units.
///
/// FR-012: 4.0 means four logical units, never four physical pixels.
#[derive(Clone, Copy, Debug, PartialEq, PartialOrd)]
pub struct Width(f64);

impl Width {
    /// Returns `None` unless the width is finite and strictly positive.
    pub fn new(value: f64) -> Option<Self> {
        (value.is_finite() && value > 0.0).then_some(Self(value))
    }

    pub fn get(self) -> f64 {
        self.0
    }
}

/// Opacity in `0.0..=1.0`.
///
/// FR-007: for a highlighter this applies to the completed stroke as a whole,
/// so that a stroke crossing itself does not darken at the overlap. That is a
/// renderer obligation (T014/T015); the document only records the value.
#[derive(Clone, Copy, Debug, PartialEq, PartialOrd)]
pub struct Opacity(f64);

impl Opacity {
    pub const OPAQUE: Self = Self(1.0);

    /// Returns `None` for a non-finite value or one outside `0.0..=1.0`.
    pub fn new(value: f64) -> Option<Self> {
        (value.is_finite() && (0.0..=1.0).contains(&value)).then_some(Self(value))
    }

    pub fn get(self) -> f64 {
        self.0
    }
}

/// How an object is painted.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Style {
    pub color: Rgb,
    pub width: Width,
    pub opacity: Opacity,
}

impl Style {
    pub const fn new(color: Rgb, width: Width, opacity: Opacity) -> Self {
        Self {
            color,
            width,
            opacity,
        }
    }
}
