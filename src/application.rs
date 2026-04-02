use chrono::{DateTime, Duration, Local};
use ip_network::IpNetwork;
use qb_sdk::Peer;
use qb_sdk::QbClient;
use std::collections::HashMap;
use std::net::{Ipv4Addr, Ipv6Addr};
use thiserror::Error;
use tokio::time::sleep;
use tracing::{debug, info};

const F64_ERROR: f64 = 0.0001;

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

#[derive(Clone, Debug)]
pub struct Torrent {
    pub size: u64,
    /// ip and port, peer info
    pub peers: HashMap<String, Peer>,
}

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
    new_torrent_state: HashMap<String, Torrent>,
    old_torrent_state: HashMap<String, Torrent>,
    /// network, banned ip count
    network_dic: HashMap<String, u64>,
}

impl Application {
    pub fn new(qb_client: QbClient, interval: u64) -> Self {
        Application {
            qb_client,
            interval,
            last_reset_time: Local::now() - Duration::days(2),
            new_torrent_state: HashMap::new(),
            old_torrent_state: HashMap::new(),
            network_dic: HashMap::new(),
        }
    }

    pub async fn wait(&self) {
        sleep(std::time::Duration::from_secs(self.interval)).await;
    }

    pub async fn ensure_api_version(&self) -> Result<(), Error> {
        let api_version = self.qb_client.get_api_version().await?;
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
        if Local::now() - self.last_reset_time < Duration::days(1) {
            return Ok(());
        };
        self.qb_client.reset_banned_IPs().await?;
        self.old_torrent_state.clear();
        self.new_torrent_state.clear();
        self.network_dic.clear();
        self.last_reset_time = Local::now();
        Ok(())
    }

    pub async fn record_and_ban_peers(&mut self) -> Result<(), Error> {
        self.update_torrent_state(self.get_torrent_state().await?);

        let mut ban_peers: Vec<String> = vec![];
        for (hash, torrent) in self.new_torrent_state.iter() {
            for (ip_port, peer) in torrent.peers.iter() {
                let network = Self::get_network(peer.ip.as_str())?;
                if !Self::judge_banned(
                    torrent.size,
                    self.old_torrent_state
                        .get(hash.as_str())
                        .and_then(|t| t.peers.get(ip_port.as_str())),
                    peer,
                    network.as_str(),
                    &self.network_dic,
                ) {
                    continue;
                }
                debug!(
                    "Banning peer [{}]:{} in torrent {}",
                    peer.ip, peer.port, hash
                );
                ban_peers.push(String::from(ip_port));
                match self.network_dic.get_mut(network.as_str()) {
                    None => {
                        self.network_dic.insert(network.clone(), 1);
                    }
                    Some(v) => *v = *v + 1,
                };
            }
        }
        if ban_peers.len() == 0 {
            return Ok(());
        };
        debug!("network {:#?}", self.network_dic);
        self.qb_client.ban_peers(ban_peers).await?;
        Ok(())
    }

    /// true means should be banned, false means should not be banned. This function is the core of the application.
    fn judge_banned(
        torrent_size: u64,
        old: Option<&Peer>,
        new: &Peer,
        network: &str,
        network_dic: &HashMap<String, u64>,
    ) -> bool {
        // Client is only allowed:
        // ASCII characters (Unicode code points 0x20 (space) to 0x7E ('~'))
        // 'µ' (0xB5), 'μ' (0x03BC)
        /*for c in new.client.chars() {
            if c < ' ' || (c > '~' && c != 'µ' && c != 'μ') {
                info!("Weird Client: [{}]:{}", new.ip, new.port, new.client);
                return true;
            }
        }*/

        // Weird client, such as client name is too short, or client name has a space in the middle, or client name starts with "Unknown", etc.
        /*if new.client.chars().count() < 4 || new.client.chars().collect::<Vec<_>>()[2] == ' '
            || new.client.starts_with("Unknown") {
            info!("Weird Client: [{}]:{}", new.ip, new.port, new.client);
            return true;
        }*/

        // Leech client
        if LEECH_CLIENTS.contains(&new.client.as_str()) {
            info!("Leech Client: [{}]:{}", new.ip, new.port);
            return true;
        }

        // Ancient client
        if ANCIENT_CLIENTS.contains(&new.client.as_str()) {
            info!("Ancient Client: [{}]:{}", new.ip, new.port);
            return true;
        }

        // Same network client count exceeds 5
        match network_dic.get(network) {
            None => {}
            Some(count) => {
                if *count >= 5 {
                    info!("Same network client: [{}]:{}", new.ip, new.port);
                    return true;
                }
            }
        }

        // Total upload exceeds reported progress * torrent size + 10 MB
        if new.uploaded > (new.progress * torrent_size as f64) as u64 + 10 * 1024 * 1024 {
            info!("Too much upload: [{}]:{}", new.ip, new.port);
            return true;
        }

        let old = match old {
            Some(old) => old,
            None => return false,
        };

        // Progress is regressive
        if new.progress < old.progress {
            info!("Progress is regressive: [{}]:{}", new.ip, new.port);
            return true;
        }

        // Progress increment is less than upload increment
        let diff_uploaded = new.uploaded - old.uploaded;
        let diff_progress = new.progress - old.progress;
        if diff_progress < (diff_uploaded as f64 / torrent_size as f64) - F64_ERROR {
            info!(
                "Progress is not expected: [{}]:{}",
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

    /// Get torrent and peer info from qbittorrent.
    async fn get_torrent_state(&self) -> Result<HashMap<String, Torrent>, Error> {
        let torrent_array = self.qb_client.get_torrents().await?;

        let mut torrent_state: HashMap<String, Torrent> = torrent_array
            .into_iter()
            .map(|t| {
                (
                    t.hash.clone(),
                    Torrent {
                        size: t.size,
                        peers: HashMap::new(),
                    },
                )
            })
            .collect();
        for (hash, torrent) in torrent_state.iter_mut() {
            let hash_peers = self.qb_client.get_peers(hash.as_str()).await?;
            torrent.peers = hash_peers;
        }
        Ok(torrent_state)
    }

    /// Update torrent state. This function should be atomicity, however, since we only have one thread, it's ok.
    fn update_torrent_state(&mut self, new_torrent_state: HashMap<String, Torrent>) {
        self.old_torrent_state = std::mem::replace(&mut self.new_torrent_state, new_torrent_state);
    }
}
