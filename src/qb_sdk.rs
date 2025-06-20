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
    torrent_dic: HashMap<String, Torrent>,
}

impl QbClient {
    pub fn new(cli: Cli) -> Self {
        QbClient{client: Client::new(), config: cli, last_reset_time: Local::now(), torrent_dic: HashMap::new() }
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

    fn api_ban_peers(&self) -> RequestBuilder {
        self.client.post(self.get_host() + "/api/v2/transfer/banPeers")
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
    pub async fn try_reset_banned_IPs(&mut self) -> Result<(), String>
    {
        if Local::now() - self.last_reset_time > Duration::days(1) {
            self.reset_banned_IPs().await?;
            self.last_reset_time = Local::now();
        }
        Ok(())
    }

    pub async fn record_and_ban_peers(&mut self) -> Result<(), String>
    {
        // 获取所有 peers 的信息
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
            })
            .collect();
        {
            // 记录 peers 的信息
            let torrent_dic = &mut self.torrent_dic;
            for torrent in torrent_array {
                match torrent_dic.get(torrent.hash.as_str()) {
                    None => {
                        torrent_dic.insert(String::from(torrent.hash.as_str()), torrent.clone());
                    }
                    Some(_) => {}
                }
            }
        }
        // hsah, ip, info
        let mut torrent_ip_peer: HashMap<String, HashMap<String, Peer>> = HashMap::with_capacity(self.torrent_dic.len());
        {
            // 获取连接到这个 peers 的所有 ip
            let torrent_dic = &self.torrent_dic;
            for (hash, _) in torrent_dic {
                let resp = match self.api_sync_torrent_peers(hash.as_str()).send().await {
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
                torrent_ip_peer.insert(String::from(hash), hash_peers);
            }
        }

        // 移除没有出现的 peer
        for (_, torrent) in self.torrent_dic.iter_mut() {
            let ips = torrent.peer_dic.iter().filter_map(|(k, _)| {
                Some(String::from(k))
            }).collect::<Vec<String>>();
            for ip in ips {
                if !torrent_ip_peer.contains_key(ip.as_str()) {
                    torrent.peer_dic.remove(ip.as_str());
                }
            }
        }

        let mut ban_peers: Vec<String> = vec![];
        // 更新 peer 信息，并判断是否 ban
        for (hash, peers) in torrent_ip_peer.iter() {
            let torrent_size =  *&self.torrent_dic[hash.as_str()].size;
            let old_torrent = match self.torrent_dic.get_mut(hash.as_str()) {
                None => {
                    log::log(&format!("Can't get QBittorrent peers from local dic: {:#?}", hash));
                    continue;
                }
                Some(v) => v
            };
            for (ip_port, peer) in peers.iter() {
                let old_peer = match old_torrent.peer_dic.insert(String::from(ip_port), peer.clone()) {
                    None => { continue; }
                    Some(v) => v
                };
                if QbClient::judge_banned(&old_peer, peer) {
                    ban_peers.push(String::from(ip_port));
                }
            }
        }

        let resp = match self.api_ban_peers()
            .form(&["peers", ban_peers.join("|").as_str()])
            .send().await {
            Ok(resp) => resp,
            Err(e) => {
                return Err(format!("Can't ban peers:\n{}", e));
            }
        };
        if !resp.status().is_success() {
            return Err(format!("Can't get QBittorrent torrents info:\n{}", resp.status()))
        }

        Ok(())
    }
    fn judge_banned(old: &Peer, new: &Peer) -> bool
    {
        false
    }
}

#[derive(Clone, Debug)]
pub struct Torrent {
    hash: String,
    size: u64,
    peer_dic: HashMap<String, Peer>
}

#[derive(Clone, Debug)]
pub struct Peer {
    ip: String,
    port: u16,
    uploaded: u64,
    progress: f64,
    client: String,
}