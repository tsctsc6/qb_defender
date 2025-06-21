use reqwest::RequestBuilder;
use crate::QbClient;

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
}