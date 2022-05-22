use std::collections::HashMap;
use std::fmt;
use std::hash::{Hash, Hasher};

use serde::de::{Deserializer, Error as DeError, Visitor};
use serde::ser::Serializer;
use serde::{Deserialize, Serialize};

use crate::Vector4;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rgba {
    value: [u8; 4],
}

impl Rgba {
    pub fn new(r: u8, g: u8, b: u8, a: u8) -> Rgba {
        Rgba {
            value: [r, g, b, a],
        }
    }

    pub fn from_value(value: u32) -> Rgba {
        let r = ((value & 0x00ff_0000) >> 16) as u8;
        let g = ((value & 0x0000_ff00) >> 8) as u8;
        let b = (value & 0x0000_00ff) as u8;
        let a = ((value & 0xff00_0000) >> 24) as u8;
        Rgba {
            value: [r, g, b, a],
        }
    }

    pub fn red(self) -> u8 {
        self.value[0]
    }

    pub fn green(self) -> u8 {
        self.value[1]
    }

    pub fn blue(self) -> u8 {
        self.value[2]
    }

    pub fn alpha(self) -> u8 {
        self.value[3]
    }
}

impl From<&Rgba> for Vector4 {
    fn from(src: &Rgba) -> Vector4 {
        Vector4::new(
            f32::from(src.red()) / 255.0,
            f32::from(src.green()) / 255.0,
            f32::from(src.blue()) / 255.0,
            f32::from(src.alpha()) / 255.0,
        )
    }
}

impl From<Rgba> for Vector4 {
    fn from(src: Rgba) -> Vector4 {
        Vector4::new(
            f32::from(src.red()) / 255.0,
            f32::from(src.green()) / 255.0,
            f32::from(src.blue()) / 255.0,
            f32::from(src.alpha()) / 255.0,
        )
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct MaterialGlitter {
    pub value: Rgba,
    pub luminance: u8,
    pub fraction: f32,
    pub vfraction: f32,
    pub size: u32,
    pub minsize: f32,
    pub maxsize: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MaterialSpeckle {
    pub value: Rgba,
    pub luminance: u8,
    pub fraction: f32,
    pub size: u32,
    pub minsize: f32,
    pub maxsize: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub enum CustomizedMaterial {
    Glitter(MaterialGlitter),
    Speckle(MaterialSpeckle),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Material {
    Plastic,
    Chrome,
    Pearlescent,
    Rubber,
    MatteMetallic,
    Metal,
    Custom(CustomizedMaterial),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Color {
    pub code: u32,
    pub name: String,
    pub color: Rgba,
    pub edge: Rgba,
    pub luminance: u8,
    pub material: Material,
}

impl Default for Color {
    fn default() -> Self {
        Color {
            code: 0,
            name: String::from("Black"),
            color: Rgba::new(0x05, 0x13, 0x1d, 0xff),
            edge: Rgba::new(0x59, 0x59, 0x59, 0xff),
            luminance: 0x00,
            material: Material::Plastic,
        }
    }
}

impl Color {
    pub fn is_translucent(&self) -> bool {
        self.color.alpha() < 255u8
    }
}

pub type ColorCatalog = HashMap<u32, Color>;

#[derive(Clone, Debug)]
pub enum ColorReference {
    Unknown(u32),
    Current,
    Complement,
    Color(Color),
    Unresolved(u32),
}

impl Serialize for ColorReference {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_u32(self.code())
    }
}

impl<'de> Deserialize<'de> for ColorReference {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct ColorReferenceVisitor;

        impl<'de> Visitor<'de> for ColorReferenceVisitor {
            type Value = ColorReference;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("an unsigned 32-bit integer")
            }

            fn visit_u64<E: DeError>(self, value: u64) -> Result<Self::Value, E> {
                Ok(ColorReference::Unresolved(value as u32))
            }

            fn visit_u32<E: DeError>(self, value: u32) -> Result<Self::Value, E> {
                Ok(ColorReference::Unresolved(value))
            }
        }

        // Needs to be resolved later
        Ok(deserializer.deserialize_u32(ColorReferenceVisitor).unwrap())
    }
}

impl Eq for ColorReference {}

impl PartialEq for ColorReference {
    fn eq(&self, other: &Self) -> bool {
        self.code() == other.code()
    }
}

impl Hash for ColorReference {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.code().hash(state)
    }
}

impl ColorReference {
    pub fn code(&self) -> u32 {
        match self {
            ColorReference::Unknown(c) => *c,
            ColorReference::Current => 16,
            ColorReference::Complement => 24,
            ColorReference::Color(m) => m.code,
            ColorReference::Unresolved(c) => *c,
        }
    }

    pub fn is_current(&self) -> bool {
        matches!(self, ColorReference::Current)
    }

    pub fn is_complement(&self) -> bool {
        matches!(self, ColorReference::Complement)
    }

    pub fn is_color(&self) -> bool {
        matches!(self, ColorReference::Color(_))
    }

    pub fn get_color(&self) -> Option<&Color> {
        match self {
            ColorReference::Color(m) => Some(m),
            _ => None,
        }
    }

    fn resolve_blended(code: u32, colors: &ColorCatalog) -> Option<Color> {
        let code1 = code / 16;
        let code2 = code % 16;

        let color1 = match colors.get(&code1) {
            Some(c) => c,
            None => return None,
        };
        let color2 = match colors.get(&code2) {
            Some(c) => c,
            None => return None,
        };

        let new_color = Rgba::new(
            color1.color.red() / 2 + color2.color.red() / 2,
            color1.color.green() / 2 + color2.color.green() / 2,
            color1.color.blue() / 2 + color2.color.blue() / 2,
            255,
        );
        Some(Color {
            code,
            name: format!("Blended Color ({} and {})", code1, code2),
            color: new_color,
            edge: Rgba::from_value(0xff59_5959),
            luminance: 0,
            material: Material::Plastic,
        })
    }

    fn resolve_rgb_4(code: u32) -> Color {
        let red = (((code & 0xf00) >> 8) * 16) as u8;
        let green = (((code & 0x0f0) >> 4) * 16) as u8;
        let blue = ((code & 0x00f) * 16) as u8;

        let edge_red = (((code & 0xf0_0000) >> 20) * 16) as u8;
        let edge_green = (((code & 0x0f_0000) >> 16) * 16) as u8;
        let edge_blue = (((code & 0x00_f000) >> 12) * 16) as u8;

        Color {
            code,
            name: format!("RGB Color ({:03x})", code & 0xfff),
            color: Rgba::new(red, green, blue, 255),
            edge: Rgba::new(edge_red, edge_green, edge_blue, 255),
            luminance: 0,
            material: Material::Plastic,
        }
    }

    fn resolve_rgb_2(code: u32) -> Color {
        Color {
            code,
            name: format!("RGB Color ({:06x})", code & 0xff_ffff),
            color: Rgba::from_value(0xff00_0000 | (code & 0xff_ffff)),
            edge: Rgba::from_value(0xff59_5959),
            luminance: 0,
            material: Material::Plastic,
        }
    }

    pub fn resolve(code: u32, colors: &ColorCatalog) -> ColorReference {
        match code {
            16 => return ColorReference::Current,
            24 => return ColorReference::Complement,
            _ => (),
        }

        if let Some(c) = colors.get(&code) {
            return ColorReference::Color(c.clone());
        }

        if (256..=512).contains(&code) {
            if let Some(c) = ColorReference::resolve_blended(code, colors) {
                return ColorReference::Color(c);
            }
        }

        if (code & 0xff00_0000) == 0x0200_0000 {
            return ColorReference::Color(ColorReference::resolve_rgb_2(code));
        } else if (code & 0xff00_0000) == 0x0400_0000 {
            return ColorReference::Color(ColorReference::resolve_rgb_4(code));
        }

        ColorReference::Unknown(code)
    }

    pub fn resolve_self(&mut self, colors: &ColorCatalog) {
        if let ColorReference::Unresolved(code) = self {
            *self = ColorReference::resolve(*code, colors);
        }
    }

    pub fn get_color_rgba(&self) -> Option<Vector4> {
        match self {
            ColorReference::Color(c) => Some(c.color.into()),
            _ => None,
        }
    }

    pub fn get_edge_color_rgba(&self) -> Option<Vector4> {
        match self {
            ColorReference::Color(m) => Some(m.edge.into()),
            _ => None,
        }
    }
}
