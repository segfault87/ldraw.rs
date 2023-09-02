use std::{
    error::Error,
    fmt::{Display, Formatter, Result as FmtResult},
};

#[derive(Debug)]
pub enum ContextCreationError {
    NoAdapterFound,
    RequestDeviceError(wgpu::RequestDeviceError),
}

impl Display for ContextCreationError {
    fn fmt(&self, f: &mut Formatter) -> FmtResult {
        match self {
            ContextCreationError::NoAdapterFound => {
                write!(f, "No adapter found.")
            }
            ContextCreationError::RequestDeviceError(e) => {
                write!(f, "Error requesting device: {}", e)
            }
        }
    }
}

impl Error for ContextCreationError {
    fn cause(&self) -> Option<&(dyn Error)> {
        match *self {
            ContextCreationError::NoAdapterFound => None,
            ContextCreationError::RequestDeviceError(ref e) => Some(e),
        }
    }
}

impl From<wgpu::RequestDeviceError> for ContextCreationError {
    fn from(e: wgpu::RequestDeviceError) -> Self {
        Self::RequestDeviceError(e)
    }
}
