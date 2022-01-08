#[cfg(all(not(target_arch = "wasm32"), feature = "http"))]
pub mod http;
#[cfg(not(target_arch = "wasm32"))]
pub mod local;
#[cfg(target_arch = "wasm32")]
pub mod web_sys;
