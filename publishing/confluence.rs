use reqwest::blocking::Client;
use serde_json::Value;
use std::thread;
use std::time::Duration;

pub struct ConfluenceClient {
    client: Client,
    pub base_url: String,
    username: String,
    api_token: String,
}

impl ConfluenceClient {
    pub fn new(base_url: &str, username: &str, api_token: &str) -> Self {
        ConfluenceClient {
            client: Client::new(),
            base_url: base_url.trim_end_matches('/').to_string(),
            username: username.to_string(),
            api_token: api_token.to_string(),
        }
    }

    pub fn get_page_by_title(&self, space_key: &str, title: &str) -> Option<Value> {
        let url = format!(
            "{}/rest/api/content?spaceKey={}&title={}&expand=version",
            self.base_url,
            space_key,
            urlencoded(title)
        );

        let resp = self
            .client
            .get(&url)
            .basic_auth(&self.username, Some(&self.api_token))
            .send()
            .ok()?;

        let json: Value = resp.json().ok()?;
        let results = json.get("results")?.as_array()?;
        results.first().cloned()
    }

    pub fn get_page_by_id(&self, page_id: &str) -> Option<Value> {
        let url = format!(
            "{}/rest/api/content/{}?expand=version",
            self.base_url, page_id
        );

        let resp = self
            .client
            .get(&url)
            .basic_auth(&self.username, Some(&self.api_token))
            .send()
            .ok()?;

        resp.json().ok()
    }

    pub fn get_child_pages(&self, page_id: &str) -> Vec<Value> {
        let url = format!(
            "{}/rest/api/content/{}/child/page?limit=200",
            self.base_url, page_id
        );

        let resp = self
            .client
            .get(&url)
            .basic_auth(&self.username, Some(&self.api_token))
            .send();

        match resp {
            Ok(r) => {
                if let Ok(json) = r.json::<Value>() {
                    json.get("results")
                        .and_then(|v| v.as_array())
                        .cloned()
                        .unwrap_or_default()
                } else {
                    Vec::new()
                }
            }
            Err(_) => Vec::new(),
        }
    }

    pub fn create_page(
        &self,
        space_key: &str,
        title: &str,
        body: &str,
        parent_id: &str,
    ) -> Option<Value> {
        let url = format!("{}/rest/api/content", self.base_url);

        let payload = serde_json::json!({
            "type": "page",
            "title": title,
            "space": { "key": space_key },
            "body": {
                "storage": {
                    "value": body,
                    "representation": "storage"
                }
            },
            "ancestors": [{ "id": parent_id }]
        });

        let resp = self
            .client
            .post(&url)
            .basic_auth(&self.username, Some(&self.api_token))
            .json(&payload)
            .send()
            .ok()?;

        resp.json().ok()
    }

    pub fn update_page(
        &self,
        space_key: &str,
        page_id: &str,
        title: &str,
        body: &str,
    ) -> Result<(), String> {
        let page_info = self
            .get_page_by_id(page_id)
            .ok_or("Failed to get page info")?;
        let version_number = page_info["version"]["number"]
            .as_i64()
            .unwrap_or(0)
            + 1;

        let payload = serde_json::json!({
            "id": page_id,
            "type": "page",
            "title": title,
            "space": { "key": space_key },
            "body": {
                "storage": {
                    "value": body,
                    "representation": "storage"
                }
            },
            "version": { "number": version_number },
            "metadata": {
                "properties": {
                    "content-appearance-draft": { "value": "full-width" },
                    "content-appearance-published": { "value": "full-width" }
                }
            }
        });

        let url = format!("{}/rest/api/content/{}", self.base_url, page_id);
        let resp = self
            .client
            .put(&url)
            .basic_auth(&self.username, Some(&self.api_token))
            .json(&payload)
            .send()
            .map_err(|e| format!("Failed to update page: {}", e))?;

        if resp.status().is_success() {
            Ok(())
        } else {
            Err(format!(
                "Failed to update page: HTTP {}",
                resp.status()
            ))
        }
    }

    pub fn attach_file(
        &self,
        page_id: &str,
        filepath: &str,
        attachment_name: &str,
    ) -> Result<(), String> {
        let url = format!(
            "{}/rest/api/content/{}/child/attachment",
            self.base_url, page_id
        );

        let file_bytes = std::fs::read(filepath)
            .map_err(|e| format!("Failed to read file {}: {}", filepath, e))?;

        let part = reqwest::blocking::multipart::Part::bytes(file_bytes)
            .file_name(attachment_name.to_string())
            .mime_str("application/octet-stream")
            .map_err(|e| format!("Failed to create multipart: {}", e))?;

        let form = reqwest::blocking::multipart::Form::new().part("file", part);

        let resp = self
            .client
            .put(&url)
            .basic_auth(&self.username, Some(&self.api_token))
            .header("X-Atlassian-Token", "nocheck")
            .multipart(form)
            .send()
            .map_err(|e| format!("Failed to attach file: {}", e))?;

        if resp.status().is_success() {
            Ok(())
        } else {
            Err(format!(
                "Failed to attach file: HTTP {}",
                resp.status()
            ))
        }
    }

    pub fn get_attachment_content(
        &self,
        page_id: &str,
        filename: &str,
    ) -> Option<String> {
        let url = format!(
            "{}/rest/api/content/{}/child/attachment?filename={}",
            self.base_url, page_id, urlencoded(filename)
        );

        let resp = self
            .client
            .get(&url)
            .basic_auth(&self.username, Some(&self.api_token))
            .send()
            .ok()?;

        let json: Value = resp.json().ok()?;
        let size = json.get("size")?.as_i64()?;
        if size == 0 {
            return None;
        }

        let download_link = json
            .get("results")?
            .as_array()?
            .first()?
            .get("_links")?
            .get("download")?
            .as_str()?;

        let download_url = format!("{}{}", self.base_url, download_link);
        let content_resp = self
            .client
            .get(&download_url)
            .basic_auth(&self.username, Some(&self.api_token))
            .send()
            .ok()?;

        let text = content_resp.text().ok()?;
        let trimmed = text.trim().to_string();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        }
    }

    pub fn wait(&self, seconds: u64) {
        thread::sleep(Duration::from_secs(seconds));
    }
}

fn urlencoded(s: &str) -> String {
    s.replace(' ', "%20")
        .replace(':', "%3A")
        .replace('/', "%2F")
}
