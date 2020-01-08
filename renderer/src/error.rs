use std::error::Error;
use std::fmt;

#[derive(Clone, Debug)]
pub enum ShaderError {
    CreationError(String),
    CompileError(String),
    LinkError(String),
}

impl fmt::Display for ShaderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ShaderError::CreationError(e) =>
                write!(f, "Error creating program/shader object: {}", e),
            ShaderError::CompileError(e) =>
                write!(f, "Error compiling shader: {}", e),
            ShaderError::LinkError(e) =>
                write!(f, "Error linking program: {}", e),
        }
    }
}

impl Error for ShaderError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        None
    }
}

#[derive(Clone, Debug)]
pub enum RendererError {
    ShaderError(ShaderError),
}

impl fmt::Display for RendererError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RendererError::ShaderError(e) => e.fmt(f)
        }
    }
}

impl Error for RendererError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            RendererError::ShaderError(e) => Some(e),
        }
    }
}

impl From<ShaderError> for RendererError {
    fn from(error: ShaderError) -> Self {
        RendererError::ShaderError(error)
    }
}
