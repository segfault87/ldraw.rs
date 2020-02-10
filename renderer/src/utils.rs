use std::{
    slice::from_raw_parts,
};

pub(crate) fn cast_as_bytes<'a>(input: &'a [f32]) -> &'a [u8] {
    unsafe { from_raw_parts(input.as_ptr() as *const u8, input.len() * 4) }
}
