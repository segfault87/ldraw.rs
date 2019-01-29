use std::fmt;
use std::io::Error as IoError;

#[derive(Debug)]
pub enum ParseError {
    TypeMismatch(&'static str, String),
    IoError(Box<IoError>),
    EndOfLine,
    InvalidBfcStatement(String),
    InvalidDocumentStructure,
    UnexpectedCommand(String),
    InvalidToken(String),
    MultipartDocument,
}

#[derive(Debug)]
pub struct DocumentParseError {
    pub line: usize,
    pub error: ParseError,
}

#[derive(Debug)]
pub enum ColorDefinitionParseError {
    ParseError(ParseError),
    DocumentParseError(DocumentParseError),
    UnknownMaterial(String),
}

#[derive(Debug)]
pub enum SerializeError {
    NoSerializable,
    IoError(Box<IoError>),
}

#[derive(Debug)]
pub enum LibraryError {
    NoLDrawDir,
    IoError(Box<IoError>),
}

impl From<IoError> for ParseError {
    fn from(e: IoError) -> ParseError {
        ParseError::IoError(Box::new(e))
    }
}

impl From<IoError> for SerializeError {
    fn from(e: IoError) -> SerializeError {
        SerializeError::IoError(Box::new(e))
    }
}

impl From<IoError> for LibraryError {
    fn from(e: IoError) -> LibraryError {
        LibraryError::IoError(Box::new(e))
    }
}

impl From<ParseError> for ColorDefinitionParseError {
    fn from(e: ParseError) -> ColorDefinitionParseError {
        ColorDefinitionParseError::ParseError(e)
    }
}

impl From<DocumentParseError> for ColorDefinitionParseError {
    fn from(e: DocumentParseError) -> ColorDefinitionParseError {
        ColorDefinitionParseError::DocumentParseError(e)
    }
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            ParseError::TypeMismatch(type_, val) => {
                write!(f, "Error reading value '{}' into {}", val, type_)
            }
            ParseError::IoError(err) => write!(f, "{}", err),
            ParseError::EndOfLine => write!(f, "End of line"),
            ParseError::InvalidBfcStatement(stmt) => write!(f, "Invalid BFC statement: {}", stmt),
            ParseError::InvalidDocumentStructure => write!(f, "Invalid document structure."),
            ParseError::UnexpectedCommand(cmd) => write!(f, "Unexpected command: {}", cmd),
            ParseError::InvalidToken(token) => write!(f, "Invalid token: {}", token),
            ParseError::MultipartDocument => write!(f, "Unexpected multipart document."),
        }
    }
}

impl fmt::Display for DocumentParseError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{} (at line {})", self.error, self.line)
    }
}

impl fmt::Display for ColorDefinitionParseError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            ColorDefinitionParseError::ParseError(e) => write!(f, "{}", e),
            ColorDefinitionParseError::DocumentParseError(e) => write!(f, "{}", e),
            ColorDefinitionParseError::UnknownMaterial(e) => write!(f, "Unknown material: {}", e),
        }
    }
}

impl fmt::Display for LibraryError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            LibraryError::NoLDrawDir => write!(f, "No LDraw library found."),
            LibraryError::IoError(err) => write!(f, "{}", err),
        }
    }
}
