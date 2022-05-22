use async_std::io::BufReader;
use async_trait::async_trait;
use futures::join;
use reqwest::{Client, Error, Response, StatusCode, Url};

use crate::{
    color::ColorCatalog,
    document::MultipartDocument,
    error::ResolutionError,
    library::{DocumentLoader, LibraryLoader, FileLocation, PartKind},
    parser::{parse_color_definitions, parse_multipart_document},
    PartAlias,
};

pub struct HttpLoader {
    ldraw_url_base: Option<Url>,
    document_url_base: Option<Url>,

    client: Client,
}

impl HttpLoader {
    pub fn new(ldraw_url_base: Option<Url>, document_url_base: Option<Url>) -> Self {
        HttpLoader {
            ldraw_url_base,
            document_url_base,
            client: Client::new(),
        }
    }
}

#[async_trait(?Send)]
impl DocumentLoader<String> for HttpLoader {
    async fn load_document(
        &self,
        locator: &String,
        colors: &ColorCatalog,
    ) -> Result<MultipartDocument, ResolutionError> {
        let url = match Url::parse(locator) {
            Ok(e) => e,
            Err(_) => return Err(ResolutionError::FileNotFound),
        };
        let bytes = self.client.get(url).send().await?.bytes().await?;

        Ok(parse_multipart_document(&mut BufReader::new(&*bytes), colors).await?)
    }
}

#[async_trait(?Send)]
impl LibraryLoader for HttpLoader {
    async fn load_colors(&self) -> Result<ColorCatalog, ResolutionError> {
        let ldraw_url_base = self.ldraw_url_base.as_ref();
        let ldraw_url_base = match ldraw_url_base {
            Some(ref e) => e,
            None => return Err(ResolutionError::NoLDrawDir),
        };

        let url = ldraw_url_base.join("LDConfig.ldr").unwrap();
        let response = self.client.get(url).send().await?;
        if response.status() == StatusCode::NOT_FOUND {
            Err(ResolutionError::FileNotFound)
        } else {
            let bytes = response.bytes().await?;
            Ok(parse_color_definitions(&mut BufReader::new(&*bytes)).await?)
        }
    }

    async fn load_ref(
        &self,
        alias: PartAlias,
        local: bool,
        colors: &ColorCatalog,
    ) -> Result<(FileLocation, MultipartDocument), ResolutionError> {
        let ldraw_url_base = self.ldraw_url_base.as_ref();
        let ldraw_url_base = match ldraw_url_base {
            Some(ref e) => e,
            None => return Err(ResolutionError::NoLDrawDir),
        };

        let parts_url = ldraw_url_base.join(&format!("parts/{}", alias.normalized)).unwrap();
        let p_url = ldraw_url_base.join(&format!("p/{}", alias.normalized)).unwrap();

        let parts_fut = self.client.get(parts_url).send();
        let p_fut = self.client.get(p_url).send();

        let (location, res) = if local && self.document_url_base.is_some() {
            let document_url_base = self.document_url_base.as_ref().unwrap();

            let local_url = document_url_base.join(&alias.normalized).unwrap();
            let local_fut = self.client.get(local_url).send();
            let (local, parts, p) = join!(local_fut, parts_fut, p_fut);

            if let Some(v) = select_response(local) {
                (FileLocation::Local, v)
            } else if let Some(v) = select_response(parts) {
                (FileLocation::Library(PartKind::Part), v)
            } else if let Some(v) = select_response(p) {
                (FileLocation::Library(PartKind::Primitive), v)
            } else {
                return Err(ResolutionError::FileNotFound);
            }
        } else {
            let (parts, p) = join!(parts_fut, p_fut);
            if let Some(v) = select_response(parts) {
                (FileLocation::Library(PartKind::Part), v)
            } else if let Some(v) = select_response(p) {
                (FileLocation::Library(PartKind::Primitive), v)
            } else {
                return Err(ResolutionError::FileNotFound);
            }
        };

        let bytes = res.bytes().await?;
        Ok((location, parse_multipart_document(&mut BufReader::new(&*bytes), colors).await?))
    }
}

fn select_response(response: Result<Response, Error>) -> Option<Response> {
    match response {
        Ok(r) => {
            if r.status() == StatusCode::OK {
                Some(r)
            } else {
                None
            }
        },
        Err(_) => None,
    }
}
