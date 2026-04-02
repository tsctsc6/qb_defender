use reqwest::{Client, RequestBuilder};
use serde_json::Value;
use std::collections::HashMap;
use thiserror::Error;
use tracing::info;

pub struct QbClient {
    client: Client,
    port: u16,
}

#[derive(Clone, Debug)]
pub struct Torrent {
    pub hash: String,
    pub size: u64,
}

#[derive(Clone, Debug)]
pub struct Peer {
    pub ip: String,
    pub port: u16,
    pub uploaded: u64,
    pub progress: f64,
    pub client: String,
}

#[derive(Error, Debug)]
pub enum Error {
    #[error("HTTP request error:\n{0}")]
    HttpRequestError(#[from] reqwest::Error),

    #[error("HTTP error:\n{0}")]
    HttpResponseError(String),

    #[error("Json error:\n{0}")]
    JsonError(#[from] serde_json::Error),

    #[error("Json type error:\n{0}")]
    JsonTypeError(String),
}

impl QbClient {
    pub fn new(port: u16) -> Self {
        QbClient {
            client: Client::new(),
            port,
        }
    }

    pub fn get_host(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }

    pub fn web_api_set_preferences(&self) -> RequestBuilder {
        self.client
            .post(self.get_host() + "/api/v2/app/setPreferences")
    }

    pub fn web_api_get_torrents_info(&self) -> RequestBuilder {
        self.client.get(self.get_host() + "/api/v2/torrents/info")
    }

    pub fn web_api_sync_torrent_peers(&self, hash: &str) -> RequestBuilder {
        self.client
            .get(self.get_host() + "/api/v2/sync/torrentPeers?hash=" + hash)
    }

    pub fn web_api_ban_peers(&self) -> RequestBuilder {
        self.client
            .post(self.get_host() + "/api/v2/transfer/banPeers")
    }

    pub async fn get_api_version(&self) -> Result<String, Error> {
        let resp = self
            .client
            .get(self.get_host() + "/api/v2/app/webapiVersion")
            .send()
            .await?;
        let text = resp.text().await?;
        Ok(text)
    }

    #[allow(non_snake_case)]
    pub async fn reset_banned_IPs(&self) -> Result<(), Error> {
        let resp = self
            .web_api_set_preferences()
            .form(&[("json", r#"{"banned_IPs":""}"#)])
            .send()
            .await?;
        if !resp.status().is_success() {
            return Err(Error::HttpResponseError(format!("{:#?}", resp)));
        }
        info!("Reset banned IPs!");
        Ok(())
    }

    pub async fn get_torrents(&self) -> Result<Vec<Torrent>, Error> {
        let resp = self.web_api_get_torrents_info().send().await?;
        if !resp.status().is_success() {
            return Err(Error::HttpResponseError(format!("{:#?}", resp)));
        }
        let content = resp.text().await?;
        let json_value: Value = serde_json::from_str(&content)?;
        let hash_array = match json_value.as_array() {
            Some(v) => v,
            None => {
                return Err(Error::JsonTypeError(format!(
                    "Expected a json array, got: {}",
                    content
                )));
            }
        };
        let torrent_array: Vec<_> = hash_array
            .iter()
            .filter_map(|p| {
                let hash = match p["hash"].as_str() {
                    Some(v) => v,
                    None => return None,
                };
                let size = match p["total_size"].as_u64() {
                    None => return None,
                    Some(v) => v,
                };
                Some(Torrent {
                    hash: String::from(hash),
                    size,
                })
            })
            .collect();
        Ok(torrent_array)
    }

    pub async fn get_peers(&self, hash: &str) -> Result<HashMap<String, Peer>, Error> {
        let resp = self.web_api_sync_torrent_peers(hash).send().await?;
        if !resp.status().is_success() {
            return Err(Error::HttpResponseError(format!("{:#?}", resp)));
        }
        let content = resp.text().await?;
        let json_value: Value = serde_json::from_str(&content)?;
        let json_value = match json_value["peers"].as_object() {
            Some(v) => v,
            None => {
                return Err(Error::JsonTypeError(format!(
                    "Expected a json object, got: {}",
                    content
                )));
            }
        };
        let hash_peers: HashMap<String, Peer> = json_value
            .iter()
            .filter_map(|(k, v)| {
                let ip = match v["ip"].as_str() {
                    Some(v) => v,
                    None => return None,
                };
                let port = match v["port"].as_i64() {
                    Some(v) => v,
                    None => return None,
                } as u16;
                let uploaded = match v["uploaded"].as_u64() {
                    Some(v) => v,
                    None => return None,
                };
                let progress = match v["progress"].as_f64() {
                    Some(v) => v,
                    None => return None,
                };
                let client = match v["client"].as_str() {
                    Some(v) => v,
                    None => return None,
                };
                Some((
                    String::from(k),
                    Peer {
                        ip: String::from(ip),
                        port,
                        uploaded,
                        progress,
                        client: String::from(client),
                    },
                ))
            })
            .collect();
        Ok(hash_peers)
    }

    pub async fn ban_peers(&self, peers: Vec<String>) -> Result<(), Error> {
        let peers = peers.join("|");
        let resp = self
            .web_api_ban_peers()
            .form(&[("peers", peers.as_str())])
            .send()
            .await?;
        if !resp.status().is_success() {
            return Err(Error::HttpResponseError(format!("{:#?}", resp)));
        }
        Ok(())
    }
}
