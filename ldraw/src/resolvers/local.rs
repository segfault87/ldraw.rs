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
    library::{DocumentLoader, LibraryLoader, FileLocation, PartKind},
    parser::{parse_color_definition, parse_multipart_document},
    PartAlias,
};

pub struct LocalLoader {
    ldrawdir: Option<PathBuf>,
    cwd: Option<PathBuf>,
}

impl LocalLoader {
    pub fn new(ldrawdir: Option<PathBuf>, cwd: Option<PathBuf>) -> Self {
        LocalLoader {
            ldrawdir, cwd
        }
    }
}

#[async_trait(?Send)]
impl DocumentLoader<PathBuf> for LocalLoader {
    async fn load_document(
        &self,
        materials: &MaterialRegistry,
        locator: &PathBuf,
    ) -> Result<MultipartDocument, ResolutionError> {
        if !locator.exists().await {
            return Err(ResolutionError::FileNotFound);
        }

        Ok(
            parse_multipart_document(materials, &mut BufReader::new(File::open(locator).await?))
                .await?,
        )
    }
}

#[async_trait(?Send)]
impl LibraryLoader for LocalLoader {
    async fn load_materials(&self) -> Result<MaterialRegistry, ResolutionError> {
        let ldrawdir = match self.ldrawdir.clone() {
            Some(e) => e,
            None => return Err(ResolutionError::NoLDrawDir),
        };

        let path = {
            let mut path = ldrawdir.clone();
            path.push("LDConfig.ldr");
            path
        };

        if !path.exists().await {
            return Err(ResolutionError::FileNotFound);
        }

        Ok(parse_color_definition(&mut BufReader::new(File::open(&*path).await?)).await?)
    }

    async fn load_ref(
        &self,
        materials: &MaterialRegistry,
        alias: PartAlias,
        local: bool,
    ) -> Result<(FileLocation, MultipartDocument), ResolutionError> {
        let ldrawdir = match self.ldrawdir.clone() {
            Some(e) => e,
            None => return Err(ResolutionError::NoLDrawDir),
        };

        let cwd_path = self.cwd.as_ref().map(|v| {
            let mut path = v.clone();
            path.push(alias.normalized.clone());
            path
        });
        let parts_path = {
            let mut path = ldrawdir.clone();
            path.push("parts");
            path.push(alias.normalized.clone());
            path
        };
        let p_path = {
            let mut path = ldrawdir.clone();
            path.push("p");
            path.push(alias.normalized.clone());
            path
        };

        let (kind, path) = if local && cwd_path.is_some() && cwd_path.as_ref().unwrap().exists().await {
            (FileLocation::Local, cwd_path.as_ref().unwrap())
        } else if parts_path.exists().await {
            (FileLocation::Library(PartKind::Part), &parts_path)
        } else if p_path.exists().await {
            (FileLocation::Library(PartKind::Primitive), &p_path)
        } else {
            return Err(ResolutionError::FileNotFound);
        };

        let document =
            parse_multipart_document(materials, &mut BufReader::new(File::open(&**path).await?))
                .await?;

        Ok((kind, document))
    }
}
