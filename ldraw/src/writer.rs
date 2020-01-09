use std::fmt;
use std::io::Write;

use cgmath::{Matrix, Vector4};

use crate::color::ColorReference;
use crate::document::{BfcCertification, Document, MultipartDocument};
use crate::elements::{
    BfcStatement, Command, Header, Line, Meta, OptionalLine, PartReference, Quad, Triangle,
};
use crate::error::SerializeError;
use crate::Winding;

fn serialize_vec3(vec: &Vector4<f32>) -> String {
    format!("{} {} {}", vec.x, vec.y, vec.z)
}

impl fmt::Display for ColorReference {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let code = match self {
            ColorReference::Unknown(code) => code,
            ColorReference::Current => &16u32,
            ColorReference::Complement => &24u32,
            ColorReference::Material(material) => &material.code,
        };
        write!(f, "{}", code)
    }
}

trait LDrawWriter {
    fn write(&self, writer: &mut dyn Write) -> Result<(), SerializeError>;
}

impl LDrawWriter for Header {
    fn write(&self, writer: &mut dyn Write) -> Result<(), SerializeError> {
        writer.write_all(format!("0 !{} {}\n", self.0, self.1).as_bytes())?;
        Ok(())
    }
}

impl LDrawWriter for BfcCertification {
    fn write(&self, writer: &mut dyn Write) -> Result<(), SerializeError> {
        match self {
            BfcCertification::NoCertify => writer.write_all(b"0 BFC NOCERTIFY\n")?,
            BfcCertification::Certify(Winding::Ccw) => writer.write_all(b"0 BFC CERTIFY CCW\n")?,
            BfcCertification::Certify(Winding::Cw) => writer.write_all(b"0 BFC CERTIFY CW\n")?,
            _ => return Err(SerializeError::NoSerializable),
        };
        Ok(())
    }
}

impl LDrawWriter for BfcStatement {
    fn write(&self, writer: &mut dyn Write) -> Result<(), SerializeError> {
        match self {
            BfcStatement::Winding(Winding::Cw) => writer.write_all(b"0 BFC CW\n")?,
            BfcStatement::Winding(Winding::Ccw) => writer.write_all(b"0 BFC CCW\n")?,
            BfcStatement::Clip(None) => writer.write_all(b"0 BFC CLIP\n")?,
            BfcStatement::Clip(Some(Winding::Cw)) => writer.write_all(b"0 BFC CLIP CW\n")?,
            BfcStatement::Clip(Some(Winding::Ccw)) => writer.write_all(b"0 BFC CLIP CW\n")?,
            BfcStatement::NoClip => writer.write_all(b"0 BFC NOCLIP\n")?,
            BfcStatement::InvertNext => writer.write_all(b"0 BFC INVERTNEXT\n")?,
        };
        Ok(())
    }
}

impl LDrawWriter for Document {
    fn write(&self, writer: &mut dyn Write) -> Result<(), SerializeError> {
        writer.write_all(format!("0 {}\n", self.description).as_bytes())?;
        writer.write_all(format!("0 Name: {}\n", self.name).as_bytes())?;
        writer.write_all(format!("0 Author: {}\n", self.author).as_bytes())?;
        for header in &self.headers {
            header.write(writer)?;
        }
        writer.write_all(b"\n")?;
        match self.bfc.write(writer) {
            Ok(()) => {
                writer.write_all(b"\n")?;
            }
            Err(SerializeError::NoSerializable) => {}
            Err(e) => return Err(e),
        };
        for command in &self.commands {
            command.write(writer)?;
        }
        writer.write_all(b"0\n\n")?;

        Ok(())
    }
}

impl LDrawWriter for MultipartDocument {
    fn write(&self, writer: &mut dyn Write) -> Result<(), SerializeError> {
        self.body.write(writer)?;
        for subpart in self.subparts.values() {
            writer.write_all(format!("0 FILE {}\n", subpart.name).as_bytes())?;
            subpart.write(writer)?;
        }

        Ok(())
    }
}

impl LDrawWriter for Meta {
    fn write(&self, writer: &mut dyn Write) -> Result<(), SerializeError> {
        match self {
            Meta::Comment(message) => {
                for line in message.lines() {
                    writer.write_all(format!("0 {}\n", line).as_bytes())?;
                }
            }
            Meta::Step => {
                writer.write_all(b"0 STEP\n")?;
            }
            Meta::Write(message) => {
                for line in message.lines() {
                    writer.write_all(format!("0 WRITE {}\n", line).as_bytes())?;
                }
            }
            Meta::Print(message) => {
                for line in message.lines() {
                    writer.write_all(format!("0 PRINT {}\n", line).as_bytes())?;
                }
            }
            Meta::Clear => {
                writer.write_all(b"0 CLEAR\n")?;
            }
            Meta::Pause => {
                writer.write_all(b"0 PAUSE\n")?;
            }
            Meta::Save => {
                writer.write_all(b"0 SAVE\n")?;
            }
            Meta::Bfc(bfc) => {
                bfc.write(writer)?;
            }
        };

        Ok(())
    }
}

impl LDrawWriter for PartReference {
    fn write(&self, writer: &mut dyn Write) -> Result<(), SerializeError> {
        let m = self.matrix.transpose();
        writer.write_all(
            format!(
                "1 {} {} {} {} {} {} {} {} {} {} {} {} {}\n",
                self.color,
                m.x.w,
                m.y.w,
                m.z.w,
                m.x.x,
                m.x.y,
                m.x.z,
                m.y.x,
                m.y.y,
                m.y.z,
                m.z.x,
                m.z.y,
                m.z.z
            )
            .as_bytes(),
        )?;
        Ok(())
    }
}

impl LDrawWriter for Line {
    fn write(&self, writer: &mut dyn Write) -> Result<(), SerializeError> {
        writer.write_all(
            format!(
                "2 {} {} {}\n",
                self.color,
                serialize_vec3(&self.a),
                serialize_vec3(&self.b)
            )
            .as_bytes(),
        )?;
        Ok(())
    }
}

impl LDrawWriter for Triangle {
    fn write(&self, writer: &mut dyn Write) -> Result<(), SerializeError> {
        writer.write_all(
            format!(
                "2 {} {} {} {}\n",
                self.color,
                serialize_vec3(&self.a),
                serialize_vec3(&self.b),
                serialize_vec3(&self.c)
            )
            .as_bytes(),
        )?;
        Ok(())
    }
}

impl LDrawWriter for Quad {
    fn write(&self, writer: &mut dyn Write) -> Result<(), SerializeError> {
        writer.write_all(
            format!(
                "2 {} {} {} {} {}\n",
                self.color,
                serialize_vec3(&self.a),
                serialize_vec3(&self.b),
                serialize_vec3(&self.c),
                serialize_vec3(&self.d)
            )
            .as_bytes(),
        )?;
        Ok(())
    }
}

impl LDrawWriter for OptionalLine {
    fn write(&self, writer: &mut dyn Write) -> Result<(), SerializeError> {
        writer.write_all(
            format!(
                "2 {} {} {} {} {}\n",
                self.color,
                serialize_vec3(&self.a),
                serialize_vec3(&self.b),
                serialize_vec3(&self.c),
                serialize_vec3(&self.d)
            )
            .as_bytes(),
        )?;
        Ok(())
    }
}

impl LDrawWriter for Command {
    fn write(&self, writer: &mut dyn Write) -> Result<(), SerializeError> {
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
