use std::{
    error::Error,
    fmt::{Display, Formatter, Result as FmtResult},
};

#[derive(Debug)]
pub enum AppCreationError {
    NoAdapterFound,
    RequestDeviceError(wgpu::RequestDeviceError),
    CreateSurfaceError(wgpu::CreateSurfaceError),
}

impl Display for AppCreationError {
    fn fmt(&self, f: &mut Formatter) -> FmtResult {
        match self {
            Self::NoAdapterFound => {
                write!(f, "No adapter found.")
            }
            Self::RequestDeviceError(e) => {
                write!(f, "Error requesting device: {}", e)
            }
            Self::CreateSurfaceError(e) => {
                write!(f, "Error creating surface: {}", e)
            }
        }
    }
}

impl Error for AppCreationError {
    fn cause(&self) -> Option<&(dyn Error)> {
        match *self {
            Self::NoAdapterFound => None,
            Self::RequestDeviceError(ref e) => Some(e),
            Self::CreateSurfaceError(ref e) => Some(e),
        }
    }
}

impl From<wgpu::RequestDeviceError> for AppCreationError {
    fn from(e: wgpu::RequestDeviceError) -> Self {
        Self::RequestDeviceError(e)
    }
}

impl From<wgpu::CreateSurfaceError> for AppCreationError {
    fn from(e: wgpu::CreateSurfaceError) -> Self {
        Self::CreateSurfaceError(e)
    }
}
