use std::collections::HashMap;
use reqwest::{Error, RequestBuilder};
use serde_json::Value;
use crate::{Peer, QbClient, Torrent};

impl QbClient {
    pub(crate) fn get_host(&self) -> String {
        format!("http://127.0.0.1:{}", self.config.port)
    }

    pub(crate) fn web_api_set_preferences(&self) -> RequestBuilder {
        self.client.post(self.get_host() + "/api/v2/app/setPreferences")
    }

    pub(crate) fn web_api_get_torrents_info(&self) -> RequestBuilder {
        self.client.get(self.get_host() + "/api/v2/torrents/info")
    }

    pub(crate) fn web_api_sync_torrent_peers(&self, hash: &str) -> RequestBuilder {
        self.client.get(self.get_host() + "/api/v2/sync/torrentPeers?hash=" + hash)
    }

    pub(crate) fn web_api_ban_peers(&self) -> RequestBuilder {
        self.client.post(self.get_host() + "/api/v2/transfer/banPeers")
    }

    pub(crate) async fn get_api_version(&self) -> Result<String, Error>
    {
        let resp = self.client.get(self.get_host() + "/api/v2/app/webapiVersion")
            .send()
            .await?;
        let text = resp.text().await?;
        Ok(text)
    }

    #[allow(non_snake_case)]
    pub(crate) async fn reset_banned_IPs(&self) -> Result<(), String>
    {
        let result = self.web_api_set_preferences()
            .form(&[("json", r#"{"banned_IPs":""}"#)])
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
        log::log("Reset banned IPs!");
        Ok(())
    }

    pub(crate) async fn get_torrents(&self) -> Result<Vec<Torrent>, String>
    {
        let resp = match self.web_api_get_torrents_info().send().await {
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
        let torrent_array: Vec<_>= hash_array
            .iter().filter_map(|p| {
            let hash = match p["hash"].as_str() {
                Some(v) => v,
                None => return None
            };
            let size = match p["total_size"].as_u64() {
                None => return None,
                Some(v) => v,
            };
            Some(Torrent{hash: String::from(hash), size, peer_dic: HashMap::new()})
        }).collect();
        Ok(torrent_array)
    }

    pub(crate) async fn get_peers(&self, hash: &str) -> Result<HashMap<String, Peer>, String>
    {
        let resp = match self.web_api_sync_torrent_peers(hash).send().await {
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
        let hash_peers : HashMap<String, Peer> = json_value.iter().filter_map(|(k, v)| {
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
            Some((String::from(k), Peer{ip: String::from(ip), port, uploaded, progress, client: String::from(client), }))
        }).collect();
        Ok(hash_peers)
    }
}