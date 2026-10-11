//! The `TABLES` section: layers, linetypes, text styles, dim styles and
//! block records, plus any other table kept verbatim.

use crate::model::{Color, Handle, LineWeight};
use crate::tokenizer::Pair;

/// Table entries are keyed by name.
pub trait Named {
    /// The entry's name.
    fn name(&self) -> &str;
}

/// Case-insensitive comparison of table entry names.
pub fn names_equal(a: &str, b: &str) -> bool {
    a.chars()
        .flat_map(char::to_uppercase)
        .eq(b.chars().flat_map(char::to_uppercase))
}

/// A symbol table whose entries are unique by case-insensitive name.
///
/// Entries with an empty name (e.g. shape-file text styles) are allowed and
/// are not looked up by name.
#[derive(Debug, Clone, PartialEq)]
pub struct Table<T> {
    /// Pairs of the table's own record (handle, owner, max entries, ...).
    pub header: Vec<Pair>,
    /// Entries in file order.
    pub entries: Vec<T>,
    /// Entries that could not be parsed, kept verbatim (including their
    /// leading `0` pair) so they are not lost.
    pub unparsed: Vec<Vec<Pair>>,
}

impl<T> Default for Table<T> {
    fn default() -> Self {
        Self {
            header: Vec::new(),
            entries: Vec::new(),
            unparsed: Vec::new(),
        }
    }
}

impl<T: Named> Table<T> {
    /// The entry called `name`, ignoring case.
    pub fn get(&self, name: &str) -> Option<&T> {
        self.entries.iter().find(|e| names_equal(e.name(), name))
    }

    /// The entry called `name`, ignoring case, for changing it.
    pub fn get_mut(&mut self, name: &str) -> Option<&mut T> {
        self.entries
            .iter_mut()
            .find(|e| names_equal(e.name(), name))
    }

    /// True if an entry is called `name`, ignoring case.
    pub fn contains(&self, name: &str) -> bool {
        self.get(name).is_some()
    }

    /// Adds `entry`, replacing and returning any entry with the same name.
    pub fn insert(&mut self, entry: T) -> Option<T> {
        if entry.name().is_empty() {
            self.entries.push(entry);
            return None;
        }
        match self.get_mut(entry.name()) {
            Some(existing) => Some(std::mem::replace(existing, entry)),
            None => {
                self.entries.push(entry);
                None
            }
        }
    }

    /// Entries in file order.
    pub fn iter(&self) -> std::slice::Iter<'_, T> {
        self.entries.iter()
    }

    /// Number of entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// True when there are no entries.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// A `LAYER` table entry.
#[derive(Debug, Clone, PartialEq)]
pub struct Layer {
    /// Layer name (group 2).
    pub name: String,
    /// Handle of the record (group 5), if it has one.
    pub handle: Option<Handle>,
    /// Handle of the owner (group 330): the block record, table or object that holds it.
    pub owner: Option<Handle>,
    /// Group 70 flags: 1 = frozen, 2 = frozen in new viewports, 4 = locked.
    pub flags: i16,
    /// Colour of the layer (group 62/420).
    pub color: Color,
    /// False when the layer is switched off (stored as a negative colour).
    pub on: bool,
    /// Linetype name (group 6).
    pub linetype: String,
    /// Line weight (group 370).
    pub lineweight: LineWeight,
    /// False when the layer is not plotted (group 290).
    pub plot: bool,
    /// Pairs this library does not interpret, in file order.
    pub extra: Vec<Pair>,
}

impl Layer {
    /// A layer called `name`: colour 7, `CONTINUOUS`, on and plotted.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            handle: None,
            owner: None,
            flags: 0,
            color: Color::Index(7),
            on: true,
            linetype: "CONTINUOUS".into(),
            lineweight: LineWeight::Default,
            plot: true,
            extra: Vec::new(),
        }
    }

    /// True when the layer is frozen (flag 1).
    pub fn frozen(&self) -> bool {
        self.flags & 1 != 0
    }

    /// True when the layer is locked (flag 4).
    pub fn locked(&self) -> bool {
        self.flags & 4 != 0
    }
}

impl Named for Layer {
    fn name(&self) -> &str {
        &self.name
    }
}

/// One dash, gap, dot, shape or text element of a linetype pattern.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct LinetypeElement {
    /// Group 49: positive = dash, negative = gap, 0 = dot.
    pub length: f64,
    /// Group 74: 0 = plain; 2 = text, 4 = shape (plus rotation/offset flags).
    pub flags: i16,
    /// Group 75: shape number, or 0 for text.
    pub shape_number: i16,
    /// Group 340: the text style holding the shape or text font.
    pub style: Option<Handle>,
    /// Group 46.
    pub scale: f64,
    /// Group 50, degrees.
    pub rotation: f64,
    /// Groups 44 and 45.
    pub offset: (f64, f64),
    /// Group 9: text for a text element.
    pub text: Option<String>,
}

/// An `LTYPE` table entry.
#[derive(Debug, Clone, PartialEq)]
pub struct Linetype {
    /// Linetype name (group 2).
    pub name: String,
    /// Handle of the record (group 5), if it has one.
    pub handle: Option<Handle>,
    /// Handle of the owner (group 330): the block record, table or object that holds it.
    pub owner: Option<Handle>,
    /// Group 70 flags.
    pub flags: i16,
    /// Description shown to users (group 3).
    pub description: String,
    /// Group 72; always 65 (`A`).
    pub alignment: i16,
    /// Group 40: total pattern length.
    pub pattern_length: f64,
    /// The pattern's elements (dashes, gaps, dots, shapes, text).
    pub elements: Vec<LinetypeElement>,
    /// Pairs this library does not interpret, in file order.
    pub extra: Vec<Pair>,
}

impl Named for Linetype {
    fn name(&self) -> &str {
        &self.name
    }
}

/// A `STYLE` table entry (text style or shape file).
#[derive(Debug, Clone, PartialEq)]
pub struct TextStyle {
    /// Empty for shape-file entries (flag 1).
    pub name: String,
    /// Handle of the record (group 5), if it has one.
    pub handle: Option<Handle>,
    /// Handle of the owner (group 330): the block record, table or object that holds it.
    pub owner: Option<Handle>,
    /// Group 70: 1 = shape file, 4 = vertical text, ...
    pub flags: i16,
    /// Group 40: fixed text height, 0 if not fixed.
    pub height: f64,
    /// Relative character width (group 41); 1 is normal.
    pub width_factor: f64,
    /// Group 50, degrees.
    pub oblique: f64,
    /// Group 71: 2 = backward, 4 = upside down.
    pub generation_flags: i16,
    /// Group 42.
    pub last_height: f64,
    /// Group 3: primary font file.
    pub font: String,
    /// Group 4: big font file, empty if none.
    pub big_font: String,
    /// Pairs this library does not interpret, in file order.
    pub extra: Vec<Pair>,
}

impl Named for TextStyle {
    fn name(&self) -> &str {
        &self.name
    }
}

/// A `DIMSTYLE` table entry.
///
/// The dimension variables are kept as raw pairs in file order; typed access
/// arrives with dimension rendering (M7).
#[derive(Debug, Clone, PartialEq)]
pub struct DimStyle {
    /// Dimension style name (group 2).
    pub name: String,
    /// Handle of the record (group 5), if it has one.
    pub handle: Option<Handle>,
    /// Handle of the owner (group 330): the block record, table or object that holds it.
    pub owner: Option<Handle>,
    /// Group 70 flags.
    pub flags: i16,
    /// Every dimension-variable pair (DIMSCALE = 40, DIMTXT = 140, ...).
    pub variables: Vec<Pair>,
    /// `102` groups and extended data.
    pub extra: Vec<Pair>,
}

impl DimStyle {
    /// The raw value of the first variable with this group code.
    pub fn variable(&self, code: i16) -> Option<&str> {
        self.variables
            .iter()
            .find(|p| p.code == code)
            .map(|p| p.raw.as_str())
    }
}

impl Named for DimStyle {
    fn name(&self) -> &str {
        &self.name
    }
}

/// A `BLOCK_RECORD` table entry (R13 and later).
#[derive(Debug, Clone, PartialEq)]
pub struct BlockRecord {
    /// Block name (group 2).
    pub name: String,
    /// Handle of the record (group 5), if it has one.
    pub handle: Option<Handle>,
    /// Handle of the owner (group 330): the block record, table or object that holds it.
    pub owner: Option<Handle>,
    /// Group 340: the associated layout, for model and paper space blocks.
    pub layout: Option<Handle>,
    /// Group 70: insertion units.
    pub units: i16,
    /// Group 280.
    pub explodable: bool,
    /// Group 281.
    pub scalable: bool,
    /// Preview image (310), `102` groups, extended data, ...
    pub extra: Vec<Pair>,
}

impl Named for BlockRecord {
    fn name(&self) -> &str {
        &self.name
    }
}

/// A table this library does not parse (`VPORT`, `VIEW`, `UCS`, `APPID`),
/// kept verbatim.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawTable {
    /// The table name from the pair after `0 TABLE`.
    pub name: String,
    /// Everything between that name and `0 ENDTAB`.
    pub pairs: Vec<Pair>,
}

/// The `TABLES` section.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Tables {
    /// `LAYER` table.
    pub layers: Table<Layer>,
    /// `LTYPE` table.
    pub linetypes: Table<Linetype>,
    /// `STYLE` table.
    pub text_styles: Table<TextStyle>,
    /// `DIMSTYLE` table.
    pub dim_styles: Table<DimStyle>,
    /// `BLOCK_RECORD` table.
    pub block_records: Table<BlockRecord>,
    /// Other tables, in file order.
    pub other: Vec<RawTable>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tables_are_case_insensitive() {
        let mut table = Table::default();
        assert!(table.insert(Layer::new("Walls")).is_none());
        assert!(table.contains("WALLS"));
        assert!(table.get("walls").is_some());
        let old = table.insert(Layer::new("WALLS")).expect("replaced");
        assert_eq!(old.name, "Walls");
        assert_eq!(table.len(), 1);
        assert!(names_equal("wände", "WÄNDE"));
    }

    #[test]
    fn empty_names_are_never_replaced() {
        let style = |font: &str| TextStyle {
            name: String::new(),
            handle: None,
            owner: None,
            flags: 1,
            height: 0.0,
            width_factor: 1.0,
            oblique: 0.0,
            generation_flags: 0,
            last_height: 0.0,
            font: font.into(),
            big_font: String::new(),
            extra: Vec::new(),
        };
        let mut table = Table::default();
        assert!(table.insert(style("a.shx")).is_none());
        assert!(table.insert(style("b.shx")).is_none());
        assert_eq!(table.len(), 2);
    }

    #[test]
    fn dim_style_variables() {
        let style = DimStyle {
            name: "Standard".into(),
            handle: None,
            owner: None,
            flags: 0,
            variables: vec![Pair::new(40, "1.0"), Pair::new(140, "2.5")],
            extra: Vec::new(),
        };
        assert_eq!(style.variable(140), Some("2.5"));
        assert_eq!(style.variable(41), None);
    }
}
