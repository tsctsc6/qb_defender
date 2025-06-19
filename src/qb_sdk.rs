use crate::command::Cli;
use crate::log;
use reqwest::{Client, Error, RequestBuilder};
use std::time::Duration;
use tokio::time::sleep;

pub struct QbClient {
    client: Client,
    config: Cli
}

impl QbClient {
    pub fn new(cli: Cli) -> Self {
        QbClient{client: Client::new(), config: cli}
    }

    fn get_host(&self) -> String {
        format!("http://127.0.0.1:{}", self.config.port)
    }

    fn set_preferences(&self) -> RequestBuilder {
        self.client.post(self.get_host() + "/api/v2/app/setPreferences")
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
                    sleep(Duration::from_secs(self.config.interval)).await;
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
        let result = self.set_preferences()
            .form(&[("json", r#""{"banned_IPs":""}""#)])
            .send()
            .await;
        let success_result = match result {
            Ok(resp) => resp,
            Err(e) => {
                return Err(format!("Can't reset QBittorrent IP:\n{:#?}", e));
            }
        };
        if !success_result.status().is_success() {
            return Err(format!("Can't reset QBittorrent IPs:\n{:#?}", success_result));
        }
        Ok(())
    }
}