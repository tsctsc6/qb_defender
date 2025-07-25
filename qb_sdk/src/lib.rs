mod web_api;

use chrono::{DateTime, Duration, Local};
use command::Cli;
use ip_network::IpNetwork;
use log;
use reqwest::Client;
use std::collections::HashMap;
use std::net::{Ipv4Addr, Ipv6Addr};
use tokio::time::sleep;

const F64_ERROR : f64 = 0.00001;

const LEECH_CLIENTS: [&str; 34] = ["-XL", "Xunlei", "XunLei", "7.", "aria2", "Xfplay", "dandanplay", "FDM", "go.torrent", "Mozilla",
    "github.com/anacrolix/torrent (devel) (anacrolix/torrent unknown)", "dt/torrent/", "Taipei-Torrent dev",
    "trafficConsume", "hp/torrent/", "BitComet 1.92", "BitComet 1.98", "xm/torrent/", "flashget", "FlashGet",
    "StellarPlayer", "Gopeed", "MediaGet", "aD/", "ADM", "coc_coc_browser", "FileCroc", "filecxx", "Folx",
    "seanime (devel) (anacrolix/torrent", "HitomiDownloader", "gateway (devel) (anacrolix/torrent",
    "offline-download (devel) (anacrolix/torrent", "QQDownload"];

const ANCIENT_CLIENTS: [&str; 16] = ["TorrentStorm", "Azureus 1.", "Azureus 2.", "Azureus 3.", "Deluge 0.", "Deluge 1.0", "Deluge 1.1",
    "qBittorrent 0.", "qBittorrent 1.", "qBittorrent 2.", "Transmission 0.", "Transmission 1.", "BitComet 0.",
    "µTorrent 1.", "uTorrent 1.", "μTorrent 1."];

pub struct QbClient {
    client: Client,
    config: Cli,
    last_reset_time: DateTime<Local>,
    torrent_dic: HashMap<String, Torrent>,
    network_dic: HashMap<String, u64>,
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

impl QbClient {
    pub fn new(cli: Cli) -> Self {
        QbClient{client: Client::new(), config: cli,
            last_reset_time: Local::now() - Duration::days(2), torrent_dic: HashMap::new(),
            network_dic: HashMap::new(),
        }
    }

    pub async fn wait(&self) {
        sleep(std::time::Duration::from_secs(self.config.interval)).await;
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
    pub async fn try_reset_banned_IPs(&mut self) -> Result<(), String>
    {
        if Local::now() - self.last_reset_time > Duration::days(1) {
            self.reset_banned_IPs().await?;
            for torrent in self.torrent_dic.values_mut() {
                torrent.peer_dic.clear();
            }
            self.network_dic.clear();
            self.last_reset_time = Local::now();
        }
        Ok(())
    }

    pub async fn record_and_ban_peers(&mut self) -> Result<(), String>
    {
        let torrent_array_from_qb = self.get_torrents().await?;
        for torrent in torrent_array_from_qb {
            match self.torrent_dic.get(torrent.hash.as_str()) {
                None => {
                    self.torrent_dic.insert(String::from(torrent.hash.as_str()), torrent.clone());
                }
                Some(_) => {}
            }
        }
        // hsah, ip, info
        let mut torrent_ip_peer_from_qb: HashMap<String, HashMap<String, Peer>> = HashMap::with_capacity(self.torrent_dic.len());
        {
            let torrent_dic = &self.torrent_dic;
            for (hash, _) in torrent_dic {
                let hash_peers = self.get_peers(hash.as_str()).await?;
                torrent_ip_peer_from_qb.insert(String::from(hash), hash_peers);
            }
        }

        // 移除没有出现的 peer
        for (_, torrent) in self.torrent_dic.iter_mut() {
            let ip_ports_from_self = torrent.peer_dic.iter().filter_map(|(k, _)| {
                Some(String::from(k))
            }).collect::<Vec<String>>();
            let torrent_from_torrent_ip_peer_from_qb =
                match torrent_ip_peer_from_qb.get(torrent.hash.as_str()) {
                Some(v) => v,
                None => continue,
            };
            for ip_port in ip_ports_from_self {
                if !torrent_from_torrent_ip_peer_from_qb.contains_key(ip_port.as_str()) {
                    torrent.peer_dic.remove(ip_port.as_str());
                }
            }
        }

        let mut ban_peers: Vec<String> = vec![];
        // 更新 peer 信息，并判断是否 ban
        for (hash, peers) in torrent_ip_peer_from_qb.iter() {
            let torrent_size =  *&self.torrent_dic[hash.as_str()].size;
            let old_torrent = match self.torrent_dic.get_mut(hash.as_str()) {
                None => {
                    log::log(&format!("Can't get QBittorrent peers from local dic: {:#?}", hash));
                    continue;
                }
                Some(v) => v
            };
            for (ip_port, peer) in peers.iter() {
                let network = match Self::get_network(peer.ip.as_str()) {
                    None => return Err(format!("Can not get network for {}", peer.ip.as_str())),
                    Some(v) => v
                };
                if Self::judge_banned_1(peer, torrent_size, network.as_str(), &self.network_dic) {
                    ban_peers.push(String::from(ip_port));
                    match self.network_dic.get_mut(network.as_str()) {
                        None => {
                            self.network_dic.insert(network.clone(), 1);
                        },
                        Some(v) => *v = *v + 1,
                    };
                    continue;
                }
                let old_peer = old_torrent.peer_dic.insert(String::from(ip_port), peer.clone());
                let old_peer = match old_peer{
                    None => {
                        continue;
                    }
                    Some(v) => v
                };
                if Self::judge_banned_2(&old_peer, peer, torrent_size) {
                    ban_peers.push(String::from(ip_port));
                    match self.network_dic.get_mut(network.as_str()) {
                        None => {
                            self.network_dic.insert(network.clone(), 1);
                        },
                        Some(v) => *v = *v + 1,
                    };
                }
            }
        }

        if ban_peers.len() == 0 {
            return Ok(())
        };
        println!("network {:#?}", self.network_dic);
        let peers = ban_peers.join("|");
        let resp = match self.web_api_ban_peers()
            .form(&[("peers", peers.as_str())])
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

    fn judge_banned_1(new: &Peer, torrent_size: u64, network: &str, network_dic: &HashMap<String, u64>) -> bool {
        // 客户端名称只允许：
        // ASCII 字符（Unicode 码点 0x20（空格） 到 0x7E（'~'））
        // 'µ'（0xB5），'μ'（0x03BC）
        /*for c in new.client.chars() {
            if c < ' ' || (c > '~' && c != 'µ' && c != 'μ') {
                log::log(format!("Banned - Weird Client: {}:{}, \"{}\"", new.ip, new.port, new.client).as_str());
                return true;
            }
        }*/

        // 诡异客户端
        /*if new.client.chars().count() < 4 || new.client.chars().collect::<Vec<_>>()[2] == ' '
            || new.client.starts_with("Unknown") {
            log::log(format!("Banned - Weird Client: {}:{}, \"{}\"", new.ip, new.port, new.client).as_str());
            return true;
        }*/

        // 吸血客户端
        if LEECH_CLIENTS.contains(&new.client.as_str()) {
            log::log(format!("Banned - Leech Client: {}:{}", new.ip, new.port).as_str());
            return true;
        }

        // 上古客户端
        if ANCIENT_CLIENTS.contains(&new.client.as_str()) {
            log::log(format!("Banned - Ancient Client: {}:{}", new.ip, new.port).as_str());
            return true;
        }

        // 通过网段禁用
        match network_dic.get(network) {
            None => {}
            Some(count) => {
                if *count >= 5 {
                    log::log(format!("Banned - Same network client: {}:{}", new.ip, new.port).as_str());
                    return true;
                }
            }
        }

        // 总上传 大于 报告进度 * 种子大小 + 10 MB
        if new.uploaded > (new.progress * torrent_size as f64) as u64 + 10 * 1024 * 1024 {
            log::log(format!("Banned - Too much upload: {}:{}", new.ip, new.port).as_str());
            return true;
        }
        
        false
    }

    fn judge_banned_2(old: &Peer, new: &Peer, torrent_size: u64) -> bool {
        // 进度倒退
        if new.progress < old.progress {
            log::log(format!("Banned - Progress is regressive: {}:{}", new.ip, new.port).as_str());
            return true;
        }

        // 进度增量小于上传增量
        let diff_uploaded = new.uploaded - old.uploaded;
        let diff_progress = new.progress - old.progress;
        if diff_progress < (diff_uploaded as f64 / torrent_size as f64) - F64_ERROR  {
            log::log(format!("Banned - Progress is not expected: {}:{}", new.ip, new.port).as_str());
            return true;
        }

        false
    }

    fn get_network(ip: &str) -> Option<String> {
        if let Ok(addr) = ip.parse::<Ipv4Addr>() {
            let network = IpNetwork::new_truncate(addr, 24).unwrap();
            Some(network.to_string())
        }
        else if let Ok(addr) = ip.parse::<Ipv6Addr>() {
            let network = IpNetwork::new_truncate(addr, 64).unwrap();
            Some(network.to_string())
        }
        else {
            None
        }
    }
}