use reqwest::{Client, Error};

pub struct QbClient {
    client: Client
}

impl QbClient {
    pub fn new() -> Self {
        QbClient{client: Client::new()}
    }

    pub async fn test(&self) -> Result<(),Error>
    {
        let resp = self.client.post("http://127.0.0.1:3004/api/fs/list")
            .body(r#"
        {
            "path": "/音乐",
            "password": "",
            "page": 1,
            "per_page": 0,
            "refresh": false
        }"#.to_owned())
            .header("Content-Type", "application/json")
            .send()
            .await?;
        let text = resp.text().await?;
        println!("{}", text);
        Ok(())
    }
}