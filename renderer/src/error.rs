#[derive(thiserror::Error, Debug)]
pub enum ObjectSelectionError {
    #[error("Selection coordinate is out of range: {0:?}")]
    OutOfRange(crate::ObjectSelection),
    #[error("Async buffer read error: {0}")]
    AsyncBufferReadError(#[from] wgpu::BufferAsyncError),
}
