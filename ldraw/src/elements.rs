use std::rc::Rc;

use crate::color::ColorReference;
use crate::document::Document;
use crate::{Matrix4, NormalizedAlias, Vector4, Winding};

#[derive(Clone, Debug)]
pub struct Header(pub String, pub String);

#[derive(Clone, Debug)]
pub enum BfcStatement {
    Winding(Winding),
    Clip(Option<Winding>),
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
pub struct PartReference {
    pub color: ColorReference,
    pub matrix: Matrix4,
    pub name: NormalizedAlias,
}

#[derive(Clone, Debug)]
pub struct Line {
    pub color: ColorReference,
    pub a: Vector4,
    pub b: Vector4,
}

#[derive(Clone, Debug)]
pub struct Triangle {
    pub color: ColorReference,
    pub a: Vector4,
    pub b: Vector4,
    pub c: Vector4,
}

#[derive(Clone, Debug)]
pub struct Quad {
    pub color: ColorReference,
    pub a: Vector4,
    pub b: Vector4,
    pub c: Vector4,
    pub d: Vector4,
}

#[derive(Clone, Debug)]
pub struct OptionalLine {
    pub color: ColorReference,
    pub a: Vector4,
    pub b: Vector4,
    pub c: Vector4,
    pub d: Vector4,
}

#[derive(Clone, Debug)]
pub enum Command {
    Meta(Meta),
    PartReference(PartReference),
    Line(Line),
    Triangle(Triangle),
    Quad(Quad),
    OptionalLine(OptionalLine),
}
