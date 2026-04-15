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
        let existing_id = self.get_attachment_id(page_id, attachment_name);

        let file_bytes = std::fs::read(filepath)
            .map_err(|e| format!("Failed to read file {}: {}", filepath, e))?;

        let mime = mime_for(attachment_name);
        let part = reqwest::blocking::multipart::Part::bytes(file_bytes)
            .file_name(attachment_name.to_string())
            .mime_str(mime)
            .map_err(|e| format!("Failed to create multipart: {}", e))?;

        let form = reqwest::blocking::multipart::Form::new()
            .text("comment", format!("Uploaded {}.", attachment_name))
            .text("minorEdit", "true")
            .part("file", part);

        let url = match &existing_id {
            Some(id) => format!(
                "{}/rest/api/content/{}/child/attachment/{}/data",
                self.base_url, page_id, id
            ),
            None => format!(
                "{}/rest/api/content/{}/child/attachment",
                self.base_url, page_id
            ),
        };

        let resp = self
            .client
            .post(&url)
            .basic_auth(&self.username, Some(&self.api_token))
            .header("X-Atlassian-Token", "no-check")
            .header("Accept", "application/json")
            .multipart(form)
            .send()
            .map_err(|e| format!("Failed to attach file: {}", e))?;

        let status = resp.status();
        let body = resp.text().unwrap_or_default();
        if status.is_success() {
            println!(
                "[DEBUG] Attached {} ({}) -> HTTP {}",
                attachment_name, mime, status
            );
            Ok(())
        } else {
            Err(format!(
                "Failed to attach file {}: HTTP {} — {}",
                attachment_name, status, body
            ))
        }
    }

    fn get_attachment_id(&self, page_id: &str, filename: &str) -> Option<String> {
        let url = format!(
            "{}/rest/api/content/{}/child/attachment?filename={}",
            self.base_url,
            page_id,
            urlencoded(filename)
        );

        let resp = self
            .client
            .get(&url)
            .basic_auth(&self.username, Some(&self.api_token))
            .send()
            .ok()?;

        let json: Value = resp.json().ok()?;
        json.get("results")?
            .as_array()?
            .first()?
            .get("id")?
            .as_str()
            .map(String::from)
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

fn mime_for(filename: &str) -> &'static str {
    let ext = std::path::Path::new(filename)
        .extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_ascii_lowercase());
    match ext.as_deref() {
        Some("png") => "image/png",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("svg") => "image/svg+xml",
        Some("webp") => "image/webp",
        Some("html") | Some("htm") => "text/html",
        Some("txt") => "text/plain",
        Some("json") => "application/json",
        Some("pdf") => "application/pdf",
        _ => "application/octet-stream",
    }
}
