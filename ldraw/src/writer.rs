use std::fmt;
use std::io::Write;

use cgmath::Vector3;

use crate::color::ColorReference;
use crate::document::{BfcCertification, Document, MultipartDocument};
use crate::elements::{BfcStatement, Header, Command, Line, Meta, OptionalLine,
                      PartReference, Quad, Triangle};
use crate::error::SerializeError;

fn serialize_vec3(vec: &Vector3<f32>) -> String {
    format!("{} {} {}", vec.x, vec.y, vec.z)
}

impl<'a> fmt::Display for ColorReference<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let code = match self {
            ColorReference::Unknown(code) => code.clone(),
            ColorReference::Current => 16,
            ColorReference::Complement => 24,
            ColorReference::PredefinedMaterial(material) => material.code.clone(),
            ColorReference::CustomMaterial(material) => material.code.clone(),
        };
        write!(f, "{}", code)
    }
}

trait LDrawWriter {
    fn write(&self, writer: &mut Write) -> Result<(), SerializeError>;
}

impl LDrawWriter for Header {
    fn write(&self, writer: &mut Write) -> Result<(), SerializeError> {
        writer.write(format!("0 !{} {}\n", self.0, self.1).as_bytes())?;
        Ok(())
    }
}

impl LDrawWriter for BfcCertification {
    fn write(&self, writer: &mut Write) -> Result<(), SerializeError> {
        match self {
            BfcCertification::NoCertify => writer.write("0 BFC NOCERTIFY\n".as_bytes())?,
            BfcCertification::CertifyCcw => writer.write("0 BFC CERTIFY CCW\n".as_bytes())?,
            BfcCertification::CertifyCw => writer.write("0 BFC CERTIFY CW\n".as_bytes())?,
            _ => return Err(SerializeError::NoSerializable),
        };
        Ok(())
    }
}

impl LDrawWriter for BfcStatement {
    fn write(&self, writer: &mut Write) -> Result<(), SerializeError> {
        match self {
            BfcStatement::Cw => writer.write("0 BFC CW\n".as_bytes())?,
            BfcStatement::Ccw => writer.write("0 BFC CCW\n".as_bytes())?,
            BfcStatement::Clip => writer.write("0 BFC CLIP\n".as_bytes())?,
            BfcStatement::ClipCw => writer.write("0 BFC CLIP CW\n".as_bytes())?,
            BfcStatement::ClipCcw => writer.write("0 BFC CLIP CW\n".as_bytes())?,
            BfcStatement::NoClip => writer.write("0 BFC NOCLIP\n".as_bytes())?,
            BfcStatement::InvertNext => writer.write("0 BFC INVERTNEXT\n".as_bytes())?,
        };
        Ok(())
    }
}

impl<'a> LDrawWriter for Document<'a> {
    fn write(&self, writer: &mut Write) -> Result<(), SerializeError> {
        writer.write(format!("0 {}\n", self.description).as_bytes())?;
        writer.write(format!("0 Name: {}\n", self.name).as_bytes())?;
        writer.write(format!("0 Author: {}\n", self.author).as_bytes())?;
        for header in &self.headers {
            header.write(writer)?;
        }
        writer.write("\n".as_bytes())?;
        match self.bfc.write(writer) {
            Ok(()) => {
                writer.write("\n".as_bytes())?;
            },
            Err(SerializeError::NoSerializable) => {},
            Err(e) => return Err(e),
        };
        for command in &self.commands {
            command.write(writer)?;
        }
        writer.write("0\n\n".as_bytes())?;
        
        Ok(())
    }
}

impl<'a> LDrawWriter for MultipartDocument<'a> {
    fn write(&self, writer: &mut Write) -> Result<(), SerializeError> {
        self.body.write(writer)?;
        for subpart in self.subparts.values() {
            writer.write(format!("0 FILE {}\n", subpart.name).as_bytes())?;
            subpart.write(writer)?;
        }

        Ok(())
    }
}

impl LDrawWriter for Meta {
    fn write(&self, writer: &mut Write) -> Result<(), SerializeError> {
        match self {
            Meta::Comment(message) => {
                for line in message.lines() {
                    writer.write(format!("0 {}\n", line).as_bytes())?;
                }
            },
            Meta::Step => {
                writer.write("0 STEP\n".as_bytes())?;
            },
            Meta::Write(message) => {
                for line in message.lines() {
                    writer.write(format!("0 WRITE {}\n", line).as_bytes())?;
                }
            },
            Meta::Print(message) => {
                for line in message.lines() {
                    writer.write(format!("0 PRINT {}\n", line).as_bytes())?;
                }
            },
            Meta::Clear => {
                writer.write("0 CLEAR\n".as_bytes())?;
            },
            Meta::Pause => {
                writer.write("0 PAUSE\n".as_bytes())?;
            },
            Meta::Save => {
                writer.write("0 SAVE\n".as_bytes())?;
            },
            Meta::Bfc(bfc) => {
                bfc.write(writer)?;
            },
        };

        Ok(())
    }
}

impl<'a> LDrawWriter for PartReference<'a> {
    fn write(&self, writer: &mut Write) -> Result<(), SerializeError> {
        let m = &self.matrix;
        writer.write(format!("1 {} {} {} {} {} {} {} {} {} {} {} {} {}\n",
                             self.color,
                             m.x.w, m.y.w, m.z.w,
                             m.x.x, m.x.y, m.x.z,
                             m.y.x, m.y.y, m.y.z,
                             m.z.x, m.z.y, m.z.z).as_bytes())?;
        Ok(())
    }
}

impl<'a> LDrawWriter for Line<'a> {
    fn write(&self, writer: &mut Write) -> Result<(), SerializeError> {
        writer.write(format!("2 {} {} {}\n",
                             self.color,
                             serialize_vec3(&self.a), serialize_vec3(&self.b)).as_bytes())?;
        Ok(())
    }
}

impl<'a> LDrawWriter for Triangle<'a> {
    fn write(&self, writer: &mut Write) -> Result<(), SerializeError> {
        writer.write(format!("2 {} {} {} {}\n",
                             self.color,
                             serialize_vec3(&self.a), serialize_vec3(&self.b),
                             serialize_vec3(&self.c)).as_bytes())?;
        Ok(())
    }
}

impl<'a> LDrawWriter for Quad<'a> {
    fn write(&self, writer: &mut Write) -> Result<(), SerializeError> {
        writer.write(format!("2 {} {} {} {} {}\n",
                             self.color,
                             serialize_vec3(&self.a), serialize_vec3(&self.b),
                             serialize_vec3(&self.c), serialize_vec3(&self.d)).as_bytes())?;
        Ok(())
    }
}

impl<'a> LDrawWriter for OptionalLine<'a> {
    fn write(&self, writer: &mut Write) -> Result<(), SerializeError> {
        writer.write(format!("2 {} {} {} {} {}\n",
                             self.color,
                             serialize_vec3(&self.a), serialize_vec3(&self.b),
                             serialize_vec3(&self.c), serialize_vec3(&self.d)).as_bytes())?;
        Ok(())
    }
}

impl<'a> LDrawWriter for Command<'a> {
    fn write(&self, writer: &mut Write) -> Result<(), SerializeError> {
        match self {
            Command::Meta(meta) => meta.write(writer),
            Command::PartReference(ref_) => ref_.write(writer),
            Command::Line(line) => line.write(writer),
            Command::Triangle(triangle) => triangle.write(writer),
            Command::Quad(quad) => quad.write(writer),
            Command::OptionalLine(optional_line) => optional_line.write(writer),
        }
    }
}

