use async_std::{
    fs::File,
    io::BufReader,
    path::{Path, PathBuf},
};
use async_trait::async_trait;

use crate::{
    color::MaterialRegistry,
    document::MultipartDocument,
    error::ResolutionError,
    library::{FileLoader, FileLocation, PartKind},
    parser::{parse_color_definition, parse_multipart_document},
    PartAlias,
};

pub struct LocalFileLoader {
    ldrawdir: Box<PathBuf>,
    cwd: Box<PathBuf>,
}

impl LocalFileLoader {

    pub fn new(ldrawdir: &Path, cwd: &Path) -> Self {
        LocalFileLoader {
            ldrawdir: Box::new(ldrawdir.to_owned()),
            cwd: Box::new(cwd.to_owned()),
        }
    }

}

#[async_trait]
impl FileLoader for LocalFileLoader {

    async fn load_materials(&self) -> Result<MaterialRegistry, ResolutionError> {
        let path = {
            let mut path = self.ldrawdir.clone();
            path.push("LDConfig.ldr");
            path
        };

        if !path.exists().await {
            return Err(ResolutionError::FileNotFound);
        }

        Ok(parse_color_definition(
            &mut BufReader::new(File::open(&**path).await?)
        ).await?)
    }

    async fn load(&self, materials: &MaterialRegistry, alias: PartAlias, local: bool) -> Result<(FileLocation, MultipartDocument), ResolutionError> {
        let cwd_path = {
            let mut path = self.cwd.clone();
            path.push(alias.normalized.clone());
            path
        };
        let parts_path = {
            let mut path = self.ldrawdir.clone();
            path.push("parts");
            path.push(alias.normalized.clone());
            path
        };
        let p_path = {
            let mut path = self.ldrawdir.clone();
            path.push("p");
            path.push(alias.normalized.clone());
            path
        };

        let (kind, path) = if local && cwd_path.exists().await {
            (FileLocation::Local, &cwd_path)
        } else if parts_path.exists().await {
            (FileLocation::Library(PartKind::Part), &parts_path)
        } else if p_path.exists().await {
            (FileLocation::Library(PartKind::Primitive), &p_path)
        } else {
            return Err(ResolutionError::FileNotFound);
        };

        let document = parse_multipart_document(
            materials,
            &mut BufReader::new(File::open(&**path).await?)
        ).await?;

        Ok((kind, document))
    }

}
