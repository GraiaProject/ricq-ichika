use ricq_core::error::RQError;
use std::collections::HashMap;

pub(crate) fn make_headers(
    version: impl Into<String>,
    cookie: impl Into<String>,
) -> HashMap<String, String> {
    let version = version.into();
    let cookie = cookie.into();
    let mut headers: HashMap<String, String> = HashMap::new();
    headers.insert("User-Agent".into(), format!("QQ/{version} CFNetwork/1126"));
    headers.insert("Net-Type".into(), "Wifi".into());
    if !cookie.is_empty() {
        headers.insert("Cookie".into(), cookie);
    }
    headers
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HttpMethod {
    GET,
    POST,
}

#[async_trait::async_trait]
pub trait HttpClient {
    async fn make_request(
        &mut self,
        method: HttpMethod,
        url: String,
        header: &HashMap<String, String>,
        body: bytes::Bytes,
    ) -> Result<bytes::Bytes, RQError>;
}

#[cfg(feature = "http-reqwest")]
mod http_reqwest {
    use super::*;
    use reqwest::header::HeaderMap;
    use reqwest::Error as ReqErr;
    use reqwest::{Client, Method as ReqMethod};
    use std::io::{Error as IOErr, ErrorKind as IOErrKind};

    impl From<HttpMethod> for ReqMethod {
        fn from(value: HttpMethod) -> Self {
            match value {
                HttpMethod::GET => ReqMethod::GET,
                HttpMethod::POST => ReqMethod::POST,
            }
        }
    }

    fn map_req_err(value: ReqErr) -> RQError {
        if value.is_builder() {
            RQError::Other(value.to_string())
        } else {
            RQError::IO(IOErr::new(IOErrKind::Other, value))
        }
    }

    #[async_trait::async_trait]
    impl HttpClient for Client {
        async fn make_request(
            &mut self,
            method: HttpMethod,
            url: String,
            headers: &HashMap<String, String>,
            body: bytes::Bytes,
        ) -> Result<bytes::Bytes, RQError> {
            let headers: HeaderMap =
                HeaderMap::try_from(headers).map_err(|e| RQError::Other(e.to_string()))?;
            let req = self
                .request(method.into(), url)
                .headers(headers)
                .body(body)
                .build()
                .map_err(map_req_err)?;
            Ok(self
                .execute(req)
                .await
                .map_err(map_req_err)?
                .bytes()
                .await
                .map_err(map_req_err)?)
        }
    }
}
