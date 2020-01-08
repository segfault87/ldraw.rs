use std::collections::HashMap;
use std::fmt;
use std::hash::{Hash, Hasher};

use serde::de::{Deserializer, Error as DeError, Visitor};
use serde::ser::Serializer;
use serde::{Deserialize, Serialize};

use crate::Vector4;

#[derive(Clone, Copy, Debug)]
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
        let r = (value & 0x00ff_0000 >> 16) as u8;
        let g = (value & 0x0000_ff00 >> 8) as u8;
        let b = (value & 0x0000_00ff) as u8;
        let a = (value & 0xff00_0000 >> 24) as u8;
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

#[derive(Clone, Debug)]
pub struct MaterialGlitter {
    pub value: Rgba,
    pub luminance: u8,
    pub fraction: f32,
    pub vfraction: f32,
    pub size: u32,
    pub minsize: u32,
    pub maxsize: u32,
}

#[derive(Clone, Debug)]
pub struct MaterialSpeckle {
    pub value: Rgba,
    pub luminance: u8,
    pub fraction: f32,
    pub size: u32,
    pub minsize: u32,
    pub maxsize: u32,
}

#[derive(Clone, Debug)]
pub enum CustomizedMaterial {
    Glitter(MaterialGlitter),
    Speckle(MaterialSpeckle),
}

#[derive(Clone, Debug)]
pub enum Finish {
    Plastic,
    Chrome,
    Pearlescent,
    Rubber,
    MatteMetallic,
    Metal,
    Custom(CustomizedMaterial),
}

#[derive(Clone, Debug)]
pub struct Material {
    pub code: u32,
    pub name: String,
    pub color: Rgba,
    pub edge: Rgba,
    pub luminance: u8,
    pub finish: Finish,
}

impl Material {
    pub fn is_semi_transparent(&self) -> bool {
        self.color.alpha() < 255u8
    }
}

pub type MaterialRegistry = HashMap<u32, Material>;

#[derive(Clone, Debug)]
pub enum ColorReference {
    Unknown(u32),
    Current,
    Complement,
    Material(Material),
}

impl Serialize for ColorReference {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_u32(self.code())
    }
}

struct U32Visitor;

impl<'de> Visitor<'de> for U32Visitor {
    type Value = u32;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("an unsigned 32-bit integer")
    }

    fn visit_u32<E: DeError>(self, value: u32) -> Result<Self::Value, E> {
        Ok(value)
    }
}

impl<'de> Deserialize<'de> for ColorReference {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        // Needs to be resolved later
        Ok(ColorReference::Unknown(
            deserializer.deserialize_u32(U32Visitor)?,
        ))
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
            ColorReference::Material(m) => m.code,
        }
    }

    pub fn is_current(&self) -> bool {
        match self {
            ColorReference::Current => true,
            _ => false,
        }
    }

    pub fn is_complement(&self) -> bool {
        match self {
            ColorReference::Complement => true,
            _ => false,
        }
    }

    pub fn is_material(&self) -> bool {
        match self {
            ColorReference::Material(_) => true,
            _ => false,
        }
    }

    pub fn get_material(&self) -> Option<&Material> {
        match self {
            ColorReference::Material(m) => Some(&m),
            _ => None,
        }
    }

    fn resolve_blended(code: u32, materials: &MaterialRegistry) -> Option<Material> {
        let code1 = code / 16;
        let code2 = code % 16;

        let color1 = match materials.get(&code1) {
            Some(c) => c,
            None => return None,
        };
        let color2 = match materials.get(&code2) {
            Some(c) => c,
            None => return None,
        };

        let new_color = Rgba::new(
            color1.color.red() / 2 + color2.color.red() / 2,
            color1.color.green() / 2 + color2.color.green() / 2,
            color1.color.blue() / 2 + color2.color.blue() / 2,
            255,
        );
        Some(Material {
            code,
            name: format!("Blended Color ({} and {})", code1, code2),
            color: new_color,
            edge: Rgba::from_value(0xff59_5959),
            luminance: 0,
            finish: Finish::Plastic,
        })
    }

    fn resolve_rgb_4(code: u32) -> Material {
        let red = (((code & 0xf00) >> 8) * 16) as u8;
        let green = (((code & 0x0f0) >> 4) * 16) as u8;
        let blue = ((code & 0x00f) * 16) as u8;

        let edge_red = (((code & 0xf0_0000) >> 20) * 16) as u8;
        let edge_green = (((code & 0x0f_0000) >> 16) * 16) as u8;
        let edge_blue = (((code & 0x00_f000) >> 12) * 16) as u8;

        Material {
            code,
            name: format!("RGB Color ({:03x})", code & 0xfff),
            color: Rgba::new(red, green, blue, 255),
            edge: Rgba::new(edge_red, edge_green, edge_blue, 255),
            luminance: 0,
            finish: Finish::Plastic,
        }
    }

    fn resolve_rgb_2(code: u32) -> Material {
        Material {
            code,
            name: format!("RGB Color ({:06x})", code & 0xff_ffff),
            color: Rgba::from_value(0xff00_0000 | (code & 0xff_ffff)),
            edge: Rgba::from_value(0xff59_5959),
            luminance: 0,
            finish: Finish::Plastic,
        }
    }

    pub fn resolve(code: u32, materials: &MaterialRegistry) -> ColorReference {
        match code {
            16 => return ColorReference::Current,
            24 => return ColorReference::Complement,
            _ => (),
        }

        if code >= 256 && code <= 512 {
            if let Some(c) = ColorReference::resolve_blended(code, materials) {
                return ColorReference::Material(c);
            }
        }

        if (code & 0xff00_0000) == 0x0200_0000 {
            return ColorReference::Material(ColorReference::resolve_rgb_2(code));
        } else if (code & 0xff00_0000) == 0x0400_0000 {
            return ColorReference::Material(ColorReference::resolve_rgb_4(code));
        }

        if let Some(c) = materials.get(&code) {
            return ColorReference::Material(c.clone());
        }

        ColorReference::Unknown(code)
    }
}
