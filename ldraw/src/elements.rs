use cgmath::{Matrix4, Vector3};

use crate::color::ColorReference;
use crate::document::Document;

#[derive(Debug)]
pub struct Header(pub String, pub String);

#[derive(Debug)]
pub enum BfcStatement {
    Cw,
    Ccw,
    Clip,
    ClipCw,
    ClipCcw,
    NoClip,
    InvertNext,
}

#[derive(Debug)]
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

#[derive(Debug)]
pub enum PartResolution<'a> {
    Unresolved,
    Missing,
    External(&'a Document<'a>),
    Subpart(&'a Document<'a>),
}

#[derive(Debug)]
pub struct PartReference<'a> {
    pub color: ColorReference<'a>,
    pub matrix: Matrix4<f32>,
    pub name: String,
}

#[derive(Debug)]
pub struct Line<'a> {
    pub color: ColorReference<'a>,
    pub a: Vector3<f32>,
    pub b: Vector3<f32>,
}

#[derive(Debug)]
pub struct Triangle<'a> {
    pub color: ColorReference<'a>,
    pub a: Vector3<f32>,
    pub b: Vector3<f32>,
    pub c: Vector3<f32>,
}

#[derive(Debug)]
pub struct Quad<'a> {
    pub color: ColorReference<'a>,
    pub a: Vector3<f32>,
    pub b: Vector3<f32>,
    pub c: Vector3<f32>,
    pub d: Vector3<f32>,
}

#[derive(Debug)]
pub struct OptionalLine<'a> {
    pub color: ColorReference<'a>,
    pub a: Vector3<f32>,
    pub b: Vector3<f32>,
    pub c: Vector3<f32>,
    pub d: Vector3<f32>,
}

#[derive(Debug)]
pub enum Command<'a> {
    Meta(Meta),
    PartReference(PartReference<'a>),
    Line(Line<'a>),
    Triangle(Triangle<'a>),
    Quad(Quad<'a>),
    OptionalLine(OptionalLine<'a>),
}
