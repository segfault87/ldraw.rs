use std::{collections::HashMap, marker::Unpin, str::Chars};

use async_std::io::BufRead;
use cgmath::Matrix;
use futures::{io::Lines, stream::Enumerate, AsyncBufReadExt, StreamExt};

use crate::{
    color::{
        ColorReference, CustomizedMaterial, Finish, Material, MaterialGlitter, MaterialRegistry,
        MaterialSpeckle, Rgba,
    },
    document::{BfcCertification, Document, MultipartDocument},
    elements::{
        BfcStatement, Command, Header, Line, Meta, OptionalLine, PartReference, Quad, Triangle,
    },
    error::{ColorDefinitionParseError, DocumentParseError, ParseError},
    {Matrix4, PartAlias, Vector4, Winding},
};

#[derive(Debug, PartialEq)]
enum Line0 {
    Header(Header),
    Meta(Meta),
    File(String),
    Name(String),
    Author(String),
    BfcCertification(BfcCertification),
}

fn is_whitespace(ch: char) -> bool {
    matches!(ch, ' ' | '\t' | '\r' | '\n')
}

fn next_token(iterator: &mut Chars, glob_remaining: bool) -> Result<String, ParseError> {
    let mut buffer = String::new();
    for v in iterator {
        if !is_whitespace(v) {
            buffer.push(v);
        } else if !buffer.is_empty() {
            if !glob_remaining {
                break;
            } else {
                buffer.push(v);
            }
        }
    }

    match buffer.len() {
        0 => Err(ParseError::EndOfLine),
        _ => Ok(buffer.trim_end().to_string()),
    }
}

fn next_token_u32(iterator: &mut Chars) -> Result<u32, ParseError> {
    let token = next_token(iterator, false)?;
    if token.starts_with("0x") {
        let trimmed = token.chars().skip(2).collect::<String>();
        return match u32::from_str_radix(trimmed.as_str(), 16) {
            Ok(v) => Ok(v),
            Err(_) => Err(ParseError::TypeMismatch("u32", token)),
        };
    }
    match token.parse::<u32>() {
        Ok(v) => Ok(v),
        Err(_) => Err(ParseError::TypeMismatch("u32", token)),
    }
}

fn next_token_f32(iterator: &mut Chars) -> Result<f32, ParseError> {
    let token = next_token(iterator, false)?;
    match token.parse::<f32>() {
        Ok(v) => Ok(v),
        Err(_) => Err(ParseError::TypeMismatch("f32", token)),
    }
}

fn next_token_rgb(iterator: &mut Chars) -> Result<(u8, u8, u8), ParseError> {
    match iterator.next() {
        Some(v) => {
            if v != '#' {
                return Err(ParseError::InvalidToken(v.to_string()));
            }
        }
        None => {
            return Err(ParseError::EndOfLine);
        }
    }

    let rs = iterator.take(2).collect::<String>();
    let gs = iterator.take(2).collect::<String>();
    let bs = iterator.take(2).collect::<String>();

    let r = match u8::from_str_radix(rs.as_str(), 16) {
        Ok(v) => v,
        Err(_) => return Err(ParseError::TypeMismatch("u8", rs)),
    };
    let g = match u8::from_str_radix(gs.as_str(), 16) {
        Ok(v) => v,
        Err(_) => return Err(ParseError::TypeMismatch("u8", gs)),
    };
    let b = match u8::from_str_radix(bs.as_str(), 16) {
        Ok(v) => v,
        Err(_) => return Err(ParseError::TypeMismatch("u8", bs)),
    };

    Ok((r, g, b))
}

fn parse_bfc_statement(iterator: &mut Chars) -> Result<Line0, ParseError> {
    let stmt = next_token(iterator, true)?;
    match stmt.as_str() {
        "NOCERTIFY" => Ok(Line0::BfcCertification(BfcCertification::NoCertify)),
        "CERTIFY" | "CERTIFY CCW" => Ok(Line0::BfcCertification(BfcCertification::Certify(
            Winding::Ccw,
        ))),
        "CERTIFY CW" => Ok(Line0::BfcCertification(BfcCertification::Certify(
            Winding::Cw,
        ))),
        "CW" => Ok(Line0::Meta(Meta::Bfc(BfcStatement::Winding(Winding::Cw)))),
        "CCW" => Ok(Line0::Meta(Meta::Bfc(BfcStatement::Winding(Winding::Ccw)))),
        "CLIP" => Ok(Line0::Meta(Meta::Bfc(BfcStatement::Clip(None)))),
        "CLIP CW" => Ok(Line0::Meta(Meta::Bfc(BfcStatement::Clip(Some(
            Winding::Cw,
        ))))),
        "CLIP CCW" => Ok(Line0::Meta(Meta::Bfc(BfcStatement::Clip(Some(
            Winding::Ccw,
        ))))),
        "NOCLIP" => Ok(Line0::Meta(Meta::Bfc(BfcStatement::NoClip))),
        "INVERTNEXT" => Ok(Line0::Meta(Meta::Bfc(BfcStatement::InvertNext))),
        _ => Err(ParseError::InvalidBfcStatement(stmt)),
    }
}

fn parse_line_0(iterator: &mut Chars) -> Result<Line0, ParseError> {
    let text = match next_token(iterator, true) {
        Ok(v) => v,
        Err(ParseError::EndOfLine) => return Ok(Line0::Meta(Meta::Comment(String::new()))),
        Err(e) => return Err(e),
    };
    let mut inner_iterator = text.chars();
    let cmd = next_token(&mut inner_iterator, false)?;

    if cmd.starts_with('!') {
        let key: String = cmd.chars().skip(1).collect();
        let value = next_token(&mut inner_iterator, true)?;
        return Ok(Line0::Header(Header(key, value)));
    }

    match cmd.as_str() {
        "BFC" => parse_bfc_statement(&mut inner_iterator),
        "Name:" => match next_token(&mut inner_iterator, true) {
            Ok(msg) => Ok(Line0::Name(msg)),
            Err(_) => Ok(Line0::Name(String::from(""))),
        },
        "Author:" => match next_token(&mut inner_iterator, true) {
            Ok(msg) => Ok(Line0::Author(msg)),
            Err(_) => Ok(Line0::Author(String::from(""))),
        },
        "FILE" => match next_token(&mut inner_iterator, true) {
            Ok(msg) => Ok(Line0::File(msg)),
            Err(e) => Err(e),
        },
        "STEP" => Ok(Line0::Meta(Meta::Step)),
        "WRITE" => match next_token(&mut inner_iterator, true) {
            Ok(msg) => Ok(Line0::Meta(Meta::Write(msg))),
            Err(e) => Err(e),
        },
        "PRINT" => match next_token(&mut inner_iterator, true) {
            Ok(msg) => Ok(Line0::Meta(Meta::Print(msg))),
            Err(e) => Err(e),
        },
        "CLEAR" => Ok(Line0::Meta(Meta::Clear)),
        "PAUSE" => Ok(Line0::Meta(Meta::Pause)),
        "SAVE" => Ok(Line0::Meta(Meta::Save)),
        _ => Ok(Line0::Meta(Meta::Comment(text))),
    }
}

fn parse_line_1(
    materials: &MaterialRegistry,
    iterator: &mut Chars,
) -> Result<PartReference, ParseError> {
    let color = next_token_u32(iterator)?;
    let x = next_token_f32(iterator)?;
    let y = next_token_f32(iterator)?;
    let z = next_token_f32(iterator)?;
    let matrix = Matrix4::new(
        next_token_f32(iterator)?,
        next_token_f32(iterator)?,
        next_token_f32(iterator)?,
        x,
        next_token_f32(iterator)?,
        next_token_f32(iterator)?,
        next_token_f32(iterator)?,
        y,
        next_token_f32(iterator)?,
        next_token_f32(iterator)?,
        next_token_f32(iterator)?,
        z,
        0.0,
        0.0,
        0.0,
        1.0,
    )
    .transpose();
    let name = next_token(iterator, true)?;
    Ok(PartReference {
        color: ColorReference::resolve(color, materials),
        matrix,
        name: PartAlias::from(name),
    })
}

fn parse_line_2(materials: &MaterialRegistry, iterator: &mut Chars) -> Result<Line, ParseError> {
    let color = next_token_u32(iterator)?;
    let a = Vector4::new(
        next_token_f32(iterator)?,
        next_token_f32(iterator)?,
        next_token_f32(iterator)?,
        1.0,
    );
    let b = Vector4::new(
        next_token_f32(iterator)?,
        next_token_f32(iterator)?,
        next_token_f32(iterator)?,
        1.0,
    );
    Ok(Line {
        color: ColorReference::resolve(color, materials),
        a,
        b,
    })
}

fn parse_line_3(
    materials: &MaterialRegistry,
    iterator: &mut Chars,
) -> Result<Triangle, ParseError> {
    let color = next_token_u32(iterator)?;
    let a = Vector4::new(
        next_token_f32(iterator)?,
        next_token_f32(iterator)?,
        next_token_f32(iterator)?,
        1.0,
    );
    let b = Vector4::new(
        next_token_f32(iterator)?,
        next_token_f32(iterator)?,
        next_token_f32(iterator)?,
        1.0,
    );
    let c = Vector4::new(
        next_token_f32(iterator)?,
        next_token_f32(iterator)?,
        next_token_f32(iterator)?,
        1.0,
    );
    Ok(Triangle {
        color: ColorReference::resolve(color, materials),
        a,
        b,
        c,
    })
}

fn parse_line_4(materials: &MaterialRegistry, iterator: &mut Chars) -> Result<Quad, ParseError> {
    let color = next_token_u32(iterator)?;
    let a = Vector4::new(
        next_token_f32(iterator)?,
        next_token_f32(iterator)?,
        next_token_f32(iterator)?,
        1.0,
    );
    let b = Vector4::new(
        next_token_f32(iterator)?,
        next_token_f32(iterator)?,
        next_token_f32(iterator)?,
        1.0,
    );
    let c = Vector4::new(
        next_token_f32(iterator)?,
        next_token_f32(iterator)?,
        next_token_f32(iterator)?,
        1.0,
    );
    let d = Vector4::new(
        next_token_f32(iterator)?,
        next_token_f32(iterator)?,
        next_token_f32(iterator)?,
        1.0,
    );
    Ok(Quad {
        color: ColorReference::resolve(color, materials),
        a,
        b,
        c,
        d,
    })
}

fn parse_line_5(
    materials: &MaterialRegistry,
    iterator: &mut Chars,
) -> Result<OptionalLine, ParseError> {
    let color = next_token_u32(iterator)?;
    let a = Vector4::new(
        next_token_f32(iterator)?,
        next_token_f32(iterator)?,
        next_token_f32(iterator)?,
        1.0,
    );
    let b = Vector4::new(
        next_token_f32(iterator)?,
        next_token_f32(iterator)?,
        next_token_f32(iterator)?,
        1.0,
    );
    let c = Vector4::new(
        next_token_f32(iterator)?,
        next_token_f32(iterator)?,
        next_token_f32(iterator)?,
        1.0,
    );
    let d = Vector4::new(
        next_token_f32(iterator)?,
        next_token_f32(iterator)?,
        next_token_f32(iterator)?,
        1.0,
    );
    Ok(OptionalLine {
        color: ColorReference::resolve(color, materials),
        a,
        b,
        c,
        d,
    })
}

async fn parse_inner<T: BufRead + Unpin>(
    materials: &MaterialRegistry,
    iterator: &mut Enumerate<Lines<T>>,
    multipart: bool,
) -> Result<(Document, Option<String>), DocumentParseError> {
    let mut next: Option<String> = None;
    let mut name = String::new();
    let mut author = String::new();
    let mut description = String::new();
    let mut bfc = BfcCertification::NotApplicable;
    let mut commands = Vec::new();
    let mut headers = Vec::new();

    'read_loop: while let Some((index, line_)) = iterator.next().await {
        let line = match line_ {
            Ok(v) => v,
            Err(e) => {
                return Err(DocumentParseError {
                    line: index + 1,
                    error: ParseError::from(e),
                });
            }
        };
        let mut it = line.chars();
        match next_token(&mut it, false) {
            Ok(token) => match token.as_str() {
                "0" => match parse_line_0(&mut it) {
                    Ok(val) => match val {
                        Line0::BfcCertification(bfc_) => {
                            bfc = bfc_;
                        }
                        Line0::File(file_) => {
                            if multipart {
                                if !description.is_empty() {
                                    next = Some(file_);
                                    break 'read_loop;
                                }
                            } else {
                                return Err(DocumentParseError {
                                    line: index + 1,
                                    error: ParseError::MultipartDocument,
                                });
                            }
                        }
                        Line0::Name(name_) => {
                            name = name_;
                        }
                        Line0::Author(author_) => {
                            author = author_;
                        }
                        Line0::Meta(meta) => {
                            if let Meta::Comment(comment) = meta {
                                if description.is_empty() {
                                    description = comment;
                                } else {
                                    commands.push(Command::Meta(Meta::Comment(comment)));
                                }
                            } else {
                                commands.push(Command::Meta(meta));
                            }
                        }
                        Line0::Header(header) => {
                            headers.push(header);
                        }
                    },
                    Err(e) => {
                        return Err(DocumentParseError {
                            line: index + 1,
                            error: e,
                        });
                    }
                },
                "1" => match parse_line_1(materials, &mut it) {
                    Ok(val) => commands.push(Command::PartReference(val)),
                    Err(e) => {
                        return Err(DocumentParseError {
                            line: index + 1,
                            error: e,
                        });
                    }
                },
                "2" => match parse_line_2(materials, &mut it) {
                    Ok(val) => commands.push(Command::Line(val)),
                    Err(e) => {
                        return Err(DocumentParseError {
                            line: index + 1,
                            error: e,
                        });
                    }
                },
                "3" => match parse_line_3(materials, &mut it) {
                    Ok(val) => commands.push(Command::Triangle(val)),
                    Err(e) => {
                        return Err(DocumentParseError {
                            line: index + 1,
                            error: e,
                        });
                    }
                },
                "4" => match parse_line_4(materials, &mut it) {
                    Ok(val) => commands.push(Command::Quad(val)),
                    Err(e) => {
                        return Err(DocumentParseError {
                            line: index + 1,
                            error: e,
                        });
                    }
                },
                "5" => match parse_line_5(materials, &mut it) {
                    Ok(val) => commands.push(Command::OptionalLine(val)),
                    Err(e) => {
                        return Err(DocumentParseError {
                            line: index + 1,
                            error: e,
                        });
                    }
                },
                _ => {
                    return Err(DocumentParseError {
                        line: index + 1,
                        error: ParseError::UnexpectedCommand(token),
                    });
                }
            },
            Err(ParseError::EndOfLine) => {}
            Err(e) => {
                return Err(DocumentParseError {
                    line: index + 1,
                    error: e,
                });
            }
        }
    }

    Ok((
        Document {
            name,
            description,
            author,
            bfc,
            headers,
            commands,
        },
        next,
    ))
}

pub async fn parse_single_document<T: BufRead + Unpin>(
    materials: &MaterialRegistry,
    reader: &mut T,
) -> Result<Document, DocumentParseError> {
    let mut it = reader.lines().enumerate();
    let (document, _) = parse_inner(materials, &mut it, false).await?;

    Ok(document)
}

pub async fn parse_multipart_document<T: BufRead + Unpin>(
    materials: &MaterialRegistry,
    reader: &mut T,
) -> Result<MultipartDocument, DocumentParseError> {
    let mut it = reader.lines().enumerate();
    let (document, mut next) = parse_inner(materials, &mut it, true).await?;
    let mut subparts = HashMap::new();

    while next.is_some() {
        let (part, next_) = parse_inner(materials, &mut it, true).await?;

        subparts.insert(PartAlias::from(&next.unwrap()), part);
        next = next_;
    }

    Ok(MultipartDocument {
        body: document,
        subparts,
    })
}

fn parse_customized_material(
    iterator: &mut Chars,
) -> Result<CustomizedMaterial, ColorDefinitionParseError> {
    match next_token(iterator, false)?.as_str() {
        "GLITTER" => {
            let mut alpha = 255u8;
            let mut luminance = 0u8;
            let mut fraction = 0.0;
            let mut vfraction = 0.0;
            let mut size = 0u32;
            let mut minsize = 0.0;
            let mut maxsize = 0.0;
            match next_token(iterator, false)?.as_str() {
                "VALUE" => (),
                e => {
                    return Err(ColorDefinitionParseError::ParseError(
                        ParseError::InvalidToken(e.to_string()),
                    ));
                }
            };
            let (vr, vg, vb) = next_token_rgb(iterator)?;
            loop {
                let token = match next_token(iterator, false) {
                    Ok(v) => v,
                    Err(ParseError::EndOfLine) => break,
                    Err(e) => return Err(ColorDefinitionParseError::ParseError(e)),
                };

                match token.as_str() {
                    "ALPHA" => {
                        alpha = next_token_u32(iterator)? as u8;
                    }
                    "LUMINANCE" => {
                        luminance = next_token_u32(iterator)? as u8;
                    }
                    "FRACTION" => {
                        fraction = next_token_f32(iterator)?;
                    }
                    "VFRACTION" => {
                        vfraction = next_token_f32(iterator)?;
                    }
                    "SIZE" => {
                        size = next_token_u32(iterator)?;
                    }
                    "MINSIZE" => {
                        minsize = next_token_f32(iterator)?;
                    }
                    "MAXSIZE" => {
                        maxsize = next_token_f32(iterator)?;
                    }
                    _ => {
                        return Err(ColorDefinitionParseError::ParseError(
                            ParseError::InvalidToken(token.clone()),
                        ));
                    }
                }
            }
            Ok(CustomizedMaterial::Glitter(MaterialGlitter {
                value: Rgba::new(vr, vg, vb, alpha),
                luminance,
                fraction,
                vfraction,
                size,
                minsize,
                maxsize,
            }))
        }
        "SPECKLE" => {
            let mut alpha = 255u8;
            let mut luminance = 0u8;
            let mut fraction = 0.0;
            let mut size = 0u32;
            let mut minsize = 0.0;
            let mut maxsize = 0.0;
            match next_token(iterator, false)?.as_str() {
                "VALUE" => (),
                e => {
                    return Err(ColorDefinitionParseError::ParseError(
                        ParseError::InvalidToken(e.to_string()),
                    ));
                }
            };
            let (vr, vg, vb) = next_token_rgb(iterator)?;
            loop {
                let token = match next_token(iterator, false) {
                    Ok(v) => v,
                    Err(ParseError::EndOfLine) => break,
                    Err(e) => return Err(ColorDefinitionParseError::ParseError(e)),
                };

                match token.as_str() {
                    "ALPHA" => {
                        alpha = next_token_u32(iterator)? as u8;
                    }
                    "LUMINANCE" => {
                        luminance = next_token_u32(iterator)? as u8;
                    }
                    "FRACTION" => {
                        fraction = next_token_f32(iterator)?;
                    }
                    "SIZE" => {
                        size = next_token_u32(iterator)?;
                    }
                    "MINSIZE" => {
                        minsize = next_token_f32(iterator)?;
                    }
                    "MAXSIZE" => {
                        maxsize = next_token_f32(iterator)?;
                    }
                    _ => {
                        return Err(ColorDefinitionParseError::ParseError(
                            ParseError::InvalidToken(token.clone()),
                        ));
                    }
                }
            }
            Ok(CustomizedMaterial::Speckle(MaterialSpeckle {
                value: Rgba::new(vr, vg, vb, alpha),
                luminance,
                fraction,
                size,
                minsize,
                maxsize,
            }))
        }
        e => Err(ColorDefinitionParseError::UnknownMaterial(e.to_string())),
    }
}

pub async fn parse_color_definition<T: BufRead + Unpin>(
    reader: &mut T,
) -> Result<MaterialRegistry, ColorDefinitionParseError> {
    // Use an empty context here
    let materials = MaterialRegistry::new();
    let document = parse_single_document(&materials, reader).await?;

    let mut materials = MaterialRegistry::new();
    for Header(_, value) in document.headers.iter().filter(|s| s.0 == "COLOUR") {
        let mut finish = Finish::Plastic;
        let mut alpha = 255u8;
        let mut luminance = 0u8;

        let mut it = value.chars();
        let name = next_token(&mut it, false)?;

        match next_token(&mut it, false)?.as_str() {
            "CODE" => (),
            e => {
                return Err(ColorDefinitionParseError::ParseError(
                    ParseError::InvalidToken(e.to_string()),
                ));
            }
        };
        let code = next_token_u32(&mut it)?;

        match next_token(&mut it, false)?.as_str() {
            "VALUE" => (),
            e => {
                return Err(ColorDefinitionParseError::ParseError(
                    ParseError::InvalidToken(e.to_string()),
                ));
            }
        };
        let (cr, cg, cb) = next_token_rgb(&mut it)?;

        match next_token(&mut it, false)?.as_str() {
            "EDGE" => (),
            e => {
                return Err(ColorDefinitionParseError::ParseError(
                    ParseError::InvalidToken(e.to_string()),
                ));
            }
        };
        let (er, eg, eb) = next_token_rgb(&mut it)?;

        loop {
            let token = match next_token(&mut it, false) {
                Ok(v) => v,
                Err(ParseError::EndOfLine) => break,
                Err(e) => return Err(ColorDefinitionParseError::ParseError(e)),
            };

            match token.as_str() {
                "ALPHA" => {
                    alpha = next_token_u32(&mut it)? as u8;
                }
                "LUMINANCE" => {
                    luminance = next_token_u32(&mut it)? as u8;
                }
                "CHROME" => {
                    finish = Finish::Chrome;
                }
                "PEARLESCENT" => {
                    finish = Finish::Pearlescent;
                }
                "METAL" => {
                    finish = Finish::Metal;
                }
                "RUBBER" => {
                    finish = Finish::Rubber;
                }
                "MATTE_METALLIC" => {
                    finish = Finish::MatteMetallic;
                }
                "MATERIAL" => {
                    finish = Finish::Custom(parse_customized_material(&mut it)?);
                }
                _ => {
                    return Err(ColorDefinitionParseError::ParseError(
                        ParseError::InvalidToken(token.clone()),
                    ));
                }
            }
        }

        materials.insert(
            code,
            Material {
                code,
                name,
                color: Rgba::new(cr, cg, cb, alpha),
                edge: Rgba::new(er, eg, eb, 255),
                luminance,
                finish,
            },
        );
    }

    Ok(materials)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_line_0_or_panic(input: &str) -> Line0 {
        match parse_line_0(&mut input.chars()) {
            Ok(line0) => line0,
            Err(e) => {
                panic!("cannot parse {}: {}", input, e);
            }
        }
    }

    #[test]
    fn parse_line_0_parses_comment() {
        let cases = [
            ("// This is a comment", "This is a comment"),
            ("This is also a comment", "This is also a comment"),
        ];

        for (input, output) in cases {
            let parsed = parse_line_0_or_panic(input);
            match parsed {
                Line0::Meta(Meta::Comment(comment)) => assert_eq!(comment, output),
                _ => panic!("expected Line0::Meta(Meta::Comment(...)), got {:?}", parsed),
            }
        }
    }

    #[test]
    fn parse_line_0_parses_offical_meta_commands_without_bfc() {
        let cases = [
            ("STEP", Meta::Step),
            (
                "WRITE any length of string",
                Meta::Write("any length of string".into()),
            ),
            (
                "PRINT also any length of string",
                Meta::Print("also any length of string".into()),
            ),
            ("CLEAR", Meta::Clear),
            ("PAUSE", Meta::Pause),
            ("SAVE", Meta::Save),
        ];
        for (input, output) in cases {
            let parsed = parse_line_0_or_panic(input);
            match parsed {
                Line0::Meta(meta) => assert_eq!(meta, output),
                _ => panic!("expected Line0::Meta(...), got {:?}", parsed),
            }
        }
    }

    #[test]
    fn parse_line_0_parses_bfc_statements() {
        let cases = [
            ("BFC CW", BfcStatement::Winding(Winding::Cw)),
            ("BFC CCW", BfcStatement::Winding(Winding::Ccw)),
            ("BFC CLIP", BfcStatement::Clip(None)),
            ("BFC CLIP CW", BfcStatement::Clip(Some(Winding::Cw))),
            ("BFC CLIP CCW", BfcStatement::Clip(Some(Winding::Ccw))),
            ("BFC CW CLIP", BfcStatement::Clip(Some(Winding::Cw))),
            ("BFC CCW CLIP", BfcStatement::Clip(Some(Winding::Ccw))),
            ("BFC NOCLIP", BfcStatement::NoClip),
            ("BFC INVERTNEXT", BfcStatement::InvertNext),
        ];
        for (input, output) in cases {
            let parsed = parse_line_0_or_panic(input);
            match parsed {
                Line0::Meta(Meta::Bfc(bfc)) => assert_eq!(bfc, output),
                _ => panic!("expected Line0::Meta(Meta::Bfc(...)) got {:?}", parsed),
            }
        }
    }

    #[test]
    fn parse_line_0_parses_bfc_certificates() {
        let cases = [
            ("BFC NOCERTIFY", BfcCertification::NoCertify),
            ("BFC CERTIFY CW", BfcCertification::Certify(Winding::Cw)),
            ("BFC CERTIFY", BfcCertification::Certify(Winding::Ccw)),
            ("BFC CERTIFY CCW", BfcCertification::Certify(Winding::Ccw)),
        ];
        for (input, output) in cases {
            let parsed = parse_line_0_or_panic(input);
            match parsed {
                Line0::BfcCertification(certification) => assert_eq!(certification, output),
                _ => panic!("expected Line0::BfsCertification(...), got {:?}", parsed),
            }
        }
    }

    #[test]
    fn parse_line_0_parses_headers() {
        let cases = [
            (
                "!LDRAW_ORG Part UPDATE 2006-01",
                Header("LDRAW_ORG".into(), "Part UPDATE 2006-01".into()),
            ),
            (
                "!LICENSE Redistributable under CCAL version 2.: see CAreadme.txt",
                Header(
                    "LICENSE".into(),
                    "Redistributable under CCAL version 2.: see CAreadme.txt".into(),
                ),
            ),
            (
                "!HELP Obviously there is no need for additional",
                Header(
                    "HELP".into(),
                    "Obviously there is no need for additional".into(),
                ),
            ),
            ("!HELP", Header("HELP".into(), "".into())),
            (
                "!CATEGORY Animal",
                Header("CATEGORY".into(), "Animal".into()),
            ),
            (
                "!KEYWORDS Sting, Poison, Adventurers, Egypt",
                Header(
                    "KEYWORDS".into(),
                    "Sting, Poison, Adventurers, Egypt".into(),
                ),
            ),
            ("!CMDLINE -c1", Header("CMDLINE".into(), "-c1".into())),
            (
                "!HISTORY 2000-08-?? {Axel Poque} fixes to resolve L3P error messages",
                Header(
                    "HISTORY".into(),
                    "2000-08-?? {Axel Poque} fixes to resolve L3P error messages".into(),
                ),
            ),
            (
                "!HISTORY 2002-04-25 [PTadmin] Official update 2002-02",
                Header(
                    "HISTORY".into(),
                    "2002-04-25 [PTadmin] Official update 2002-02".into(),
                ),
            ),
        ];
        for (input, output) in cases {
            let parsed = parse_line_0_or_panic(input);
            match parsed {
                Line0::Header(header) => assert_eq!(header, output),
                _ => panic!("expected Line0::Header(...), got {:?}", parsed),
            }
        }
    }

    #[test]
    fn parse_line_0_parses_name_author() {
        let name = "Name: 193a.dat";
        let parsed_name = parse_line_0_or_panic(name);
        assert_eq!(parsed_name, Line0::Name("193a.dat".into()));

        let author = "Author: Chris Dee [cwdee]";
        let parsed_author = parse_line_0_or_panic(author);
        assert_eq!(parsed_author, Line0::Author("Chris Dee [cwdee]".into()));
    }

    #[test]
    fn parse_line_0_parses_file() {
        let file = "FILE main.ldr";
        let parsed_file = parse_line_0_or_panic(file);
        assert_eq!(parsed_file, Line0::File("main.ldr".into()));
    }

    #[test]
    fn parse_customized_material_parses_glitter() {
        let cases = [
            ("GLITTER VALUE #122334 FRACTION 0.3 VFRACTION 2.4 SIZE 1", MaterialGlitter {
                value: Rgba::new(0x12, 0x23, 0x34, 255),
                luminance: 0,
                fraction: 0.3,
                vfraction: 2.4,
                size: 1,
                minsize: 0.,
                maxsize: 0.,
            }),
            ("GLITTER VALUE #00DEAD LUMINANCE 128 FRACTION 0.5 VFRACTION 0.4 MINSIZE 2 MAXSIZE 3", MaterialGlitter {
                value: Rgba::new(0x00, 0xde, 0xad, 255),
                luminance: 128,
                fraction: 0.5,
                vfraction: 0.4,
                size: 0,
                minsize: 2.,
                maxsize: 3.,
            }),
            ("GLITTER VALUE #BEEF00 ALPHA 240 FRACTION 0.1 VFRACTION 0.12 SIZE 7", MaterialGlitter {
                value: Rgba::new(0xbe, 0xef, 0x00, 240),
                luminance: 0,
                fraction: 0.1,
                vfraction: 0.12,
                size: 7,
                minsize: 0.,
                maxsize: 0.,
            }),
            ("GLITTER VALUE #677889 ALPHA 5 LUMINANCE 10 FRACTION 1 VFRACTION 2 MINSIZE 1.1 MAXSIZE 4.3", MaterialGlitter {
                value: Rgba::new(0x67, 0x78, 0x89, 5),
                luminance: 10,
                fraction: 1.,
                vfraction: 2.,
                size: 0,
                minsize: 1.1,
                maxsize: 4.3,
            }),
        ];
        for (input, output) in cases {
            let parsed = parse_customized_material(&mut input.chars()).unwrap();
            match parsed {
                CustomizedMaterial::Glitter(glitter) => assert_eq!(glitter, output),
                _ => panic!(
                    "expected CustomizedMaterial::Glitter(...), got: {:?}",
                    parsed
                ),
            }
        }
    }

    #[test]
    fn parse_customized_material_parses_speckle() {
        let cases = [
            (
                "SPECKLE VALUE #122334 FRACTION 0.3 SIZE 1",
                MaterialSpeckle {
                    value: Rgba::new(0x12, 0x23, 0x34, 255),
                    luminance: 0,
                    fraction: 0.3,
                    size: 1,
                    minsize: 0.,
                    maxsize: 0.,
                },
            ),
            (
                "SPECKLE VALUE #00DEAD LUMINANCE 128 FRACTION 0.5 MINSIZE 2 MAXSIZE 3",
                MaterialSpeckle {
                    value: Rgba::new(0x00, 0xde, 0xad, 255),
                    luminance: 128,
                    fraction: 0.5,
                    size: 0,
                    minsize: 2.,
                    maxsize: 3.,
                },
            ),
            (
                "SPECKLE VALUE #BEEF00 ALPHA 240 FRACTION 0.1 SIZE 7",
                MaterialSpeckle {
                    value: Rgba::new(0xbe, 0xef, 0x00, 240),
                    luminance: 0,
                    fraction: 0.1,
                    size: 7,
                    minsize: 0.,
                    maxsize: 0.,
                },
            ),
            (
                "SPECKLE VALUE #677889 ALPHA 5 LUMINANCE 10 FRACTION 1 MINSIZE 1.1 MAXSIZE 4.3",
                MaterialSpeckle {
                    value: Rgba::new(0x67, 0x78, 0x89, 5),
                    luminance: 10,
                    fraction: 1.,
                    size: 0,
                    minsize: 1.1,
                    maxsize: 4.3,
                },
            ),
        ];
        for (input, output) in cases {
            let parsed = parse_customized_material(&mut input.chars()).unwrap();
            match parsed {
                CustomizedMaterial::Speckle(speckle) => assert_eq!(speckle, output),
                _ => panic!(
                    "expected CustomizedMaterial::Speckle(...), got: {:?}",
                    parsed
                ),
            }
        }
    }

    const COLOR_DEFINITIONS: &str =
"0 Color Definition for testing
0 Name: LDConfig.ldr
0 Author: LDraw.rs

0 !COLOUR Solid                                                 CODE   0   VALUE #000000   EDGE #595959
0 !COLOUR Transparent                                           CODE   1   VALUE #FF0000   EDGE #00FF00   ALPHA 128
0 !COLOUR Chrome                                                CODE   2   VALUE #00FF00   EDGE #FF0000   CHROME
0 !COLOUR Pearl                                                 CODE   3   VALUE #0000FF   EDGE #00FF00   PEARLESCENT
0 !COLOUR Metal                                                 CODE   4   VALUE #FF0000   EDGE #0000FF   METAL
0 !COLOUR Phosphorescent                                        CODE   5   VALUE #FF00FF   EDGE #00FF00   ALPHA 240   LUMINANCE 15
0 !COLOUR Glitter                                               CODE   6   VALUE #FFFF00   EDGE #00FFFF   MATERIAL GLITTER VALUE #FF00FF FRACTION 0.17 VFRACTION 0.2 SIZE 1
0 !COLOUR Glitter_Transparent                                   CODE   7   VALUE #00FFFF   EDGE #FFFF00   ALPHA 128   MATERIAL GLITTER VALUE #FF00FF FRACTION 0.17 VFRACTION 0.2 SIZE 1
0 !COLOUR Speckle                                               CODE   8   VALUE #123456   EDGE #654321   MATERIAL SPECKLE VALUE #898788 FRACTION 0.4 MINSIZE 1 MAXSIZE 3
0 !COLOUR Rubber                                                CODE   9   VALUE #ABCDEF   EDGE #FEDCBA   RUBBER";

    #[async_std::test]
    async fn test_parse_color_definition() {
        let parsed = parse_color_definition(&mut COLOR_DEFINITIONS.as_bytes())
            .await
            .unwrap();
        let materials = [
            Material {
                code: 0,
                name: "Solid".into(),
                color: Rgba::new(0x00, 0x00, 0x00, 255),
                edge: Rgba::new(0x59, 0x59, 0x59, 255),
                luminance: 0,
                finish: Finish::Plastic,
            },
            Material {
                code: 1,
                name: "Transparent".into(),
                color: Rgba::new(0xff, 0x00, 0x00, 128),
                edge: Rgba::new(0x00, 0xff, 0x00, 255),
                luminance: 0,
                finish: Finish::Plastic,
            },
            Material {
                code: 2,
                name: "Chrome".into(),
                color: Rgba::new(0x00, 0xff, 0x00, 255),
                edge: Rgba::new(0xff, 0x00, 0x00, 255),
                luminance: 0,
                finish: Finish::Chrome,
            },
            Material {
                code: 3,
                name: "Pearl".into(),
                color: Rgba::new(0x00, 0x00, 0xff, 255),
                edge: Rgba::new(0x00, 0xff, 0x00, 255),
                luminance: 0,
                finish: Finish::Pearlescent,
            },
            Material {
                code: 4,
                name: "Metal".into(),
                color: Rgba::new(0xff, 0x00, 0x00, 255),
                edge: Rgba::new(0x00, 0x00, 0xff, 255),
                luminance: 0,
                finish: Finish::Metal,
            },
            Material {
                code: 5,
                name: "Phosphorescent".into(),
                color: Rgba::new(0xff, 0x00, 0xff, 240),
                edge: Rgba::new(0x00, 0xff, 0x00, 255),
                luminance: 15,
                finish: Finish::Plastic,
            },
            Material {
                code: 6,
                name: "Glitter".into(),
                color: Rgba::new(0xff, 0xff, 0x00, 255),
                edge: Rgba::new(0x00, 0xff, 0xff, 255),
                luminance: 0,
                finish: Finish::Custom(CustomizedMaterial::Glitter(MaterialGlitter {
                    value: Rgba::new(0xff, 0x00, 0xff, 255),
                    luminance: 0,
                    fraction: 0.17,
                    vfraction: 0.2,
                    size: 1,
                    minsize: 0.,
                    maxsize: 0.,
                })),
            },
            Material {
                code: 7,
                name: "Glitter_Transparent".into(),
                color: Rgba::new(0x00, 0xff, 0xff, 128),
                edge: Rgba::new(0xff, 0xff, 0x00, 255),
                luminance: 0,
                finish: Finish::Custom(CustomizedMaterial::Glitter(MaterialGlitter {
                    value: Rgba::new(0xff, 0x00, 0xff, 255),
                    luminance: 0,
                    fraction: 0.17,
                    vfraction: 0.2,
                    size: 1,
                    minsize: 0.,
                    maxsize: 0.,
                })),
            },
            Material {
                code: 8,
                name: "Speckle".into(),
                color: Rgba::new(0x12, 0x34, 0x56, 255),
                edge: Rgba::new(0x65, 0x43, 0x21, 255),
                luminance: 0,
                finish: Finish::Custom(CustomizedMaterial::Speckle(MaterialSpeckle {
                    value: Rgba::new(0x89, 0x87, 0x88, 255),
                    luminance: 0,
                    fraction: 0.4,
                    size: 0,
                    minsize: 1.,
                    maxsize: 3.,
                })),
            },
            Material {
                code: 9,
                name: "Rubber".into(),
                color: Rgba::new(0xab, 0xcd, 0xef, 255),
                edge: Rgba::new(0xfe, 0xdc, 0xba, 255),
                luminance: 0,
                finish: Finish::Rubber,
            },
        ];
        for material in materials {
            assert_eq!(parsed[&material.code], material);
        }
    }

    #[async_std::test]
    async fn test_parse_line_1() {
        let colors = parse_color_definition(&mut COLOR_DEFINITIONS.as_bytes())
            .await
            .unwrap();
        let line_1 = "1 11 -0.25 -16 2 0 0 0 1 0 0 0 -2 1-4disc.dat";
        let parsed = parse_line_1(&colors, &mut line_1.chars()).unwrap();
        assert_eq!(
            parsed,
            PartReference {
                color: ColorReference::Material(colors[&1].clone()),
                matrix: Matrix4::new(
                    2., 0., 0., 0., 0., 1., 0., 0., 0., 0., -2., 0., 11., -0.25, -16., 1.,
                ),
                name: "1-4disc.dat".into(),
            }
        );
    }

    #[async_std::test]
    async fn test_parse_line_2() {
        let colors = parse_color_definition(&mut COLOR_DEFINITIONS.as_bytes())
            .await
            .unwrap();
        let line_2 = "16 3 2.7 8 -12.23 4.17 .67";
        let parsed = parse_line_2(&colors, &mut line_2.chars()).unwrap();
        assert_eq!(
            parsed,
            Line {
                color: ColorReference::Current,
                a: Vector4::new(3., 2.7, 8., 1.),
                b: Vector4::new(-12.23, 4.17, 0.67, 1.),
            }
        );
    }

    #[async_std::test]
    async fn test_parse_line_3() {
        let colors = parse_color_definition(&mut COLOR_DEFINITIONS.as_bytes())
            .await
            .unwrap();
        let line_3 = "15 22.04 -.25 -1.16 23.72 -.25 -4.49 23.72 -.25 -2.61";
        let parsed = parse_line_3(&colors, &mut line_3.chars()).unwrap();
        assert_eq!(
            parsed,
            Triangle {
                color: ColorReference::Unknown(15),
                a: Vector4::new(22.04, -0.25, -1.16, 1.),
                b: Vector4::new(23.72, -0.25, -4.49, 1.),
                c: Vector4::new(23.72, -0.25, -2.61, 1.),
            }
        );
    }

    #[async_std::test]
    async fn test_parse_line_4() {
        let colors = parse_color_definition(&mut COLOR_DEFINITIONS.as_bytes())
            .await
            .unwrap();
        let line_4 = "1 -11 -0.25 -18 11 -0.25 -18 11 -0.25 -12.7 -11 -0.25 -12.7";
        let parsed = parse_line_4(&colors, &mut line_4.chars()).unwrap();
        assert_eq!(
            parsed,
            Quad {
                color: ColorReference::Material(colors[&1].clone()),
                a: Vector4::new(-11., -0.25, -18., 1.),
                b: Vector4::new(11., -0.25, -18., 1.),
                c: Vector4::new(11., -0.25, -12.7, 1.),
                d: Vector4::new(-11., -0.25, -12.7, 1.),
            }
        );
    }

    #[async_std::test]
    async fn test_parse_line_5() {
        let colors = parse_color_definition(&mut COLOR_DEFINITIONS.as_bytes())
            .await
            .unwrap();
        let line_5 =
            "24 0 -55.673 -15.623 0 -59.974 -18.831 4.233 -59.338 -18.968 -4.233 -59.338 -18.968";
        let parsed = parse_line_5(&colors, &mut line_5.chars()).unwrap();
        assert_eq!(
            parsed,
            OptionalLine {
                color: ColorReference::Complement,
                a: Vector4::new(0., -55.673, -15.623, 1.),
                b: Vector4::new(0., -59.974, -18.831, 1.),
                c: Vector4::new(4.233, -59.338, -18.968, 1.),
                d: Vector4::new(-4.233, -59.338, -18.968, 1.),
            }
        );
    }

    #[async_std::test]
    async fn test_parse_single_document() {
        let colors = parse_color_definition(&mut COLOR_DEFINITIONS.as_bytes())
            .await
            .unwrap();
        let document = "0 Boat Base 8 x 10
0 Name: 2622.dat
0 Author: Chris Alano
0 !LDRAW_ORG Part UPDATE 2000-02
0 !LICENSE Not redistributable : see NonCAreadme.txt

0 BFC NOCERTIFY

0 !KEYWORDS Pirates, Caribbean, Ship

2 24 100 24 80 80 24 20";
        let parsed = parse_single_document(&colors, &mut document.as_bytes())
            .await
            .unwrap();
        assert_eq!(
            parsed,
            Document {
                name: "2622.dat".into(),
                description: "Boat Base 8 x 10".into(),
                author: "Chris Alano".into(),
                bfc: BfcCertification::NoCertify,
                headers: vec![
                    Header("LDRAW_ORG".into(), "Part UPDATE 2000-02".into()),
                    Header(
                        "LICENSE".into(),
                        "Not redistributable : see NonCAreadme.txt".into()
                    ),
                    Header("KEYWORDS".into(), "Pirates, Caribbean, Ship".into()),
                ],
                commands: vec![Command::Line(Line {
                    color: ColorReference::Complement,
                    a: Vector4::new(100., 24., 80., 1.),
                    b: Vector4::new(80., 24., 20., 1.),
                }),]
            }
        );
    }

    #[async_std::test]
    async fn test_parse_multipart_document() {
        let colors = parse_color_definition(&mut COLOR_DEFINITIONS.as_bytes())
            .await
            .unwrap();
        let document = "0 FILE test.ldr
0 LDraw.rs
0 Name: test.ldr
0 Author: kiwiyou


0 Unofficial Model
0 ROTATION CENTER 0 0 0 1 \"Custom\"
3 7 22.04 -.25 -1.16 23.72 -.25 -4.49 23.72 -.25 -2.61
1 3 0 0 0 0 0 0 0 0 0 0 0 0 apple.ldr

0 FILE apple.ldr
0 Apple
0 Name: apple.ldr
0 Author: kiwiyou
0 BFC CERTIFY CCW


5 24 0 -55.673 -15.623 0 -59.974 -18.831 4.233 -59.338 -18.968 -4.233 -59.338 -18.968";
        let parsed = parse_multipart_document(&colors, &mut document.as_bytes())
            .await
            .unwrap();
        let mut subparts = HashMap::new();
        subparts.insert(
            PartAlias {
                normalized: "apple.ldr".into(),
                original: "apple.ldr".into(),
            },
            Document {
                name: "apple.ldr".into(),
                description: "Apple".into(),
                author: "kiwiyou".into(),
                bfc: BfcCertification::Certify(Winding::Ccw),
                headers: vec![],
                commands: vec![Command::OptionalLine(OptionalLine {
                    color: ColorReference::Complement,
                    a: Vector4::new(0., -55.673, -15.623, 1.),
                    b: Vector4::new(0., -59.974, -18.831, 1.),
                    c: Vector4::new(4.233, -59.338, -18.968, 1.),
                    d: Vector4::new(-4.233, -59.338, -18.968, 1.),
                })],
            },
        );
        assert_eq!(
            parsed,
            MultipartDocument {
                body: Document {
                    name: "test.ldr".into(),
                    description: "LDraw.rs".into(),
                    author: "kiwiyou".into(),
                    bfc: BfcCertification::NotApplicable,
                    headers: vec![],
                    commands: vec![
                        Command::Meta(Meta::Comment("Unofficial Model".into())),
                        Command::Meta(Meta::Comment("ROTATION CENTER 0 0 0 1 \"Custom\"".into())),
                        Command::Triangle(Triangle {
                            color: ColorReference::Material(colors[&7].clone()),
                            a: Vector4::new(22.04, -0.25, -1.16, 1.),
                            b: Vector4::new(23.72, -0.25, -4.49, 1.),
                            c: Vector4::new(23.72, -0.25, -2.61, 1.),
                        }),
                        Command::PartReference(PartReference {
                            color: ColorReference::Material(colors[&3].clone()),
                            matrix: Matrix4::new(
                                0., 0., 0., 0., 0., 0., 0., 0., 0., 0., 0., 0., 0., 0., 0., 1.,
                            ),
                            name: "apple.ldr".into(),
                        }),
                    ]
                },
                subparts,
            }
        )
    }
}
