use crate::command::Cli;
use crate::log;
use chrono::{DateTime, Duration, Local};
use reqwest::{Client, Error, RequestBuilder};
use serde_json::Value;
use std::collections::HashMap;
use tokio::time::sleep;

pub struct QbClient {
    client: Client,
    config: Cli,
    last_reset_time: DateTime<Local>,
    peer_dic: HashMap<String, Peer>,
}

impl QbClient {
    pub fn new(cli: Cli) -> Self {
        QbClient{client: Client::new(), config: cli, last_reset_time: Local::now(), peer_dic: HashMap::new() }
    }

    pub async fn wait(&self) {
        sleep(std::time::Duration::from_secs(self.config.interval)).await;
    }

    fn get_host(&self) -> String {
        format!("http://127.0.0.1:{}", self.config.port)
    }

    fn api_set_preferences(&self) -> RequestBuilder {
        self.client.post(self.get_host() + "/api/v2/app/setPreferences")
    }

    fn api_get_torrents_info(&self) -> RequestBuilder {
        self.client.get(self.get_host() + "/api/v2/torrents/info")
    }

    fn api_sync_torrent_peers(&self, hash: &str) -> RequestBuilder {
        self.client.get(self.get_host() + "/api/v2/sync/torrentPeers?hash=" + hash)
    }

    async fn get_api_version(&self) -> Result<String, Error>
    {
        let resp = self.client.get(self.get_host() + "/api/v2/app/webapiVersion")
            .send()
            .await?;
        let text = resp.text().await?;
        Ok(text)
    }

    pub async fn ensure_api_version(&self) -> Result<(), String>
    {
        let api_version = loop {
            match self.get_api_version().await
            {
                Ok(version) => break version,
                Err(_) => {
                    log::log(format!("Can't connect to qBittorrent WebUI, wait {} seconds to reconnect!",
                        self.config.interval).as_str());
                    self.wait().await;
                }
            }
        };
        let api_versions = api_version.split('.')
            .map(|s| {
                match s.parse::<i32>(){
                    Ok(value) => value,
                    Err(_) => {
                        panic!("Can't parse qBittorrent WebUI version to i32: {}", api_version);
                    }
                }
            })
            .collect::<Vec<i32>>();
        if api_versions[0] < 2 || api_versions[1] < 3 {
            return Err("Need QBittorrent API version >= 2.3.0".to_string())
        };
        Ok(())
    }

    #[allow(non_snake_case)]
    pub async fn reset_banned_IPs(&self) -> Result<(), String>
    {
        let result = self.api_set_preferences()
            .form(&[("json", r#""{"banned_IPs":""}""#)])
            .send()
            .await;
        let resp = match result {
            Ok(resp) => resp,
            Err(e) => {
                return Err(format!("Can't reset QBittorrent IP:\n{:#?}", e));
            }
        };
        if !resp.status().is_success() {
            return Err(format!("Can't reset QBittorrent IPs:\n{:#?}", resp));
        }
        Ok(())
    }

    #[allow(non_snake_case)]
    pub async fn try_reset_banned_IPs(&self) -> Result<(), String>
    {
        if Local::now() - self.last_reset_time > Duration::days(1) {
            self.reset_banned_IPs().await?;
            self.last_reset_time = Local::now();
        }
        Ok(())
    }

    pub async fn record_peers(&mut self) -> Result<(), String>
    {
        let resp = match self.api_get_torrents_info().send().await {
            Ok(resp) => resp,
            Err(e) => return Err(format!("Can't get QBittorrent torrents info:\n{:#?}", e))
        };
        if !resp.status().is_success() {
            return Err(format!("Can't get QBittorrent torrents info:\n{:#?}", resp));
        }
        let content = match resp.text().await {
            Ok(t) => t,
            Err(e) => return Err(format!("Can't get QBittorrent torrents info:\n{:#?}", e)),
        };
        let json_value: Value = match serde_json::from_str(&content){
            Ok(v) => v,
            Err(e) => return Err(format!("Can't get QBittorrent torrents info:\n{:#?}", e))
        };
        let hash_array = match json_value.as_array() {
            Some(v) => v,
            None => return Err("Can't get QBittorrent torrents info".to_string()),
        };
        let hash_array: Vec<&str> = hash_array
            .iter().filter_map(|p| {
                match p["hash"].as_str() {
                    Some(v) => Some(v),
                    None => return None
                }
            })
            .collect();
        for hash in hash_array {
            let resp = match self.api_sync_torrent_peers(hash).send().await {
                Ok(resp) => resp,
                Err(e) => return Err(format!("Can't get QBittorrent torrents info: {}\n{}", hash, e)),
            };
            if !resp.status().is_success() {
                return Err(format!("Can't get QBittorrent torrents info: {}\n{}", hash, resp.status()))
            }
            let content = match resp.text().await {
                Ok(t) => t,
                Err(e) => return Err(format!("Can't get QBittorrent torrents info: {}\n{}", hash, e)),
            };
            let json_value: Value = match serde_json::from_str(&content){
                Ok(v) => v,
                Err(e) => return Err(format!("Can't get QBittorrent torrents info: {}\n{}", hash, e)),
            };
            let json_value = match json_value["peers"].as_object() {
                Some(v) => v,
                None => return Err(format!("Can't get QBittorrent torrents info: {}", hash)),
            };
            for (key, value) in json_value.iter() {
                let ip = match value["ip"].as_str() {
                    Some(v) => v,
                    None => return Err(format!("Can't get QBittorrent torrents info: {}, {}", hash, key)),
                };
                let port = match value["port"].as_i64() {
                    Some(v) => v,
                    None => return Err(format!("Can't get QBittorrent torrents info: {}, {}", hash, key)),
                } as u16;
                let uploaded = match value["uploaded"].as_u64() {
                    Some(v) => v,
                    None => return Err(format!("Can't get QBittorrent torrents info: {}, {}", hash, key)),
                };
                match self.peer_dic.get_mut(key) {
                    None => {
                        match self.peer_dic.insert(String::from(key),
                                             Peer{ip: String::from(ip), port, uploaded, last_uploaded: uploaded }) {
                            None => {}
                            Some(_) => {}
                        }
                    },
                    Some(v) => {
                        v.last_uploaded = v.uploaded;
                        v.uploaded = uploaded;
                    }
                }
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct Peer {
    ip: String,
    port: u16,
    uploaded: u64,
    last_uploaded: u64
}