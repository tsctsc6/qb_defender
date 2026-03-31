use chrono::{DateTime, Duration, Local};
use ip_network::IpNetwork;
use qb_sdk::Peer;
use qb_sdk::QbClient;
use qb_sdk::Torrent;
use std::collections::HashMap;
use std::net::{Ipv4Addr, Ipv6Addr};
use thiserror::Error;
use tokio::time::sleep;
use tracing::{debug, error, info};

const F64_ERROR: f64 = 0.00001;

const LEECH_CLIENTS: [&str; 36] = [
    "-XL",
    "Xunlei",
    "XunLei",
    "7.",
    "aria2",
    "Xfplay",
    "dandanplay",
    "FDM",
    "go.torrent",
    "Mozilla",
    "github.com/anacrolix/torrent (devel) (anacrolix/torrent unknown)",
    "dt/torrent/",
    "Taipei-Torrent dev",
    "trafficConsume",
    "hp/torrent/",
    "BitComet 1.92",
    "BitComet 1.98",
    "xm/torrent/",
    "flashget",
    "FlashGet",
    "StellarPlayer",
    "Gopeed",
    "MediaGet",
    "aD/",
    "ADM",
    "coc_coc_browser",
    "FileCroc",
    "filecxx",
    "Folx",
    "seanime (devel) (anacrolix/torrent",
    "HitomiDownloader",
    "gateway (devel) (anacrolix/torrent",
    "offline-download",
    "QQDownload",
    "git.woa.com",
    "iLivid",
];

const ANCIENT_CLIENTS: [&str; 16] = [
    "TorrentStorm",
    "Azureus 1.",
    "Azureus 2.",
    "Azureus 3.",
    "Deluge 0.",
    "Deluge 1.0",
    "Deluge 1.1",
    "qBittorrent 0.",
    "qBittorrent 1.",
    "qBittorrent 2.",
    "Transmission 0.",
    "Transmission 1.",
    "BitComet 0.",
    "µTorrent 1.",
    "uTorrent 1.",
    "μTorrent 1.",
];

#[derive(Error, Debug)]
pub enum Error {
    #[error("QBittorrent SDK error:\n{0}")]
    QBSDKError(#[from] qb_sdk::Error),

    #[error("Parse QBittorrent API version error:\n{0}")]
    ParseQBAPIVersionError(#[from] std::num::ParseIntError),

    #[error("QBittorrent API version error:\n{0}")]
    QBAPIVersionError(String),

    #[error("Get network error:\n{0}")]
    GetNetworkError(#[from] GetNetworkError),
}

#[derive(Error, Debug)]
pub enum GetNetworkError {
    #[error("IP address parse error:\n{0}")]
    IPAddrParseError(#[from] std::net::AddrParseError),

    #[error("Invalid IP address:\n{0}")]
    IpNetworkError(#[from] ip_network::IpNetworkError),
}

pub struct Application {
    qb_client: QbClient,
    interval: u64,
    last_reset_time: DateTime<Local>,
    /// hash, torrent
    torrent_dic: HashMap<String, Torrent>,
    /// network, banned ip count
    network_dic: HashMap<String, u64>,
}

impl Application {
    pub fn new(qb_client: QbClient, interval: u64) -> Self {
        Application {
            qb_client,
            interval,
            last_reset_time: Local::now() - Duration::days(2),
            torrent_dic: HashMap::new(),
            network_dic: HashMap::new(),
        }
    }

    pub async fn wait(&self) {
        sleep(std::time::Duration::from_secs(self.interval)).await;
    }

    pub async fn ensure_api_version(&self) -> Result<(), Error> {
        let api_version = loop {
            match self.qb_client.get_api_version().await {
                Ok(version) => break version,
                Err(_) => {
                    error!(
                        "Can't connect to qBittorrent WebUI, wait {} seconds to reconnect!",
                        self.interval,
                    );
                    self.wait().await;
                }
            }
        };
        let api_versions = api_version
            .split('.')
            .map(|s| s.parse::<i32>())
            .collect::<Result<Vec<i32>, std::num::ParseIntError>>()?;
        if api_versions[0] < 2 {
            return Err(Error::QBAPIVersionError(
                "Need QBittorrent API version >= 2.3.0".to_string(),
            ));
        }
        if api_versions[0] == 2 && api_versions[1] < 3 {
            return Err(Error::QBAPIVersionError(
                "Need QBittorrent API version >= 2.3.0".to_string(),
            ));
        };
        Ok(())
    }

    #[allow(non_snake_case)]
    pub async fn try_reset_banned_IPs(&mut self) -> Result<(), Error> {
        if Local::now() - self.last_reset_time > Duration::days(1) {
            self.qb_client.reset_banned_IPs().await?;
            for torrent in self.torrent_dic.values_mut() {
                torrent.peer_dic.clear();
            }
            self.network_dic.clear();
            self.last_reset_time = Local::now();
        }
        Ok(())
    }

    pub async fn record_and_ban_peers(&mut self) -> Result<(), Error> {
        let torrent_array_from_qb = self.qb_client.get_torrents().await?;
        for torrent in torrent_array_from_qb {
            match self.torrent_dic.get(torrent.hash.as_str()) {
                None => {
                    self.torrent_dic
                        .insert(String::from(torrent.hash.as_str()), torrent.clone());
                }
                Some(_) => {}
            }
        }
        // hsah, ip, info
        let mut torrent_ip_peer_from_qb: HashMap<String, HashMap<String, Peer>> =
            HashMap::with_capacity(self.torrent_dic.len());
        {
            let torrent_dic = &self.torrent_dic;
            for (hash, _) in torrent_dic {
                let hash_peers = self.qb_client.get_peers(hash.as_str()).await?;
                torrent_ip_peer_from_qb.insert(String::from(hash), hash_peers);
            }
        }

        // 移除qb那边没有出现的 peer
        for (_, torrent) in self.torrent_dic.iter_mut() {
            let ip_ports_from_this_process = torrent
                .peer_dic
                .iter()
                .filter_map(|(k, _)| Some(String::from(k)))
                .collect::<Vec<String>>();
            let torrent_from_torrent_ip_peer_from_qb =
                match torrent_ip_peer_from_qb.get(torrent.hash.as_str()) {
                    Some(v) => v,
                    None => continue,
                };
            for ip_port in ip_ports_from_this_process {
                if !torrent_from_torrent_ip_peer_from_qb.contains_key(ip_port.as_str()) {
                    torrent.peer_dic.remove(ip_port.as_str());
                }
            }
        }

        let mut ban_peers: Vec<String> = vec![];
        // 更新 peer 信息，并判断是否 ban
        for (hash, peers) in torrent_ip_peer_from_qb.iter() {
            let torrent_size = *&self.torrent_dic[hash.as_str()].size;
            let old_torrent = match self.torrent_dic.get_mut(hash.as_str()) {
                None => {
                    error!("Can't get QBittorrent peers from local dic: {:#?}", hash);
                    continue;
                }
                Some(v) => v,
            };
            for (ip_port, peer) in peers.iter() {
                let network = Self::get_network(peer.ip.as_str())?;
                if Self::judge_banned_1(peer, torrent_size, network.as_str(), &self.network_dic) {
                    ban_peers.push(String::from(ip_port));
                    match self.network_dic.get_mut(network.as_str()) {
                        None => {
                            self.network_dic.insert(network.clone(), 1);
                        }
                        Some(v) => *v = *v + 1,
                    };
                    continue;
                }
                let old_peer = old_torrent
                    .peer_dic
                    .insert(String::from(ip_port), peer.clone());
                let old_peer = match old_peer {
                    None => {
                        continue;
                    }
                    Some(v) => v,
                };
                if Self::judge_banned_2(&old_peer, peer, torrent_size) {
                    ban_peers.push(String::from(ip_port));
                    match self.network_dic.get_mut(network.as_str()) {
                        None => {
                            self.network_dic.insert(network.clone(), 1);
                        }
                        Some(v) => *v = *v + 1,
                    };
                }
            }
        }

        if ban_peers.len() == 0 {
            return Ok(());
        };
        debug!("network {:#?}", self.network_dic);
        self.qb_client.ban_peers(ban_peers).await?;
        Ok(())
    }

    fn judge_banned_1(
        new: &Peer,
        torrent_size: u64,
        network: &str,
        network_dic: &HashMap<String, u64>,
    ) -> bool {
        // 客户端名称只允许：
        // ASCII 字符（Unicode 码点 0x20（空格） 到 0x7E（'~'））
        // 'µ'（0xB5），'μ'（0x03BC）
        /*for c in new.client.chars() {
            if c < ' ' || (c > '~' && c != 'µ' && c != 'μ') {
                info!("Banned - Weird Client: [{}]:{}", new.ip, new.port, new.client);
                return true;
            }
        }*/

        // 诡异客户端
        /*if new.client.chars().count() < 4 || new.client.chars().collect::<Vec<_>>()[2] == ' '
            || new.client.starts_with("Unknown") {
            info!("Banned - Weird Client: [{}]:{}", new.ip, new.port, new.client);
            return true;
        }*/

        // 吸血客户端
        if LEECH_CLIENTS.contains(&new.client.as_str()) {
            info!("Banned - Leech Client: [{}]:{}", new.ip, new.port);
            return true;
        }

        // 上古客户端
        if ANCIENT_CLIENTS.contains(&new.client.as_str()) {
            info!("Banned - Ancient Client: [{}]:{}", new.ip, new.port);
            return true;
        }

        // 通过网段禁用
        match network_dic.get(network) {
            None => {}
            Some(count) => {
                if *count >= 5 {
                    info!("Banned - Same network client: [{}]:{}", new.ip, new.port);
                    return true;
                }
            }
        }

        // 总上传 大于 报告进度 * 种子大小 + 10 MB
        if new.uploaded > (new.progress * torrent_size as f64) as u64 + 10 * 1024 * 1024 {
            info!("Banned - Too much upload: [{}]:{}", new.ip, new.port);
            return true;
        }

        false
    }

    fn judge_banned_2(old: &Peer, new: &Peer, torrent_size: u64) -> bool {
        // 进度倒退
        if new.progress < old.progress {
            info!("Banned - Progress is regressive: [{}]:{}", new.ip, new.port);
            return true;
        }

        // 进度增量小于上传增量
        let diff_uploaded = new.uploaded - old.uploaded;
        let diff_progress = new.progress - old.progress;
        if diff_progress < (diff_uploaded as f64 / torrent_size as f64) - F64_ERROR {
            info!(
                "Banned - Progress is not expected: [{}]:{}",
                new.ip, new.port
            );
            return true;
        }

        false
    }

    fn get_network(ip: &str) -> Result<String, GetNetworkError> {
        let addr = ip.parse::<Ipv4Addr>();
        match addr {
            Ok(addr) => {
                let network = IpNetwork::new_truncate(addr, 24)?;
                return Ok(network.to_string());
            }
            Err(_) => {}
        }
        let addr = ip.parse::<Ipv6Addr>()?;
        let network = IpNetwork::new_truncate(addr, 64)?;
        Ok(network.to_string())
    }
}
