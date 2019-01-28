use std::collections::HashMap;
use std::iter::Iterator;
use std::rc::Rc;
use std::vec::Vec;

use crate::elements::{Command, Header, Line, Meta, OptionalLine, PartReference, Quad, Triangle};
use crate::NormalizedAlias;

#[derive(Clone, Debug)]
pub enum BfcCertification {
    NotApplicable,
    NoCertify,
    CertifyCcw,
    CertifyCw,
}

impl BfcCertification {
    pub fn is_certified(&self) -> bool {
        match self {
            BfcCertification::CertifyCw | BfcCertification::CertifyCcw => true,
            _ => false,
        }
    }

    pub fn is_ccw(&self) -> bool {
        match self {
            BfcCertification::CertifyCcw => true,
            _ => false,
        }
    }

    pub fn is_cw(&self) -> bool {
        match self {
            BfcCertification::CertifyCw => true,
            _ => false,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Document<'a> {
    pub name: String,
    pub description: String,
    pub author: String,
    pub bfc: BfcCertification,
    pub headers: Vec<Header>,
    pub commands: Vec<Command<'a>>,
}

macro_rules! define_iterator(
    ($fn:ident, $fn_mut:ident, $cmdval:path, $type:ty) => (
        impl<'a> Document<'a> {
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
define_iterator!(iter_refs, iter_refs_mut, Command::PartReference, PartReference<'a>);
define_iterator!(iter_lines, iter_lines_mut, Command::Line, Line<'a>);
define_iterator!(iter_triangles, iter_triangles_mut, Command::Triangle, Triangle<'a>);
define_iterator!(iter_quads, iter_quads_mut, Command::Quad, Quad<'a>);
define_iterator!(iter_optional_lines, iter_optioanl_lines_mut, Command::OptionalLine, OptionalLine<'a>);

#[derive(Debug)]
pub struct MultipartDocument<'a> {
    pub body: Rc<Document<'a>>,
    pub subparts: HashMap<NormalizedAlias, Rc<Document<'a>>>,
}

impl<'a> MultipartDocument<'a> {
    pub fn query(&'a self, alias: &NormalizedAlias) -> Option<Rc<Document<'a>>> {
        match self.subparts.get(alias) {
            Some(e) => Some(Rc::clone(&e)),
            None => None,
        }
    }
}
