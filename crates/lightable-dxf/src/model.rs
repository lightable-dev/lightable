//! The in-memory drawing model.

use std::fmt;
use std::sync::Arc;

use crate::encoding::FileEncoding;
use crate::entity::Entity;
use crate::table::{Named, Table, Tables};
use crate::tokenizer::Pair;
use crate::value::{parse_f64, parse_int};

/// A point or vector in drawing units.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Point3 {
    /// X coordinate.
    pub x: f64,
    /// Y coordinate.
    pub y: f64,
    /// Z coordinate.
    pub z: f64,
}

impl Point3 {
    /// A point from its coordinates.
    pub const fn new(x: f64, y: f64, z: f64) -> Self {
        Self { x, y, z }
    }
}

/// The default extrusion direction (the world Z axis).
pub const Z_AXIS: Point3 = Point3::new(0.0, 0.0, 1.0);

/// An object handle: a hexadecimal identifier unique within the drawing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Handle(pub u64);

impl Handle {
    /// Parses a hexadecimal handle; `None` if it is not one.
    pub fn parse(raw: &str) -> Option<Self> {
        let text = raw.trim();
        if text.is_empty() || text.len() > 16 {
            return None;
        }
        u64::from_str_radix(text, 16).ok().map(Self)
    }
}

impl fmt::Display for Handle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:X}", self.0)
    }
}

/// DXF file format versions this library targets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Version {
    /// Release 12 file format (`AC1009`).
    R12,
    /// Release 13 format (`AC1012`).
    R13,
    /// Release 14 format (`AC1014`).
    R14,
    /// 2000 format (`AC1015`).
    R2000,
    /// 2004 format (`AC1018`).
    R2004,
    /// 2007 format (`AC1021`).
    R2007,
    /// 2010 format (`AC1024`).
    R2010,
    /// 2013 format (`AC1027`).
    R2013,
    /// 2018 format (`AC1032`).
    R2018,
}

impl Version {
    /// Maps a `$ACADVER` value such as `AC1032`.
    pub fn from_acadver(raw: &str) -> Option<Self> {
        Some(match raw.trim() {
            "AC1009" => Self::R12,
            "AC1012" => Self::R13,
            "AC1014" => Self::R14,
            "AC1015" => Self::R2000,
            "AC1018" => Self::R2004,
            "AC1021" => Self::R2007,
            "AC1024" => Self::R2010,
            "AC1027" => Self::R2013,
            "AC1032" => Self::R2018,
            _ => return None,
        })
    }

    /// The `$ACADVER` string for this version, e.g. `AC1032`.
    pub fn acadver(self) -> &'static str {
        match self {
            Self::R12 => "AC1009",
            Self::R13 => "AC1012",
            Self::R14 => "AC1014",
            Self::R2000 => "AC1015",
            Self::R2004 => "AC1018",
            Self::R2007 => "AC1021",
            Self::R2010 => "AC1024",
            Self::R2013 => "AC1027",
            Self::R2018 => "AC1032",
        }
    }
}

/// An entity or layer colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Color {
    /// Colour 0: take the colour of the block reference.
    ByBlock,
    /// Colour 256: take the layer's colour.
    #[default]
    ByLayer,
    /// ACI colour index, 1–255.
    Index(u8),
    /// 24-bit true colour.
    Rgb(u8, u8, u8),
}

impl Color {
    /// From a group 62 value (0 = ByBlock, 256 = ByLayer, 1–255 = index).
    pub fn from_aci(value: i64) -> Option<Self> {
        match value {
            0 => Some(Self::ByBlock),
            256 => Some(Self::ByLayer),
            1..=255 => Some(Self::Index(value as u8)),
            _ => None,
        }
    }

    /// The RGB value of an ACI index (1–255). Indexes 1–9 and 250–255 use
    /// the standard values; 10–249 follow the usual layout of 24 hues × 5
    /// shades × 2 saturations, computed rather than tabulated, so they are
    /// close to but not always exactly the colours other programs show.
    pub fn aci_rgb(index: u8) -> (u8, u8, u8) {
        const FIXED: [(u8, u8, u8); 10] = [
            (0, 0, 0),
            (255, 0, 0),
            (255, 255, 0),
            (0, 255, 0),
            (0, 255, 255),
            (0, 0, 255),
            (255, 0, 255),
            (255, 255, 255),
            (128, 128, 128),
            (192, 192, 192),
        ];
        match index {
            0..=9 => FIXED[usize::from(index)],
            250..=255 => {
                let v = [51, 91, 132, 173, 214, 255][usize::from(index - 250)];
                (v, v, v)
            }
            _ => {
                let i = f64::from(index - 10);
                let hue = (i / 10.0).floor() * 15.0;
                let shade = (i % 10.0 / 2.0).floor();
                let value = [1.0, 0.65, 0.5, 0.3, 0.15][shade as usize];
                let saturation = if index.is_multiple_of(2) { 1.0 } else { 0.5 };
                hsv(hue, saturation, value)
            }
        }
    }

    /// The ACI index (1–255) closest to an RGB colour.
    pub fn nearest_aci(r: u8, g: u8, b: u8) -> u8 {
        let distance = |i: u8| {
            let (pr, pg, pb) = Self::aci_rgb(i);
            let d = |a: u8, b: u8| (i32::from(a) - i32::from(b)).pow(2);
            d(pr, r) + d(pg, g) + d(pb, b)
        };
        (1..=255).min_by_key(|&i| distance(i)).unwrap_or(7)
    }

    /// From a group 420 value (`0x00RRGGBB`). Some writers set flag bits in
    /// the top byte, so only the low 24 bits are used.
    pub fn from_true_color(value: i64) -> Option<Self> {
        if !(i64::from(i32::MIN)..=i64::from(u32::MAX)).contains(&value) {
            return None;
        }
        let rgb = value & 0x00FF_FFFF;
        Some(Self::Rgb((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8))
    }
}

fn hsv(hue: f64, saturation: f64, value: f64) -> (u8, u8, u8) {
    let c = value * saturation;
    let h = hue / 60.0;
    let x = c * (1.0 - (h % 2.0 - 1.0).abs());
    let (r, g, b) = match h as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = value - c;
    let to = |v: f64| ((v + m) * 255.0).round() as u8;
    (to(r), to(g), to(b))
}

/// Line weight (group 370).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineWeight {
    /// Take the layer's line weight (-1).
    ByLayer,
    /// Take the block reference's line weight (-2).
    ByBlock,
    /// The program's default line weight (-3).
    Default,
    /// Hundredths of a millimetre (0–211).
    Hundredths(u16),
}

impl LineWeight {
    /// Reads a group 370 value; `None` if it is not a valid line weight.
    pub fn from_code(value: i64) -> Option<Self> {
        match value {
            -1 => Some(Self::ByLayer),
            -2 => Some(Self::ByBlock),
            -3 => Some(Self::Default),
            0..=211 => Some(Self::Hundredths(value as u16)),
            _ => None,
        }
    }
}

/// One `$NAME` header variable and its pairs, as read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeaderVariable {
    /// Variable name including the `$`, e.g. `$INSUNITS`.
    pub name: String,
    /// The variable's value pairs, as in the file.
    pub pairs: Vec<Pair>,
}

/// The `HEADER` section.
///
/// All variables are kept in file order in [`Header::variables`]; the typed
/// accessors read from there, so there is a single source of truth.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Header {
    /// Every variable, in file order.
    pub variables: Vec<HeaderVariable>,
}

impl Header {
    /// Looks up a variable by name (e.g. `"$INSUNITS"`), ignoring ASCII case.
    pub fn get(&self, name: &str) -> Option<&HeaderVariable> {
        self.variables
            .iter()
            .find(|v| v.name.eq_ignore_ascii_case(name))
    }

    fn raw(&self, name: &str, code: i16) -> Option<&str> {
        self.get(name)?
            .pairs
            .iter()
            .find(|p| p.code == code)
            .map(|p| p.raw.as_str())
    }

    fn name_value(&self, name: &str, code: i16) -> Option<&str> {
        self.raw(name, code).map(str::trim)
    }

    fn point(&self, name: &str) -> Option<Point3> {
        let coord = |code| self.raw(name, code).and_then(parse_f64);
        Some(Point3::new(
            coord(10)?,
            coord(20)?,
            coord(30).unwrap_or(0.0),
        ))
    }

    /// The raw `$ACADVER` string, e.g. `AC1032`.
    pub fn acadver(&self) -> Option<&str> {
        self.name_value("$ACADVER", 1)
    }

    /// `$ACADVER` mapped to a supported [`Version`].
    pub fn version(&self) -> Option<Version> {
        self.acadver().and_then(Version::from_acadver)
    }

    /// `$DWGCODEPAGE`, e.g. `ANSI_1252`.
    pub fn codepage(&self) -> Option<&str> {
        self.name_value("$DWGCODEPAGE", 3)
    }

    /// `$INSUNITS` (0 = unitless, 1 = inches, 4 = millimetres, 6 = metres, ...).
    pub fn insunits(&self) -> Option<i16> {
        let value = parse_int(self.raw("$INSUNITS", 70)?)?;
        i16::try_from(value).ok()
    }

    /// The drawing's lower-left extent (`$EXTMIN`), if set.
    pub fn extmin(&self) -> Option<Point3> {
        self.point("$EXTMIN")
    }

    /// The drawing's upper-right extent (`$EXTMAX`), if set.
    pub fn extmax(&self) -> Option<Point3> {
        self.point("$EXTMAX")
    }

    /// `$CLAYER`: the current layer.
    pub fn current_layer(&self) -> Option<&str> {
        self.name_value("$CLAYER", 8)
    }

    /// `$TEXTSTYLE`: the current text style.
    pub fn text_style(&self) -> Option<&str> {
        self.name_value("$TEXTSTYLE", 7)
    }

    /// `$DIMSTYLE`: the current dimension style.
    pub fn dim_style(&self) -> Option<&str> {
        self.name_value("$DIMSTYLE", 2)
    }
}

/// A section this library does not parse, kept verbatim.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawSection {
    /// Section name (group 2), e.g. `THUMBNAILIMAGE`.
    pub name: String,
    /// Everything between `2 <name>` and `0 ENDSEC`.
    pub pairs: Vec<Pair>,
}

/// A block definition from the `BLOCKS` section.
#[derive(Debug, Clone, PartialEq)]
pub struct Block {
    /// Block name (group 2).
    pub name: String,
    /// Handle of the record (group 5), if it has one.
    pub handle: Option<Handle>,
    /// Handle of the owner (group 330): the block record, table or object that holds it.
    pub owner: Option<Handle>,
    /// Layer of the block definition (group 8).
    pub layer: String,
    /// Group 70: 1 = anonymous, 2 = has attribute definitions, 4 = xref, ...
    pub flags: i16,
    /// Base point (groups 10/20/30): the point placed at a reference's insertion point.
    pub base_point: Point3,
    /// Xref path (group 1), empty if none.
    pub xref_path: String,
    /// Description (group 4), empty if none.
    pub description: String,
    /// The entities of the block, in order.
    pub entities: Vec<Entity>,
    /// The pairs of the closing `ENDBLK` record (handle, owner, layer, ...).
    pub end: Vec<Pair>,
    /// Pairs of the `BLOCK` record this library does not interpret.
    pub extra: Vec<Pair>,
}

impl Block {
    /// An empty block called `name`.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            handle: None,
            owner: None,
            layer: "0".into(),
            flags: 0,
            base_point: Point3::default(),
            xref_path: String::new(),
            description: String::new(),
            entities: Vec::new(),
            end: Vec::new(),
            extra: Vec::new(),
        }
    }
}

impl Named for Block {
    fn name(&self) -> &str {
        &self.name
    }
}

/// Pairs found where the format does not allow them (outside any section,
/// between tables or blocks, after `EOF`, ...). They are kept, with a
/// warning, so nothing is lost; `section` and `index` say where they were.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StrayRecord {
    /// The enclosing section (`HEADER`, `TABLES`, `BLOCKS`, `ENTITIES`), an
    /// empty string outside any section, or `EOF` for data after the end.
    pub section: String,
    /// How many items came before it in that place: sections (outside any
    /// section), header variables, tables, blocks, or entities.
    pub index: usize,
    /// The record's pairs, starting with its `0` pair.
    pub pairs: Vec<Pair>,
}

/// The bytes a drawing was read from, kept so that an unchanged drawing saves
/// back byte-for-byte (see [`Drawing::save_bytes`]).
///
/// The source is provenance, not content: it never affects equality, so two
/// drawings with the same content compare equal whatever they were read from.
#[derive(Clone, Default)]
pub struct Source(Option<Arc<[u8]>>);

impl Source {
    /// Keeps a copy of the bytes a drawing was read from.
    pub fn new(bytes: &[u8]) -> Self {
        Self(Some(Arc::from(bytes)))
    }

    /// The original bytes, if the drawing was read from a file.
    pub fn bytes(&self) -> Option<&[u8]> {
        self.0.as_deref()
    }
}

impl PartialEq for Source {
    fn eq(&self, _other: &Self) -> bool {
        true
    }
}

impl fmt::Debug for Source {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.0 {
            Some(bytes) => write!(f, "Source({} bytes)", bytes.len()),
            None => f.write_str("Source(none)"),
        }
    }
}

/// A whole DXF document.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Drawing {
    /// How the file's bytes were decoded; a writer uses it to save back.
    pub encoding: FileEncoding,
    /// Header variables.
    pub header: Header,
    /// The `CLASSES` section, verbatim.
    pub classes: Vec<Pair>,
    /// Symbol tables (layers, linetypes, styles, ...).
    pub tables: Tables,
    /// Block definitions, keyed by case-insensitive name. Definitions that
    /// could not be parsed are kept verbatim in `blocks.unparsed`.
    pub blocks: Table<Block>,
    /// Model and paper space entities, in file order.
    pub entities: Vec<Entity>,
    /// The `OBJECTS` section, verbatim.
    pub objects: Vec<Pair>,
    /// Sections with other names (e.g. `THUMBNAILIMAGE`), verbatim.
    pub other_sections: Vec<RawSection>,
    /// Misplaced pairs, in file order.
    pub stray: Vec<StrayRecord>,
    /// The text from a line the tokenizer could not read to the end of the
    /// file, if reading stopped early. Kept so it is not lost.
    pub unparsed_tail: Option<String>,
    /// The bytes the drawing was read from (not part of its content).
    pub source: Source,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colors_from_aci() {
        assert_eq!(Color::from_aci(0), Some(Color::ByBlock));
        assert_eq!(Color::from_aci(256), Some(Color::ByLayer));
        assert_eq!(Color::from_aci(1), Some(Color::Index(1)));
        assert_eq!(Color::from_aci(255), Some(Color::Index(255)));
        assert_eq!(Color::from_aci(257), None);
        assert_eq!(Color::from_aci(-1), None);
    }

    #[test]
    fn true_color_uses_low_24_bits() {
        assert_eq!(
            Color::from_true_color(0x00FF_8000),
            Some(Color::Rgb(255, 128, 0))
        );
        assert_eq!(
            Color::from_true_color(0xC2FF_8000),
            Some(Color::Rgb(255, 128, 0))
        );
        assert_eq!(Color::from_true_color(1 << 40), None);
    }

    #[test]
    fn aci_palette_and_nearest() {
        assert_eq!(Color::aci_rgb(1), (255, 0, 0));
        assert_eq!(Color::aci_rgb(10), (255, 0, 0));
        assert_eq!(Color::aci_rgb(255), (255, 255, 255));
        assert_eq!(Color::nearest_aci(255, 0, 0), 1);
        assert_eq!(Color::nearest_aci(0, 0, 250), 5);
        assert_eq!(Color::nearest_aci(250, 250, 0), 2);
        assert_eq!(Color::nearest_aci(130, 130, 130), 8);
        for i in 1..=255 {
            let (r, g, b) = Color::aci_rgb(i);
            assert_eq!(Color::aci_rgb(Color::nearest_aci(r, g, b)), (r, g, b));
        }
    }

    #[test]
    fn lineweights() {
        assert_eq!(LineWeight::from_code(-1), Some(LineWeight::ByLayer));
        assert_eq!(LineWeight::from_code(25), Some(LineWeight::Hundredths(25)));
        assert_eq!(LineWeight::from_code(212), None);
    }

    #[test]
    fn handles_are_hex() {
        assert_eq!(Handle::parse(" 1F "), Some(Handle(0x1F)));
        assert_eq!(Handle::parse("zz"), None);
        assert_eq!(Handle::parse(""), None);
        assert_eq!(Handle(0x2A).to_string(), "2A");
    }

    #[test]
    fn versions_round_trip() {
        for v in [Version::R12, Version::R2000, Version::R2018] {
            assert_eq!(Version::from_acadver(v.acadver()), Some(v));
        }
        assert_eq!(Version::from_acadver("AC1014"), Some(Version::R14));
        assert_eq!(Version::from_acadver("AC1012"), Some(Version::R13));
        assert_eq!(Version::from_acadver("AC1006"), None);
    }

    #[test]
    fn header_accessors() {
        let var = |name: &str, pairs: &[(i16, &str)]| HeaderVariable {
            name: name.into(),
            pairs: pairs.iter().map(|&(c, v)| Pair::new(c, v)).collect(),
        };
        let header = Header {
            variables: vec![
                var("$ACADVER", &[(1, "AC1032")]),
                var("$INSUNITS", &[(70, "     4")]),
                var("$EXTMIN", &[(10, "-1.5"), (20, "2"), (30, "0")]),
                var("$extmax", &[(10, "10"), (20, "20")]),
                var("$CLAYER", &[(8, "WALLS")]),
            ],
        };
        assert_eq!(header.version(), Some(Version::R2018));
        assert_eq!(header.insunits(), Some(4));
        assert_eq!(header.extmin(), Some(Point3::new(-1.5, 2.0, 0.0)));
        assert_eq!(header.extmax(), Some(Point3::new(10.0, 20.0, 0.0)));
        assert_eq!(header.current_layer(), Some("WALLS"));
        assert_eq!(header.codepage(), None);
    }
}
