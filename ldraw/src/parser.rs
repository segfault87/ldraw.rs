use std::collections::HashMap;
use std::io::{BufRead, Lines};
use std::iter::Enumerate;
use std::str::Chars;

use cgmath::Matrix;

use crate::color::{
    ColorReference, CustomizedMaterial, Finish, Material, MaterialGlitter, MaterialRegistry,
    MaterialSpeckle, Rgba,
};
use crate::document::{BfcCertification, Document, MultipartDocument};
use crate::elements::{
    BfcStatement, Command, Header, Line, Meta, OptionalLine, PartReference, Quad, Triangle,
};
use crate::error::{ColorDefinitionParseError, DocumentParseError, ParseError};
use crate::NormalizedAlias;
use crate::{Matrix4, Vector4, Winding};

#[derive(Debug)]
enum Line0 {
    Header(Header),
    Meta(Meta),
    File(String),
    Name(String),
    Author(String),
    BfcCertification(BfcCertification),
}

fn is_whitespace(ch: char) -> bool {
    match ch {
        ' ' | '\t' | '\r' | '\n' => true,
        _ => false,
    }
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
        "CERTIFY" | "CERTIFY CCW" => Ok(Line0::BfcCertification(BfcCertification::Certify(Winding::Ccw))),
        "CERTIFY CW" => Ok(Line0::BfcCertification(BfcCertification::Certify(Winding::Cw))),
        "CW" => Ok(Line0::Meta(Meta::Bfc(BfcStatement::Winding(Winding::Cw)))),
        "CCW" => Ok(Line0::Meta(Meta::Bfc(BfcStatement::Winding(Winding::Ccw)))),
        "CLIP" => Ok(Line0::Meta(Meta::Bfc(BfcStatement::Clip))),
        "CLIP CW" => Ok(Line0::Meta(Meta::Bfc(BfcStatement::ClipWinding(Winding::Cw)))),
        "CLIP CCW" => Ok(Line0::Meta(Meta::Bfc(BfcStatement::ClipWinding(Winding::Ccw)))),
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
            Err(e) => Err(e),
        },
        "Author:" => match next_token(&mut inner_iterator, true) {
            Ok(msg) => Ok(Line0::Author(msg)),
            Err(e) => Err(e),
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
    ).transpose();
    let name = next_token(iterator, true)?;
    Ok(PartReference {
        color: ColorReference::resolve(color, materials),
        matrix,
        name: NormalizedAlias::from(name),
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

fn parse_inner<T: BufRead>(
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
    let mut last_index: usize = 0;

    'read_loop: for (index, line_) in iterator {
        last_index = index;

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

    if name.is_empty() || author.is_empty() || description.is_empty() {
        Err(DocumentParseError {
            line: last_index + 1,
            error: ParseError::InvalidDocumentStructure,
        })
    } else {
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
}

pub fn parse_single_document<T: BufRead>(
    materials: &MaterialRegistry,
    reader: &mut T,
) -> Result<Document, DocumentParseError> {
    let mut it = reader.lines().enumerate();
    let (document, _) = parse_inner(materials, &mut it, false)?;

    Ok(document)
}

pub fn parse_multipart_document<T: BufRead>(
    materials: &MaterialRegistry,
    reader: &mut T,
) -> Result<MultipartDocument, DocumentParseError> {
    let mut it = reader.lines().enumerate();
    let (document, mut next) = parse_inner(materials, &mut it, true)?;
    let mut subparts = HashMap::new();

    while next.is_some() {
        let (part, next_) = parse_inner(materials, &mut it, true)?;

        subparts.insert(NormalizedAlias::from(&next.unwrap()), part);
        next = next_;
    }

    Ok(MultipartDocument {
        body: document,
        subparts,
    })
}

fn parse_customized_material(
    mut iterator: &mut Chars,
) -> Result<CustomizedMaterial, ColorDefinitionParseError> {
    match next_token(&mut iterator, false)?.as_str() {
        "GLITTER" => {
            let mut alpha = 255u8;
            let mut luminance = 0u8;
            let mut fraction = 0.0;
            let mut vfraction = 0.0;
            let mut size = 0u32;
            let mut minsize = 0u32;
            let mut maxsize = 0u32;
            match next_token(&mut iterator, false)?.as_str() {
                "VALUE" => (),
                e => {
                    return Err(ColorDefinitionParseError::ParseError(
                        ParseError::InvalidToken(e.to_string()),
                    ));
                }
            };
            let (vr, vg, vb) = next_token_rgb(&mut iterator)?;
            loop {
                let token = match next_token(&mut iterator, false) {
                    Ok(v) => v,
                    Err(ParseError::EndOfLine) => break,
                    Err(e) => return Err(ColorDefinitionParseError::ParseError(e)),
                };

                match token.as_str() {
                    "ALPHA" => {
                        alpha = next_token_u32(&mut iterator)? as u8;
                    }
                    "LUMINANCE" => {
                        luminance = next_token_u32(&mut iterator)? as u8;
                    }
                    "FRACTION" => {
                        fraction = next_token_f32(&mut iterator)?;
                    }
                    "VFRACTION" => {
                        vfraction = next_token_f32(&mut iterator)?;
                    }
                    "SIZE" => {
                        size = next_token_u32(&mut iterator)?;
                    }
                    "MINSIZE" => {
                        minsize = next_token_u32(&mut iterator)?;
                    }
                    "MAXSIZE" => {
                        maxsize = next_token_u32(&mut iterator)?;
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
            let mut minsize = 0u32;
            let mut maxsize = 0u32;
            match next_token(&mut iterator, false)?.as_str() {
                "VALUE" => (),
                e => {
                    return Err(ColorDefinitionParseError::ParseError(
                        ParseError::InvalidToken(e.to_string()),
                    ));
                }
            };
            let (vr, vg, vb) = next_token_rgb(&mut iterator)?;
            loop {
                let token = match next_token(&mut iterator, false) {
                    Ok(v) => v,
                    Err(ParseError::EndOfLine) => break,
                    Err(e) => return Err(ColorDefinitionParseError::ParseError(e)),
                };

                match token.as_str() {
                    "ALPHA" => {
                        alpha = next_token_u32(&mut iterator)? as u8;
                    }
                    "LUMINANCE" => {
                        luminance = next_token_u32(&mut iterator)? as u8;
                    }
                    "FRACTION" => {
                        fraction = next_token_f32(&mut iterator)?;
                    }
                    "SIZE" => {
                        size = next_token_u32(&mut iterator)?;
                    }
                    "MINSIZE" => {
                        minsize = next_token_u32(&mut iterator)?;
                    }
                    "MAXSIZE" => {
                        maxsize = next_token_u32(&mut iterator)?;
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

pub fn parse_color_definition<T: BufRead>(
    reader: &mut T,
) -> Result<MaterialRegistry, ColorDefinitionParseError> {
    // Use an empty context here
    let materials = MaterialRegistry::new();
    let document = parse_single_document(&materials, reader)?;

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
    use super::{parse_color_definition, parse_multipart_document, parse_single_document};
    use crate::color::MaterialRegistry;
    use crate::error::{ColorDefinitionParseError, ParseError};
    use std::fs::File;
    use std::io::BufReader;

    const PATH_LDCONFIG: &str = "/home/segfault/.ldraw/LDConfig.ldr";
    const PATH_PART: &str = "/home/segfault/.ldraw/parts/u9318.dat";
    const PATH_MPD: &str = "/home/segfault/Downloads/6973.ldr";

    fn set_up_materials() -> Result<MaterialRegistry, ColorDefinitionParseError> {
        let mut reader = BufReader::new(File::open(PATH_LDCONFIG).unwrap());
        match parse_color_definition::<BufReader<File>>(&mut reader) {
            Ok(m) => Ok(m),
            Err(e) => Err(e),
        }
    }

    #[test]
    fn test_parse_color_definition() {
        let materials = set_up_materials().unwrap();

        println!("{:#?}\n", materials);
    }

    #[test]
    fn test_parse_single_document() {
        let materials = set_up_materials().unwrap();
        let mut reader_part = BufReader::new(File::open(PATH_PART).unwrap());
        match parse_single_document::<BufReader<File>>(&materials, &mut reader_part) {
            Ok(model) => {
                println!("{:#?}\n", model);
            }
            Err(e) => {
                assert!(false, "{}", e);
            }
        };

        let mut reader_mpd = BufReader::new(File::open(PATH_MPD).unwrap());
        match parse_single_document::<BufReader<File>>(&materials, &mut reader_mpd) {
            Ok(_) => {
                assert!(false, "Should not read properly");
            }
            Err(e) => {
                assert!(if let ParseError::MultipartDocument = e.error {
                    true
                } else {
                    false
                });
            }
        };
    }

    #[test]
    fn test_parse_multipart_document() {
        let materials = set_up_materials().unwrap();
        let f = File::open(PATH_MPD).unwrap();
        let mut reader = BufReader::new(f);

        match parse_multipart_document::<BufReader<File>>(&materials, &mut reader) {
            Ok(model) => {
                println!("{:#?}\n", model);
            }
            Err(e) => {
                assert!(false, "{}", e);
            }
        };
    }
}
