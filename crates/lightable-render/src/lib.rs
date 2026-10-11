//! Rendering without a UI: a [`Drawing`](lightable_dxf::Drawing) becomes a
//! [`Scene`] of styled 2D primitives in world coordinates ([`build`]), which
//! can be written as SVG ([`svg::to_svg`]) or turned into chunked vertex data
//! for a GPU ([`tess::tessellate`]). [`View`] is the pan/zoom maths.
//!
//! Everything here is pure and deterministic, so the app, the SVG snapshots
//! and the visual diff (M4b) share one implementation.

mod build;
mod scene;
pub mod svg;
pub mod tess;
mod view;

pub use build::{build, build_block, build_with};
pub use scene::*;
pub use view::View;
