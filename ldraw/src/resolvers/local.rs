use std::path::PathBuf;

use async_trait::async_trait;
use tokio::{
    fs::{try_exists, File},
    io::BufReader,
};

use crate::{
    color::ColorCatalog,
    document::MultipartDocument,
    error::ResolutionError,
    library::{DocumentLoader, FileLocation, LibraryLoader, PartKind},
    parser::{parse_color_definitions, parse_multipart_document},
    PartAlias,
};

pub struct LocalLoader {
    ldrawdir: Option<PathBuf>,
    cwd: Option<PathBuf>,
}

impl LocalLoader {
    pub fn new(ldrawdir: Option<PathBuf>, cwd: Option<PathBuf>) -> Self {
        LocalLoader { ldrawdir, cwd }
    }
}

#[async_trait(?Send)]
impl DocumentLoader<PathBuf> for LocalLoader {
    async fn load_document(
        &self,
        locator: &PathBuf,
        colors: &ColorCatalog,
    ) -> Result<MultipartDocument, ResolutionError> {
        if !try_exists(&locator).await? {
            return Err(ResolutionError::FileNotFound);
        }

        Ok(
            parse_multipart_document(&mut BufReader::new(File::open(locator).await?), colors)
                .await?,
        )
    }
}

#[async_trait(?Send)]
impl LibraryLoader for LocalLoader {
    async fn load_colors(&self) -> Result<ColorCatalog, ResolutionError> {
        let ldrawdir = match self.ldrawdir.clone() {
            Some(e) => e,
            None => return Err(ResolutionError::NoLDrawDir),
        };

        let path = {
            let mut path = ldrawdir.clone();
            path.push("LDConfig.ldr");
            path
        };

        if !try_exists(&path).await? {
            return Err(ResolutionError::FileNotFound);
        }

        Ok(parse_color_definitions(&mut BufReader::new(File::open(&*path).await?)).await?)
    }

    async fn load_ref(
        &self,
        alias: PartAlias,
        local: bool,
        colors: &ColorCatalog,
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

        let (kind, path) =
            if local && cwd_path.is_some() && try_exists(&cwd_path.as_ref().unwrap()).await? {
                (FileLocation::Local, cwd_path.as_ref().unwrap())
            } else if try_exists(&parts_path).await? {
                (FileLocation::Library(PartKind::Part), &parts_path)
            } else if try_exists(&p_path).await? {
                (FileLocation::Library(PartKind::Primitive), &p_path)
            } else {
                return Err(ResolutionError::FileNotFound);
            };

        let document =
            parse_multipart_document(&mut BufReader::new(File::open(&**path).await?), colors)
                .await?;

        Ok((kind, document))
    }
}
