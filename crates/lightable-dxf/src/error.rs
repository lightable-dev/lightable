use std::fmt;

/// A failure that stops a file from loading at all.
///
/// Problems that only affect part of a file are reported as [`Warning`]s instead.
#[derive(Debug)]
pub enum DxfError {
    /// The file could not be read.
    Io(std::io::Error),
    /// The file is a binary DXF, which is not supported yet.
    BinaryNotSupported,
    /// The input does not look like a DXF file at all.
    NotDxf,
    /// A group code line could not be parsed.
    Syntax {
        /// 1-based line of the group code.
        line: usize,
        /// What is wrong with it.
        message: String,
    },
    /// The input ended after a group code, before its value.
    UnexpectedEof {
        /// 1-based line of the last group code.
        line: usize,
    },
    /// Writing is supported for R12 and R2018 only.
    UnsupportedVersion(crate::model::Version),
}

impl DxfError {
    /// The 1-based line the error refers to, if any.
    pub fn line(&self) -> Option<usize> {
        match self {
            Self::Syntax { line, .. } | Self::UnexpectedEof { line } => Some(*line),
            _ => None,
        }
    }
}

impl fmt::Display for DxfError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(err) => write!(f, "file error: {err}"),
            Self::BinaryNotSupported => f.write_str("binary DXF is not supported yet"),
            Self::NotDxf => f.write_str("not a DXF file (no SECTION found)"),
            Self::Syntax { line, message } => write!(f, "line {line}: {message}"),
            Self::UnexpectedEof { line } => {
                write!(
                    f,
                    "line {line}: file ends after a group code, before its value"
                )
            }
            Self::UnsupportedVersion(v) => {
                write!(
                    f,
                    "cannot write {} ({v:?}); write R12 or R2018",
                    v.acadver()
                )
            }
        }
    }
}

impl std::error::Error for DxfError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(err) => Some(err),
            _ => None,
        }
    }
}

impl From<std::io::Error> for DxfError {
    fn from(err: std::io::Error) -> Self {
        Self::Io(err)
    }
}

/// A recoverable problem found while reading. The drawing still loads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Warning {
    /// 1-based line of the group code where the problem starts, if known.
    pub line: Option<usize>,
    /// What went wrong, in plain words.
    pub message: String,
}

impl Warning {
    /// A warning at `line` (1-based), if known.
    pub fn new(line: Option<usize>, message: impl Into<String>) -> Self {
        Self {
            line,
            message: message.into(),
        }
    }
}

impl fmt::Display for Warning {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.line {
            Some(line) => write!(f, "line {line}: {}", self.message),
            None => f.write_str(&self.message),
        }
    }
}
