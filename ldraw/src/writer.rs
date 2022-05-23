use std::fmt;

use async_std::{io::Write, prelude::*};
use async_trait::async_trait;
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
            ColorReference::Color(color) => &color.code,
            ColorReference::Unresolved(code) => code,
        };
        write!(f, "{}", code)
    }
}

#[async_trait]
trait LDrawWriter {
    async fn write(&self, writer: &mut (dyn Write + Unpin + Send)) -> Result<(), SerializeError>;
}

#[async_trait]
impl LDrawWriter for Header {
    async fn write(&self, writer: &mut (dyn Write + Unpin + Send)) -> Result<(), SerializeError> {
        writer
            .write_all(format!("0 !{} {}\n", self.0, self.1).as_bytes())
            .await?;
        Ok(())
    }
}

#[async_trait]
impl LDrawWriter for BfcCertification {
    async fn write(&self, writer: &mut (dyn Write + Unpin + Send)) -> Result<(), SerializeError> {
        match self {
            BfcCertification::NoCertify => writer.write_all(b"0 BFC NOCERTIFY\n").await?,
            BfcCertification::Certify(Winding::Ccw) => {
                writer.write_all(b"0 BFC CERTIFY CCW\n").await?
            }
            BfcCertification::Certify(Winding::Cw) => {
                writer.write_all(b"0 BFC CERTIFY CW\n").await?
            }
            _ => return Err(SerializeError::NoSerializable),
        };
        Ok(())
    }
}

#[async_trait]
impl LDrawWriter for BfcStatement {
    async fn write(&self, writer: &mut (dyn Write + Unpin + Send)) -> Result<(), SerializeError> {
        match self {
            BfcStatement::Winding(Winding::Cw) => writer.write_all(b"0 BFC CW\n").await?,
            BfcStatement::Winding(Winding::Ccw) => writer.write_all(b"0 BFC CCW\n").await?,
            BfcStatement::Clip(None) => writer.write_all(b"0 BFC CLIP\n").await?,
            BfcStatement::Clip(Some(Winding::Cw)) => writer.write_all(b"0 BFC CLIP CW\n").await?,
            BfcStatement::Clip(Some(Winding::Ccw)) => writer.write_all(b"0 BFC CLIP CW\n").await?,
            BfcStatement::NoClip => writer.write_all(b"0 BFC NOCLIP\n").await?,
            BfcStatement::InvertNext => writer.write_all(b"0 BFC INVERTNEXT\n").await?,
        };
        Ok(())
    }
}

#[async_trait]
impl LDrawWriter for Document {
    async fn write(&self, writer: &mut (dyn Write + Unpin + Send)) -> Result<(), SerializeError> {
        writer
            .write_all(format!("0 {}\n", self.description).as_bytes())
            .await?;
        writer
            .write_all(format!("0 Name: {}\n", self.name).as_bytes())
            .await?;
        writer
            .write_all(format!("0 Author: {}\n", self.author).as_bytes())
            .await?;
        for header in &self.headers {
            header.write(writer).await?;
        }
        writer.write_all(b"\n").await?;
        match self.bfc.write(writer).await {
            Ok(()) => {
                writer.write_all(b"\n").await?;
            }
            Err(SerializeError::NoSerializable) => {}
            Err(e) => return Err(e),
        };
        for command in &self.commands {
            command.write(writer).await?;
        }
        writer.write_all(b"0\n\n").await?;

        Ok(())
    }
}

#[async_trait]
impl LDrawWriter for MultipartDocument {
    async fn write(&self, writer: &mut (dyn Write + Unpin + Send)) -> Result<(), SerializeError> {
        self.body.write(writer).await?;
        for subpart in self.subparts.values() {
            writer
                .write_all(format!("0 FILE {}\n", subpart.name).as_bytes())
                .await?;
            subpart.write(writer).await?;
        }

        Ok(())
    }
}

#[async_trait]
impl LDrawWriter for Meta {
    async fn write(&self, writer: &mut (dyn Write + Unpin + Send)) -> Result<(), SerializeError> {
        match self {
            Meta::Comment(message) => {
                for line in message.lines() {
                    writer.write_all(format!("0 {}\n", line).as_bytes()).await?;
                }
            }
            Meta::Step => {
                writer.write_all(b"0 STEP\n").await?;
            }
            Meta::Write(message) => {
                for line in message.lines() {
                    writer
                        .write_all(format!("0 WRITE {}\n", line).as_bytes())
                        .await?;
                }
            }
            Meta::Print(message) => {
                for line in message.lines() {
                    writer
                        .write_all(format!("0 PRINT {}\n", line).as_bytes())
                        .await?;
                }
            }
            Meta::Clear => {
                writer.write_all(b"0 CLEAR\n").await?;
            }
            Meta::Pause => {
                writer.write_all(b"0 PAUSE\n").await?;
            }
            Meta::Save => {
                writer.write_all(b"0 SAVE\n").await?;
            }
            Meta::Bfc(bfc) => {
                bfc.write(writer).await?;
            }
        };

        Ok(())
    }
}

#[async_trait]
impl LDrawWriter for PartReference {
    async fn write(&self, writer: &mut (dyn Write + Unpin + Send)) -> Result<(), SerializeError> {
        let m = self.matrix.transpose();
        writer
            .write_all(
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
            )
            .await?;
        Ok(())
    }
}

#[async_trait]
impl LDrawWriter for Line {
    async fn write(&self, writer: &mut (dyn Write + Unpin + Send)) -> Result<(), SerializeError> {
        writer
            .write_all(
                format!(
                    "2 {} {} {}\n",
                    self.color,
                    serialize_vec3(&self.a),
                    serialize_vec3(&self.b)
                )
                .as_bytes(),
            )
            .await?;
        Ok(())
    }
}

#[async_trait]
impl LDrawWriter for Triangle {
    async fn write(&self, writer: &mut (dyn Write + Unpin + Send)) -> Result<(), SerializeError> {
        writer
            .write_all(
                format!(
                    "2 {} {} {} {}\n",
                    self.color,
                    serialize_vec3(&self.a),
                    serialize_vec3(&self.b),
                    serialize_vec3(&self.c)
                )
                .as_bytes(),
            )
            .await?;
        Ok(())
    }
}

#[async_trait]
impl LDrawWriter for Quad {
    async fn write(&self, writer: &mut (dyn Write + Unpin + Send)) -> Result<(), SerializeError> {
        writer
            .write_all(
                format!(
                    "2 {} {} {} {} {}\n",
                    self.color,
                    serialize_vec3(&self.a),
                    serialize_vec3(&self.b),
                    serialize_vec3(&self.c),
                    serialize_vec3(&self.d)
                )
                .as_bytes(),
            )
            .await?;
        Ok(())
    }
}

#[async_trait]
impl LDrawWriter for OptionalLine {
    async fn write(&self, writer: &mut (dyn Write + Unpin + Send)) -> Result<(), SerializeError> {
        writer
            .write_all(
                format!(
                    "2 {} {} {} {} {}\n",
                    self.color,
                    serialize_vec3(&self.a),
                    serialize_vec3(&self.b),
                    serialize_vec3(&self.c),
                    serialize_vec3(&self.d)
                )
                .as_bytes(),
            )
            .await?;
        Ok(())
    }
}

#[async_trait]
impl LDrawWriter for Command {
    async fn write(&self, writer: &mut (dyn Write + Unpin + Send)) -> Result<(), SerializeError> {
        match self {
            Command::Meta(meta) => meta.write(writer).await,
            Command::PartReference(ref_) => ref_.write(writer).await,
            Command::Line(line) => line.write(writer).await,
            Command::Triangle(triangle) => triangle.write(writer).await,
            Command::Quad(quad) => quad.write(writer).await,
            Command::OptionalLine(optional_line) => optional_line.write(writer).await,
        }
    }
}
