use std::{
    error::Error,
    fmt::{Display, Formatter, Result as FmtResult}
};

use glutin::CreationError;
use ldraw_renderer::error::ShaderError;

#[derive(Debug)]
pub enum ContextCreationError {
    GlContextError(CreationError),
    ShaderInitializationError(ShaderError),
}

impl Display for ContextCreationError {
    fn fmt(&self, f: &mut Formatter) -> FmtResult {
        match self {
            ContextCreationError::GlContextError(e) => {
                write!(f, "Error creating OpenGL context: {}", e)
            }
            ContextCreationError::ShaderInitializationError(e) => {
                write!(f, "Error initializing shaders: {}", e)
            }
        }
    }
}

impl Error for ContextCreationError {
    fn cause(&self) -> Option<&(dyn Error)> {
        match *self {
            ContextCreationError::GlContextError(ref err) => Some(&*err),
            ContextCreationError::ShaderInitializationError(ref err) => Some(&*err),
        }
    }
}

impl From<CreationError> for ContextCreationError {
    fn from(e: CreationError) -> Self {
        ContextCreationError::GlContextError(e)
    }
}

impl From<ShaderError> for ContextCreationError {
    fn from(e: ShaderError) -> Self {
        ContextCreationError::ShaderInitializationError(e)
    }
}
