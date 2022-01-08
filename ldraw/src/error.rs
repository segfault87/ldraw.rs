use std::{error::Error, fmt, io::Error as IoError};

#[cfg(any(target_arch = "wasm32", feature = "http"))]
use reqwest::Error as ReqwestError;

#[cfg(not(any(target_arch = "wasm32", feature = "http")))]
mod stub {
    use super::{Error, fmt};

    #[derive(Debug)]
    pub struct ReqwestError;

    impl fmt::Display for ReqwestError {
        fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
            write!(f, "")
        }
    }

    impl Error for ReqwestError {
        fn source(&self) -> Option<&(dyn Error + 'static)> {
            None
        }
    }
}

#[cfg(not(any(target_arch = "wasm32", feature = "http")))]
use stub::ReqwestError;

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

impl From<IoError> for ParseError {
    fn from(e: IoError) -> ParseError {
        ParseError::IoError(Box::new(e))
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

impl Error for ParseError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            ParseError::IoError(e) => Some(e),
            _ => None,
        }
    }
}

#[derive(Debug)]
pub struct DocumentParseError {
    pub line: usize,
    pub error: ParseError,
}

impl From<DocumentParseError> for ColorDefinitionParseError {
    fn from(e: DocumentParseError) -> ColorDefinitionParseError {
        ColorDefinitionParseError::DocumentParseError(e)
    }
}

impl fmt::Display for DocumentParseError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{} (at line {})", self.error, self.line)
    }
}

impl Error for DocumentParseError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(&self.error)
    }
}

#[derive(Debug)]
pub enum ColorDefinitionParseError {
    ParseError(ParseError),
    DocumentParseError(DocumentParseError),
    UnknownMaterial(String),
}

impl From<ParseError> for ColorDefinitionParseError {
    fn from(e: ParseError) -> ColorDefinitionParseError {
        ColorDefinitionParseError::ParseError(e)
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

impl Error for ColorDefinitionParseError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            ColorDefinitionParseError::ParseError(e) => Some(e),
            ColorDefinitionParseError::DocumentParseError(e) => Some(e),
            _ => None,
        }
    }
}

#[derive(Debug)]
pub enum SerializeError {
    NoSerializable,
    IoError(Box<IoError>),
}

impl From<IoError> for SerializeError {
    fn from(e: IoError) -> SerializeError {
        SerializeError::IoError(Box::new(e))
    }
}

impl fmt::Display for SerializeError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            SerializeError::NoSerializable => write!(f, "Statement is not serializable."),
            SerializeError::IoError(err) => write!(f, "{}", err),
        }
    }
}

impl Error for SerializeError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            SerializeError::IoError(e) => Some(e),
            _ => None,
        }
    }
}

#[derive(Debug)]
pub enum ResolutionError {
    NoLDrawDir,
    FileNotFound,
    IoError(Box<IoError>),
    DocumentParseError(DocumentParseError),
    ColorDefinitionParseError(ColorDefinitionParseError),
    RemoteError(ReqwestError),
}

impl From<IoError> for ResolutionError {
    fn from(e: IoError) -> ResolutionError {
        ResolutionError::IoError(Box::new(e))
    }
}

impl From<DocumentParseError> for ResolutionError {
    fn from(e: DocumentParseError) -> ResolutionError {
        ResolutionError::DocumentParseError(e)
    }
}

impl From<ColorDefinitionParseError> for ResolutionError {
    fn from(e: ColorDefinitionParseError) -> ResolutionError {
        ResolutionError::ColorDefinitionParseError(e)
    }
}

impl From<ReqwestError> for ResolutionError {
    fn from(e: ReqwestError) -> ResolutionError {
        ResolutionError::RemoteError(e)
    }
}

impl fmt::Display for ResolutionError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            ResolutionError::NoLDrawDir => write!(f, "No LDraw library found."),
            ResolutionError::FileNotFound => write!(f, "File not found."),
            ResolutionError::IoError(err) => write!(f, "{}", err),
            ResolutionError::DocumentParseError(err) => write!(f, "{}", err),
            ResolutionError::ColorDefinitionParseError(err) => write!(f, "{}", err),
            ResolutionError::RemoteError(err) => write!(f, "{}", err),
        }
    }
}

impl Error for ResolutionError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            ResolutionError::IoError(e) => Some(e),
            ResolutionError::DocumentParseError(e) => Some(e),
            ResolutionError::ColorDefinitionParseError(e) => Some(e),
            ResolutionError::RemoteError(e) => Some(e),
            _ => None,
        }
    }
}
