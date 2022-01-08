#[cfg(any(target_arch = "wasm32", feature = "http"))]
pub mod http;
#[cfg(not(target_arch = "wasm32"))]
pub mod local;
