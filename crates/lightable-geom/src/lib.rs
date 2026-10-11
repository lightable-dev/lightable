//! Pure 2D geometry for CAD: polyline bulges and arcs ([`bulge`]), points
//! along ellipses and NURBS curves ([`curve`]), object coordinate systems
//! and affine transforms ([`transform`]), and even-odd region filling
//! ([`fill`]).
//!
//! Every function is pure and unit-tested; the Lightable drafting app uses
//! the same functions for its commands and its rendering. Coordinates are
//! `f64` throughout.
//!
//! ```
//! use lightable_geom::bulge::arc_from_bulge;
//!
//! // A bulge of 1 is a half circle from start to end.
//! let arc = arc_from_bulge([0.0, 0.0], [2.0, 0.0], 1.0).unwrap();
//! assert!((arc.radius - 1.0).abs() < 1e-12);
//! assert_eq!(arc.center, [1.0, 0.0]);
//! ```

#![warn(missing_docs)]

pub mod bulge;
pub mod curve;
pub mod fill;
pub mod transform;

// The README's examples are compiled and run as doctests.
#[doc = include_str!("../README.md")]
#[cfg(doctest)]
pub struct ReadmeDoctests;
