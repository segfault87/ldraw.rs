use std::collections::HashMap;
use std::vec::Vec;

use crate::NormalizedAlias;
use crate::elements::{Command, Header};

#[derive(Debug)]
pub enum BfcCertification {
    NotApplicable,
    NoCertify,
    CertifyCcw,
    CertifyCw,
}

#[derive(Debug)]
pub struct Document<'a> {
    pub name: String,
    pub description: String,
    pub author: String,
    pub bfc: BfcCertification,
    pub headers: Vec<Header>,
    pub commands: Vec<Command<'a>>,
}

#[derive(Debug)]
pub struct MultipartDocument<'a> {
    pub body: Document<'a>,
    pub subparts: HashMap<NormalizedAlias, Document<'a>>,
}
