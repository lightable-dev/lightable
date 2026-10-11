//! Writing a [`Drawing`] as DXF: R12 or R2018, or back to its own bytes.
//!
//! Writing happens in two passes: [`normalize`] makes a copy of the drawing
//! valid for the target version (handles, owners, required tables, layouts,
//! R12 downgrades), then the emitters format it. Both are deterministic, so
//! writing what was written gives the same bytes again.

mod downgrade;
mod entity;
mod normalize;
mod objects;
mod out;
mod table;
#[cfg(test)]
mod tests;

use std::path::Path;

use crate::encoding::original_byte;
use crate::error::{DxfError, Warning};
use crate::model::{Drawing, Version};
use crate::reader::read_bytes;

use entity::EntityWriter;
use out::Out;
use table::EntityWriterFactory;

impl Drawing {
    /// Writes the drawing as `version`, which must be [`Version::R12`] or
    /// [`Version::R2018`].
    ///
    /// If the drawing was read from a file of that version and has not been
    /// changed since, the original bytes are returned unchanged. Otherwise
    /// the file is generated (see [`Drawing::generate_bytes`]).
    pub fn to_bytes(&self, version: Version) -> Result<(Vec<u8>, Vec<Warning>), DxfError> {
        check_target(version)?;
        if self.header.version() == Some(version)
            && let Some(bytes) = self.unchanged_source()
        {
            return Ok((bytes.to_vec(), Vec::new()));
        }
        self.generate_bytes(version)
    }

    /// Generates a complete file for `version` (R12 or R2018) from the
    /// drawing's content, never reusing the source bytes. Every object gets
    /// a handle; in R2018 every object also gets its owner link.
    pub fn generate_bytes(&self, version: Version) -> Result<(Vec<u8>, Vec<Warning>), DxfError> {
        check_target(version)?;
        Ok(generate(self, version))
    }

    /// Writes the drawing to `path` as `version` (see [`Drawing::to_bytes`]).
    pub fn write_file(
        &self,
        path: impl AsRef<Path>,
        version: Version,
    ) -> Result<Vec<Warning>, DxfError> {
        let (bytes, warnings) = self.to_bytes(version)?;
        std::fs::write(path, bytes)?;
        Ok(warnings)
    }

    /// Saves the drawing back: the original bytes if it is unchanged since
    /// it was read (whatever its version, including stray records, the
    /// unparsed tail and undecodable bytes), otherwise a generated file in
    /// R12 for R12 drawings and R2018 for everything else, with a warning
    /// when that changes the version.
    pub fn save_bytes(&self) -> (Vec<u8>, Vec<Warning>) {
        if let Some(bytes) = self.unchanged_source() {
            return (bytes.to_vec(), Vec::new());
        }
        let target = if self.header.version() == Some(Version::R12) {
            Version::R12
        } else {
            Version::R2018
        };
        let (bytes, mut warnings) = generate(self, target);
        let was = self.header.acadver().unwrap_or("an unknown version");
        if was != target.acadver() {
            warnings.insert(
                0,
                Warning::new(
                    None,
                    format!(
                        "the drawing has changed, so it is saved as {} (it was {was})",
                        target.acadver()
                    ),
                ),
            );
        }
        (bytes, warnings)
    }

    /// Saves the drawing back to `path` (see [`Drawing::save_bytes`]).
    pub fn save_file(&self, path: impl AsRef<Path>) -> Result<Vec<Warning>, DxfError> {
        let (bytes, warnings) = self.save_bytes();
        std::fs::write(path, bytes)?;
        Ok(warnings)
    }

    /// The source bytes, if reading them again gives this same drawing.
    fn unchanged_source(&self) -> Option<&[u8]> {
        let bytes = self.source.bytes()?;
        let (reread, _) = read_bytes(bytes).ok()?;
        (reread == *self).then_some(bytes)
    }
}

fn check_target(version: Version) -> Result<(), DxfError> {
    match version {
        Version::R12 | Version::R2018 => Ok(()),
        other => Err(DxfError::UnsupportedVersion(other)),
    }
}

fn generate(d: &Drawing, version: Version) -> (Vec<u8>, Vec<Warning>) {
    let normalize::Normalized {
        drawing: d,
        code_page,
        mut notes,
    } = normalize::normalize(d, version);
    let r12 = version == Version::R12;
    let mut out = Out::default();

    out.pair(0, "SECTION");
    out.pair(2, "HEADER");
    for var in &d.header.variables {
        out.pair(9, &var.name);
        out.raw(&var.pairs);
    }
    out.pair(0, "ENDSEC");

    if !r12 {
        out.pair(0, "SECTION");
        out.pair(2, "CLASSES");
        out.raw(&d.classes);
        out.pair(0, "ENDSEC");
    }

    table::tables(&mut out, &d, version);
    let mut factory = EntityWriterFactory { version, code_page };
    table::blocks(&mut out, &d, &mut factory);

    out.pair(0, "SECTION");
    out.pair(2, "ENTITIES");
    let mut w = EntityWriter {
        out: &mut out,
        version,
        code_page,
    };
    for e in &d.entities {
        w.entity(e);
    }
    out.pair(0, "ENDSEC");

    if !r12 {
        out.pair(0, "SECTION");
        out.pair(2, "OBJECTS");
        out.raw(&d.objects);
        out.pair(0, "ENDSEC");
        for section in &d.other_sections {
            out.pair(0, "SECTION");
            out.pair(2, &section.name);
            out.raw(&section.pairs);
            out.pair(0, "ENDSEC");
        }
    }
    out.pair(0, "EOF");

    let bytes = if r12 {
        let (bytes, escaped) = code_page.encode(&out.buf);
        notes.add_n(
            format!(
                "{{n}} character(s) not in {} written as \\U+XXXX escapes",
                code_page.dwgcodepage()
            ),
            escaped,
        );
        bytes
    } else {
        let kept = out
            .buf
            .chars()
            .filter(|&c| original_byte(c).is_some())
            .count();
        notes.add_n(
            "{n} byte(s) that were not valid in the source code page written as private-use characters (UTF-8 cannot hold raw bytes)",
            kept,
        );
        out.buf.into_bytes()
    };
    (bytes, notes.into_warnings())
}
