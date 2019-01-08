use std::collections::HashMap;

use cgmath::Vector4;

#[derive(Clone, Copy, Debug)]
pub struct Rgba {
    value: [u8; 4],
    vec: Vector4<f32>,
}

impl Rgba {
    pub fn new(r: u8, g: u8, b: u8, a: u8) -> Rgba {
        Rgba {
            value: [r, g, b, a],
            vec: Vector4::new(
                r as f32 / 255.0,
                g as f32 / 255.0,
                b as f32 / 255.0,
                a as f32 / 255.0,
            ),
        }
    }

    pub fn from_value(value: u32) -> Rgba {
        let r = (value & 0x00ff0000 >> 16) as u8;
        let g = (value & 0x0000ff00 >> 8) as u8;
        let b = (value & 0x000000ff) as u8;
        let a = (value & 0xff000000 >> 24) as u8;
        Rgba {
            value: [r, g, b, a],
            vec: Vector4::new(
                r as f32 / 255.0,
                g as f32 / 255.0,
                b as f32 / 255.0,
                a as f32 / 255.0,
            ),
        }
    }

    pub fn red(&self) -> u8 {
        self.value[0]
    }

    pub fn green(&self) -> u8 {
        self.value[1]
    }

    pub fn blue(&self) -> u8 {
        self.value[2]
    }

    pub fn alpha(&self) -> u8 {
        self.value[3]
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

pub type MaterialRegistry = HashMap<u32, Material>;

#[derive(Clone, Debug)]
pub enum ColorReference<'a> {
    Unknown(u32),
    Current,
    Complement,
    PredefinedMaterial(&'a Material),
    CustomMaterial(Material),
}

impl<'a> ColorReference<'a> {
    pub fn code(&self) -> u32 {
        match self {
            ColorReference::Unknown(c) => c.clone(),
            ColorReference::Current => 16,
            ColorReference::Complement => 24,
            ColorReference::PredefinedMaterial(m) => m.code.clone(),
            ColorReference::CustomMaterial(m) => m.code.clone(),
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
            code: code,
            name: format!("Blended Color ({} and {})", code1, code2),
            color: new_color,
            edge: Rgba::from_value(0xff595959),
            luminance: 0,
            finish: Finish::Plastic,
        })
    }

    fn resolve_rgb_4(code: u32) -> Material {
        let red = (((code & 0xf00) >> 8) * 16) as u8;
        let green = (((code & 0x0f0) >> 4) * 16) as u8;
        let blue = ((code & 0x00f) * 16) as u8;

        let edge_red = (((code & 0xf00000) >> 20) * 16) as u8;
        let edge_green = (((code & 0x0f0000) >> 16) * 16) as u8;
        let edge_blue = (((code & 0x00f000) >> 12) * 16) as u8;

        Material {
            code: code,
            name: format!("RGB Color ({:03x})", code & 0xfff),
            color: Rgba::new(red, green, blue, 255),
            edge: Rgba::new(edge_red, edge_green, edge_blue, 255),
            luminance: 0,
            finish: Finish::Plastic,
        }
    }

    fn resolve_rgb_2(code: u32) -> Material {
        Material {
            code: code,
            name: format!("RGB Color ({:06x})", code & 0xffffff),
            color: Rgba::from_value(0xff000000 | (code & 0xffffff)),
            edge: Rgba::from_value(0xff595959),
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
                return ColorReference::CustomMaterial(c);
            }
        }

        if (code & 0xff000000) == 0x02000000 {
            return ColorReference::CustomMaterial(ColorReference::resolve_rgb_2(code));
        } else if (code & 0xff000000) == 0x04000000 {
            return ColorReference::CustomMaterial(ColorReference::resolve_rgb_4(code));
        }

        if let Some(c) = materials.get(&code) {
            return ColorReference::PredefinedMaterial(c);
        }

        ColorReference::Unknown(code)
    }
}
