use async_std::io::BufReader;
use async_trait::async_trait;
use futures::join;
use reqwest::{Client, Error, Response, StatusCode, Url};

use crate::{
    color::MaterialRegistry,
    document::MultipartDocument,
    error::ResolutionError,
    library::{FileLoader, FileLocation, PartKind},
    parser::{parse_color_definition, parse_multipart_document},
    PartAlias,
};

pub struct HttpFileLoader {
    ldraw_url_base: Url,
    document_url_base: Url,

    client: Client,
}

impl HttpFileLoader {
    pub fn new(ldraw_url_base: Url, document_url_base: Url) -> Self {
        HttpFileLoader {
            ldraw_url_base,
            document_url_base,
            client: Client::new(),
        }
    }
}

#[async_trait]
impl FileLoader<String> for HttpFileLoader {
    async fn load_materials(&self) -> Result<MaterialRegistry, ResolutionError> {
        let url = self.ldraw_url_base.join("LDConfig.ldr").unwrap();
        let response = self.client.get(url).send().await?;
        if response.status() == StatusCode::NOT_FOUND {
            Err(ResolutionError::FileNotFound)
        } else {
            let bytes = response.bytes().await?;
            Ok(parse_color_definition(&mut BufReader::new(&*bytes)).await?)
        }
    }

    async fn load_document(
        &self,
        materials: &MaterialRegistry,
        locator: &String,
    ) -> Result<MultipartDocument, ResolutionError> {
        let url = match Url::parse(locator) {
            Ok(e) => e,
            Err(_) => return Err(ResolutionError::FileNotFound),
        };
        let bytes = self.client.get(url).send().await?.bytes().await?;

        Ok(parse_multipart_document(materials, &mut BufReader::new(&*bytes)).await?)
    }

    async fn load_ref(
        &self,
        materials: &MaterialRegistry,
        alias: PartAlias,
        local: bool,
    ) -> Result<(FileLocation, MultipartDocument), ResolutionError> {
        let parts_url = self.ldraw_url_base.join(&format!("parts/{}", alias.normalized)).unwrap();
        let p_url = self.ldraw_url_base.join(&format!("p/{}", alias.normalized)).unwrap();

        let parts_fut = self.client.get(parts_url).send();
        let p_fut = self.client.get(p_url).send();

        let (location, res) = if local {
            let local_url = self.document_url_base.join(&alias.normalized).unwrap();
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
        Ok((location, parse_multipart_document(materials, &mut BufReader::new(&*bytes)).await?))
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
