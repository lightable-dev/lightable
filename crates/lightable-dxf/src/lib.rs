//! Reads and writes ASCII DXF drawings (R12 to 2018) as a plain Rust model.
//!
//! - Read with [`read_file`], [`read_bytes`] or [`read_str`]; the result is
//!   a [`Drawing`] (header, tables, blocks, entities) and a list of
//!   [`Warning`]s. Problems that affect only part of a file are warnings,
//!   not errors, and input never causes a panic.
//! - Anything the library does not understand is kept verbatim (as
//!   [`Unknown`] entities, raw tables and raw sections), so
//!   [`Drawing::save_bytes`] writes an unchanged drawing back byte for byte.
//! - [`Drawing::to_bytes`] and [`Drawing::write_file`] write R12 or R2018,
//!   reporting anything the target version cannot hold.
//! - [`hatch`] reads HATCH boundaries from their raw data.
//!
//! ```
//! let text = "0\nSECTION\n2\nENTITIES\n0\nLINE\n8\nWALLS\n10\n0\n20\n0\n11\n5\n21\n0\n0\nENDSEC\n0\nEOF\n";
//! let (drawing, warnings) = lightable_dxf::read_str(text)?;
//! assert!(warnings.is_empty());
//! let walls = drawing.entities.iter().filter(|e| e.layer == "WALLS").count();
//! assert_eq!(walls, 1);
//! # Ok::<(), lightable_dxf::DxfError>(())
//! ```

#![warn(missing_docs)]

mod encoding;
mod entity;
mod error;
pub mod hatch;
mod model;
mod reader;
mod table;
pub mod tokenizer;
pub mod value;
mod writer;

pub use encoding::{CodePage, FileEncoding, decode_mtext, decode_text};
pub use entity::{
    Arc, AttDef, Attrib, Circle, Dimension, DimensionKind, Ellipse, Entity, EntityKind, Insert,
    Line, LwPolyline, LwVertex, MText, Point, Polyline, Spline, Text, Unknown, Vertex,
    mtext_plain_lines,
};
pub use error::{DxfError, Warning};
pub use model::{
    Block, Color, Drawing, Handle, Header, HeaderVariable, LineWeight, Point3, RawSection, Source,
    StrayRecord, Version, Z_AXIS,
};
pub use reader::{read_bytes, read_file, read_str};
pub use table::{
    BlockRecord, DimStyle, Layer, Linetype, LinetypeElement, Named, RawTable, Table, Tables,
    TextStyle, names_equal,
};
pub use tokenizer::Pair;
pub use value::Value;

// The README's examples are compiled and run as doctests.
#[doc = include_str!("../README.md")]
#[cfg(doctest)]
pub struct ReadmeDoctests;
