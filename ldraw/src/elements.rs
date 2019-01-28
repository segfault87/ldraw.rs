use std::rc::Rc;

use crate::{Matrix4, NormalizedAlias, Vector4};
use crate::color::ColorReference;
use crate::document::Document;

#[derive(Clone, Debug)]
pub struct Header(pub String, pub String);

#[derive(Clone, Debug)]
pub enum BfcStatement {
    Cw,
    Ccw,
    Clip,
    ClipCw,
    ClipCcw,
    NoClip,
    InvertNext,
}

#[derive(Clone, Debug)]
pub enum Meta {
    Comment(String),
    Step,
    Write(String),
    Print(String),
    Clear,
    Pause,
    Save,
    Bfc(BfcStatement),
}

#[derive(Clone, Debug)]
pub enum PartResolution<'a> {
    Unresolved,
    Missing,
    External(Rc<Document<'a>>),
    Subpart(Rc<Document<'a>>),
}

#[derive(Clone, Debug)]
pub struct PartReference<'a> {
    pub color: ColorReference<'a>,
    pub matrix: Matrix4,
    pub name: NormalizedAlias,
}

#[derive(Clone, Debug)]
pub struct Line<'a> {
    pub color: ColorReference<'a>,
    pub a: Vector4,
    pub b: Vector4,
}

#[derive(Clone, Debug)]
pub struct Triangle<'a> {
    pub color: ColorReference<'a>,
    pub a: Vector4,
    pub b: Vector4,
    pub c: Vector4,
}

#[derive(Clone, Debug)]
pub struct Quad<'a> {
    pub color: ColorReference<'a>,
    pub a: Vector4,
    pub b: Vector4,
    pub c: Vector4,
    pub d: Vector4,
}

#[derive(Clone, Debug)]
pub struct OptionalLine<'a> {
    pub color: ColorReference<'a>,
    pub a: Vector4,
    pub b: Vector4,
    pub c: Vector4,
    pub d: Vector4,
}

#[derive(Clone, Debug)]
pub enum Command<'a> {
    Meta(Meta),
    PartReference(PartReference<'a>),
    Line(Line<'a>),
    Triangle(Triangle<'a>),
    Quad(Quad<'a>),
    OptionalLine(OptionalLine<'a>),
}
