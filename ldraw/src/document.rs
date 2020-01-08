use std::collections::HashMap;
use std::iter::Iterator;
use std::vec::Vec;

use crate::elements::{Command, Header, Line, Meta, OptionalLine, PartReference, Quad, Triangle};
use crate::{NormalizedAlias, Winding};

#[derive(Clone, Debug)]
pub enum BfcCertification {
    NotApplicable,
    NoCertify,
    Certify(Winding),
}

impl BfcCertification {
    pub fn is_certified(&self) -> Option<bool> {
        match self {
            BfcCertification::Certify(_) => Some(true),
            BfcCertification::NoCertify => Some(false),
            BfcCertification::NotApplicable => None,
        }
    }

    pub fn get_winding(&self) -> Option<Winding> {
        match self {
            BfcCertification::Certify(w) => Some(*w),
            _ => None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Document {
    pub name: String,
    pub description: String,
    pub author: String,
    pub bfc: BfcCertification,
    pub headers: Vec<Header>,
    pub commands: Vec<Command>,
}

impl Document {
    pub fn has_geometry(&self) -> bool {
        for item in self.commands.iter() {
            match item {
                Command::Line(_)
                | Command::Triangle(_)
                | Command::Quad(_)
                | Command::OptionalLine(_) => {
                    return true;
                }
                _ => (),
            }
        }

        false
    }
}

macro_rules! define_iterator(
    ($fn:ident, $fn_mut:ident, $cmdval:path, $type:ty) => (
        impl<'a> Document {
            pub fn $fn(&'a self) -> impl Iterator<Item = &'a $type> {
                self.commands.iter().filter_map(|value| match value {
                    $cmdval(m) => Some(m),
                    _ => None,
                })
            }

            pub fn $fn_mut(&'a mut self) -> impl Iterator<Item = &'a mut $type> + 'a {
                self.commands.iter_mut().filter_map(|value| match value {
                    $cmdval(m) => Some(m),
                    _ => None,
                })
            }
        }
    )
);

define_iterator!(iter_meta, iter_meta_mut, Command::Meta, Meta);
define_iterator!(
    iter_refs,
    iter_refs_mut,
    Command::PartReference,
    PartReference
);
define_iterator!(iter_lines, iter_lines_mut, Command::Line, Line);
define_iterator!(
    iter_triangles,
    iter_triangles_mut,
    Command::Triangle,
    Triangle
);
define_iterator!(iter_quads, iter_quads_mut, Command::Quad, Quad);
define_iterator!(
    iter_optional_lines,
    iter_optioanl_lines_mut,
    Command::OptionalLine,
    OptionalLine
);

#[derive(Clone, Debug)]
pub struct MultipartDocument {
    pub body: Document,
    pub subparts: HashMap<NormalizedAlias, Document>,
}
