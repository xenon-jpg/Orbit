use crate::{
    error::{Error, Result},
    model::*,
    target,
    vault::Vault,
};
use reqwest::{
    header::{HeaderMap, HeaderValue, COOKIE, SET_COOKIE},
    Method, StatusCode,
};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::Mutex;

pub struct Roblox {
    http: reqwest::Client,
    vault: Arc<Vault>,
    account_locks: Mutex<HashMap<String, Arc<Mutex<()>>>>,
    csrf: Mutex<HashMap<String, String>>,
    throttle: Mutex<HashMap<String, Instant>>,
}
fn checked_url(url: &str) -> Result<reqwest::Url> {
    let parsed = reqwest::Url::parse(url).map_err(|_| Error::Invalid("Invalid API URL."))?;
    let host = parsed.host_str().unwrap_or_default();
    if parsed.scheme() != "https"
        || !(host == "roblox.com" || host.ends_with(".roblox.com") || host.ends_with(".rbxcdn.com"))
        || parsed.port().is_some()
        || !parsed.username().is_empty()
        || parsed.password().is_some()
    {
        return Err(Error::Invalid("Blocked a non-Roblox request."));
    }
    Ok(parsed)
}
fn cookie_header(cookie: &Secret) -> Result<HeaderValue> {
    let cookie = cookie.expose();
    if cookie.is_empty() || cookie.len() > 16_384 || cookie.bytes().any(|b| b < 32 || b == 127) {
        return Err(Error::SessionExpired);
    }
    HeaderValue::from_str(&format!(".ROBLOSECURITY={cookie}")).map_err(|_| Error::SessionExpired)
}
fn classify(status: StatusCode, body: &[u8]) -> Error {
    if status == StatusCode::UNAUTHORIZED {
        let v: Value = serde_json::from_slice(body).unwrap_or_default();
        let errors = v.get("errors").and_then(Value::as_array);
        if errors
            .and_then(|e| e.first())
            .and_then(|e| e.get("subcode"))
            .and_then(Value::as_u64)
            == Some(2)
        {
            Error::BrowserVerification
        } else {
            Error::SessionExpired
        }
    } else if status == StatusCode::TOO_MANY_REQUESTS {
        Error::RateLimited
    } else if status.is_server_error() || status == StatusCode::REQUEST_TIMEOUT {
        Error::Network
    } else {
        Error::HttpStatus(status.as_u16())
    }
}

impl Roblox {
    pub fn new(vault: Arc<Vault>) -> Result<Self> {
        // Redirects are disabled: a session cookie must never follow a cross-host redirect.
        let http = reqwest::Client::builder()
            .use_rustls_tls()
            .redirect(reqwest::redirect::Policy::none())
            .user_agent(concat!("AltDeck/", env!("CARGO_PKG_VERSION")))
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(25))
            .build()?;
        Ok(Self {
            http,
            vault,
            account_locks: Mutex::new(HashMap::new()),
            csrf: Mutex::new(HashMap::new()),
            throttle: Mutex::new(HashMap::new()),
        })
    }
    async fn pace(&self, url: &reqwest::Url) {
        let host = url.host_str().unwrap_or("roblox").to_owned();
        let wait = {
            let throttle = self.throttle.lock().await;
            throttle
                .get(&host)
                .map(|last| Duration::from_millis(350).saturating_sub(last.elapsed()))
        };
        if let Some(wait) = wait {
            tokio::time::sleep(wait).await;
        }
        self.throttle.lock().await.insert(host, Instant::now());
    }
    async fn raw_value(
        &self,
        method: Method,
        url: &str,
        cookie: Option<&Secret>,
        body: Option<Value>,
        account: Option<&str>,
    ) -> Result<(Value, Option<Secret>)> {
        let url = checked_url(url)?;
        let mut effective_cookie = cookie.cloned();
        let mut csrf = match account {
            Some(id) => self.csrf.lock().await.get(id).cloned(),
            None => None,
        };
        for attempt in 0..3 {
            self.pace(&url).await;
            let mut headers = HeaderMap::new();
            headers.insert(
                "referer",
                HeaderValue::from_static("https://www.roblox.com/"),
            );
            headers.insert("origin", HeaderValue::from_static("https://www.roblox.com"));
            if let Some(cookie) = &effective_cookie {
                headers.insert(COOKIE, cookie_header(cookie)?);
            }
            if let Some(token) = &csrf {
                headers.insert(
                    "x-csrf-token",
                    HeaderValue::from_str(token).map_err(|_| Error::Internal)?,
                );
            }
            let mut request = self
                .http
                .request(method.clone(), url.clone())
                .headers(headers);
            if let Some(body) = &body {
                request = request.json(body);
            }
            let response = request.send().await?;
            let status = response.status();
            let mut rotated_cookie = None;
            // Persist rotation before acknowledging any successful API call.
            if let Some(id) = account {
                for value in response.headers().get_all(SET_COOKIE) {
                    if let Ok(value) = value.to_str() {
                        if let Some(rest) = value.strip_prefix(".ROBLOSECURITY=") {
                            let rotated = rest.split(';').next().unwrap_or_default();
                            if !rotated.is_empty()
                                && rotated.len() <= 16_384
                                && !rotated.bytes().any(|b| b < 32 || b == 127)
                            {
                                let secret = Secret::new(rotated.into());
                                self.vault.replace_cookie(id, secret.clone()).await?;
                                effective_cookie = Some(secret.clone());
                                rotated_cookie = Some(secret);
                            }
                        }
                    }
                }
            }
            let next_csrf = response
                .headers()
                .get("x-csrf-token")
                .and_then(|v| v.to_str().ok())
                .map(str::to_owned);
            let challenge = response.headers().contains_key("rblx-challenge-id");
            let retry_after = response
                .headers()
                .get("retry-after")
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.parse::<u64>().ok());
            let mut length = 0usize;
            let mut bytes = Vec::new();
            let mut response = response;
            while let Some(chunk) = response.chunk().await? {
                length += chunk.len();
                if length > 4 * 1024 * 1024 {
                    return Err(Error::HttpStatus(502));
                }
                bytes.extend_from_slice(&chunk);
            }
            if challenge {
                return Err(Error::BrowserVerification);
            }
            if status == StatusCode::FORBIDDEN && attempt == 0 {
                if let Some(token) = next_csrf {
                    if token.len() < 512 {
                        if let Some(id) = account {
                            self.csrf.lock().await.insert(id.into(), token.clone());
                        }
                        csrf = Some(token);
                        continue;
                    }
                }
            }
            if status == StatusCode::TOO_MANY_REQUESTS && attempt < 2 {
                tokio::time::sleep(Duration::from_secs(
                    retry_after.unwrap_or(2u64.pow(attempt as u32)).min(30),
                ))
                .await;
                continue;
            }
            if status.is_server_error() && attempt < 2 {
                tokio::time::sleep(Duration::from_secs(2u64.pow(attempt as u32))).await;
                continue;
            }
            if !status.is_success() {
                return Err(classify(status, &bytes));
            }
            let value = if bytes.is_empty() {
                Value::Null
            } else {
                serde_json::from_slice(&bytes).map_err(|_| Error::HttpStatus(502))?
            };
            return Ok((value, rotated_cookie));
        }
        Err(Error::Network)
    }
    async fn raw(
        &self,
        method: Method,
        url: &str,
        cookie: Option<&Secret>,
        body: Option<Value>,
        account: Option<&str>,
    ) -> Result<Value> {
        self.raw_value(method, url, cookie, body, account)
            .await
            .map(|(value, _)| value)
    }
    pub async fn public_get(&self, url: &str) -> Result<Value> {
        self.raw(Method::GET, url, None, None, None).await
    }
    async fn account_lock(&self, id: &str) -> Arc<Mutex<()>> {
        self.account_locks
            .lock()
            .await
            .entry(id.into())
            .or_default()
            .clone()
    }
    pub async fn get(&self, id: &str, url: &str) -> Result<Value> {
        let lock = self.account_lock(id).await;
        let _guard = lock.lock().await;
        let cookie = self.vault.cookie(id).await?;
        self.raw(Method::GET, url, Some(&cookie), None, Some(id))
            .await
    }
    pub async fn post(&self, id: &str, url: &str, body: Value) -> Result<Value> {
        let lock = self.account_lock(id).await;
        let _guard = lock.lock().await;
        let cookie = self.vault.cookie(id).await?;
        self.raw(Method::POST, url, Some(&cookie), Some(body), Some(id))
            .await
    }
    pub async fn validate(&self, cookie: &Secret) -> Result<(u64, String, String, Secret)> {
        let (value, rotated) = self
            .raw_value(
                Method::GET,
                "https://users.roblox.com/v1/users/authenticated",
                Some(cookie),
                None,
                None,
            )
            .await?;
        let id = value["id"].as_u64().ok_or(Error::HttpStatus(502))?;
        let name = value["name"]
            .as_str()
            .ok_or(Error::HttpStatus(502))?
            .to_owned();
        let display = value["displayName"].as_str().unwrap_or(&name).to_owned();
        Ok((id, name, display, rotated.unwrap_or_else(|| cookie.clone())))
    }
    pub async fn ticket(&self, id: &str) -> Result<zeroize::Zeroizing<String>> {
        let lock = self.account_lock(id).await;
        let _guard = lock.lock().await;
        let cookie = self.vault.cookie(id).await?;
        let assertion = self
            .raw(
                Method::GET,
                "https://auth.roblox.com/v1/client-assertion/",
                Some(&cookie),
                None,
                Some(id),
            )
            .await
            .unwrap_or(json!({}));
        let url = checked_url("https://auth.roblox.com/v1/authentication-ticket/")?;
        let mut csrf = self.csrf.lock().await.get(id).cloned();
        for attempt in 0..2 {
            self.pace(&url).await;
            let mut headers = HeaderMap::new();
            headers.insert(COOKIE, cookie_header(&self.vault.cookie(id).await?)?);
            headers.insert(
                "referer",
                HeaderValue::from_static("https://www.roblox.com/"),
            );
            headers.insert("origin", HeaderValue::from_static("https://www.roblox.com"));
            if let Some(token) = &csrf {
                headers.insert(
                    "x-csrf-token",
                    HeaderValue::from_str(token).map_err(|_| Error::Internal)?,
                );
            }
            let response = self
                .http
                .post(url.clone())
                .headers(headers)
                .json(&assertion)
                .send()
                .await?;
            let status = response.status();
            for value in response
                .headers()
                .get_all("set-cookie")
                .iter()
                .filter_map(|v| v.to_str().ok())
                .filter_map(|v| v.strip_prefix(".ROBLOSECURITY="))
            {
                let value = value.split(';').next().unwrap_or_default();
                if !value.is_empty()
                    && value.len() <= 16_384
                    && !value.bytes().any(|b| b < 32 || b == 127)
                {
                    self.vault
                        .replace_cookie(id, Secret::new(value.into()))
                        .await?;
                }
            }
            if status == StatusCode::FORBIDDEN && attempt == 0 {
                if let Some(token) = response
                    .headers()
                    .get("x-csrf-token")
                    .and_then(|v| v.to_str().ok())
                {
                    csrf = Some(token.into());
                    self.csrf.lock().await.insert(id.into(), token.into());
                    continue;
                }
            }
            if response.headers().contains_key("rblx-challenge-id") {
                return Err(Error::BrowserVerification);
            }
            if !status.is_success() {
                let mut response = response;
                let mut bytes = Vec::new();
                while let Some(chunk) = response.chunk().await? {
                    if bytes.len() + chunk.len() > 4 * 1024 * 1024 {
                        return Err(Error::HttpStatus(502));
                    }
                    bytes.extend_from_slice(&chunk);
                }
                return Err(classify(status, &bytes));
            }
            let ticket = response
                .headers()
                .get("rbx-authentication-ticket")
                .and_then(|v| v.to_str().ok())
                .ok_or(Error::HttpStatus(502))?;
            return Ok(zeroize::Zeroizing::new(ticket.to_owned()));
        }
        Err(Error::SessionExpired)
    }
    pub async fn resolve_username(&self, username: &str) -> Result<u64> {
        let value = self
            .raw(
                Method::POST,
                "https://users.roblox.com/v1/usernames/users",
                None,
                Some(json!({"usernames": [username], "excludeBannedUsers": false})),
                None,
            )
            .await?;
        value["data"][0]["id"].as_u64().ok_or(Error::NotFound)
    }
    pub async fn resolve_target(&self, input: &str) -> Result<Target> {
        if let Some(username) = input.trim().strip_prefix('@') {
            target::parse(input)?;
            return Ok(Target::Follow {
                user_id: self.resolve_username(username).await?,
            });
        }
        target::parse(input)
    }
    pub async fn game(&self, place_id: u64) -> Result<GameInfo> {
        let value = self
            .public_get(&format!(
                "https://apis.roblox.com/universes/v1/places/{place_id}/universe"
            ))
            .await?;
        let universe_id = value["universeId"].as_u64().ok_or(Error::NotFound)?;
        let details = self
            .public_get(&format!(
                "https://games.roblox.com/v1/games?universeIds={universe_id}"
            ))
            .await?;
        let game = &details["data"][0];
        let icon = self.public_get(&format!("https://thumbnails.roblox.com/v1/places/gameicons?placeIds={place_id}&size=150x150&format=Png&isCircular=false")).await.unwrap_or_default();
        Ok(GameInfo {
            place_id,
            universe_id,
            name: game["name"].as_str().unwrap_or("Roblox experience").into(),
            description: game["description"]
                .as_str()
                .unwrap_or("")
                .chars()
                .take(1000)
                .collect(),
            icon: safe_image(icon["data"][0]["imageUrl"].as_str().unwrap_or_default()),
            playing: game["playing"].as_u64().unwrap_or(0),
            visits: game["visits"].as_u64().unwrap_or(0),
            max_players: game["maxPlayers"].as_u64().unwrap_or(0),
        })
    }
    pub async fn servers(
        &self,
        account: Option<&str>,
        place_id: u64,
        private: bool,
        cursor: &str,
        sort: &str,
    ) -> Result<ServerPage> {
        if cursor.len() > 4096 || cursor.chars().any(char::is_control) {
            return Err(Error::Invalid("Invalid server cursor."));
        }
        let kind = if private {
            "private-servers"
        } else {
            "servers/Public"
        };
        let order = if sort == "asc" { "Asc" } else { "Desc" };
        let mut url = format!("https://games.roblox.com/v1/games/{place_id}/{kind}?limit=25&sortOrder={order}&excludeFullGames=true");
        if !cursor.is_empty() {
            url.push_str("&cursor=");
            url.push_str(
                &url::form_urlencoded::byte_serialize(cursor.as_bytes()).collect::<String>(),
            );
        }
        let value = match account {
            Some(id) => self.get(id, &url).await?,
            None => self.public_get(&url).await?,
        };
        let mut servers = Vec::new();
        for server in value["data"]
            .as_array()
            .ok_or(Error::HttpStatus(502))?
            .iter()
            .take(100)
        {
            let id = server["id"].as_str().unwrap_or_default();
            if !private && !target::valid_job(id) {
                continue;
            }
            let code = server["accessCode"].as_str().unwrap_or_default();
            if private && !target::valid_code(code) {
                continue;
            }
            servers.push(ServerInfo {
                id: id.chars().take(128).collect(),
                playing: server["playing"].as_u64().unwrap_or(0),
                max_players: server["maxPlayers"].as_u64().unwrap_or(0),
                ping: server["ping"].as_u64(),
                fps: server["fps"].as_f64(),
                private,
                name: server["name"]
                    .as_str()
                    .unwrap_or("Private server")
                    .chars()
                    .take(100)
                    .collect(),
                link_code: code.into(),
            });
        }
        Ok(ServerPage {
            servers,
            next_cursor: value["nextPageCursor"]
                .as_str()
                .map(|s| s.chars().take(4096).collect()),
        })
    }
    pub async fn refresh(&self, id: &str) -> Result<Account> {
        let lock = self.account_lock(id).await;
        let _guard = lock.lock().await;
        let stored_id = self
            .vault
            .read(|d| Ok(d.accounts.iter().find(|a| a.id == id).map(|a| a.user_id)))
            .await?;
        let cookie = self.vault.cookie(id).await?;
        let (user_id, username, display_name, cookie) = self.validate(&cookie).await?;
        if stored_id != Some(user_id) {
            return Err(Error::Invalid(
                "This session belongs to a different Roblox account.",
            ));
        }
        self.vault.replace_cookie(id, cookie).await?;
        drop(_guard);
        let robux = self
            .get(id, "https://economy.roblox.com/v1/user/currency")
            .await
            .ok()
            .and_then(|v| v["robux"].as_u64());
        let friends = self
            .public_get(&format!(
                "https://friends.roblox.com/v1/users/{user_id}/friends/count"
            ))
            .await
            .ok()
            .and_then(|v| v["count"].as_u64());
        let followers = self
            .public_get(&format!(
                "https://friends.roblox.com/v1/users/{user_id}/followers/count"
            ))
            .await
            .ok()
            .and_then(|v| v["count"].as_u64());
        let premium = self
            .public_get(&format!(
                "https://premiumfeatures.roblox.com/v1/users/{user_id}/validate-membership"
            ))
            .await
            .ok()
            .and_then(|v| v.as_bool());
        let avatars = self.public_get(&format!("https://thumbnails.roblox.com/v1/users/avatar-headshot?userIds={user_id}&size=150x150&format=Png&isCircular=false")).await.unwrap_or_default();
        let avatar_url = safe_image(avatars["data"][0]["imageUrl"].as_str().unwrap_or_default());
        let presence = self
            .post(
                id,
                "https://presence.roblox.com/v1/presence/users",
                json!({"userIds": [user_id]}),
            )
            .await
            .ok()
            .and_then(|v| v["userPresences"][0]["userPresenceType"].as_u64())
            .unwrap_or(0) as u8;
        let groups = self
            .public_get(&format!(
                "https://groups.roblox.com/v2/users/{user_id}/groups/roles"
            ))
            .await
            .ok()
            .and_then(|v| {
                v["data"].as_array().map(|a| {
                    a.iter()
                        .filter_map(|g| g["group"]["name"].as_str().map(str::to_owned))
                        .take(30)
                        .collect()
                })
            })
            .unwrap_or_default();
        self.vault
            .with(|data| {
                let account = data
                    .accounts
                    .iter_mut()
                    .find(|a| a.id == id)
                    .ok_or(Error::NotFound)?;
                account.username = username;
                account.display_name = display_name;
                account.robux = robux;
                account.friends = friends;
                account.followers = followers;
                account.premium = premium;
                if !avatar_url.is_empty() {
                    account.avatar_url = avatar_url;
                }
                account.groups = groups;
                account.presence = presence;
                account.session_status = "valid".into();
                account.checked_at = Some(chrono::Utc::now().timestamp());
                Ok(account.clone())
            })
            .await
    }
    pub async fn update_available(&self) -> Result<bool> {
        let value = self
            .public_get("https://clientsettingscdn.roblox.com/v2/client-version/WindowsPlayer")
            .await?;
        let latest = value["clientVersionUpload"]
            .as_str()
            .ok_or(Error::HttpStatus(502))?;
        #[cfg(windows)]
        {
            let path = crate::platform::win::roblox_path()?;
            let installed = path
                .parent()
                .and_then(|p| p.file_name())
                .and_then(|n| n.to_str())
                .unwrap_or_default();
            Ok(!latest.is_empty() && latest != installed)
        }
        #[cfg(not(windows))]
        {
            Ok(!latest.is_empty())
        }
    }
    pub async fn webhook_test(&self) -> Result<()> {
        let url = self
            .vault
            .read(|d| {
                Ok(d.settings
                    .webhook_enabled
                    .then(|| d.webhook.expose().to_owned()))
            })
            .await?
            .ok_or(Error::Invalid("Enable Discord alerts first."))?;
        validate_webhook(&url)?;
        let response = self.http.post(&url).json(&json!({"content": "AltDeck connected. Alerts are enabled.", "allowed_mentions": {"parse": []}})).send().await?;
        if !response.status().is_success() {
            return Err(Error::HttpStatus(response.status().as_u16()));
        }
        Ok(())
    }
    pub async fn notify(&self, message: &str) {
        let config = self
            .vault
            .background_data()
            .await
            .map(|d| (d.settings.webhook_enabled, d.webhook.expose().to_owned()));
        if let Ok((true, url)) = config {
            if validate_webhook(&url).is_ok() {
                let _ = self.http.post(url).timeout(Duration::from_secs(8))
                    .json(&json!({"content": message.chars().take(500).collect::<String>(), "allowed_mentions": {"parse": []}})).send().await;
            }
        }
    }
}
pub fn safe_image(url: &str) -> String {
    match reqwest::Url::parse(url) {
        Ok(url)
            if url.scheme() == "https"
                && url.host_str().is_some_and(|h| h.ends_with(".rbxcdn.com"))
                && url.as_str().len() < 2048 =>
        {
            url.into()
        }
        _ => String::new(),
    }
}
pub fn validate_webhook(url: &str) -> Result<()> {
    let parsed = reqwest::Url::parse(url).map_err(|_| Error::Invalid("Invalid webhook URL."))?;
    if parsed.scheme() != "https"
        || !matches!(parsed.host_str(), Some("discord.com" | "discordapp.com"))
        || !parsed.path().starts_with("/api/webhooks/")
        || !parsed.username().is_empty()
        || parsed.password().is_some()
    {
        return Err(Error::Invalid(
            "Only official Discord HTTPS webhooks are accepted.",
        ));
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn allows_only_roblox_https() {
        assert!(checked_url("https://auth.roblox.com/v1/client-assertion/").is_ok());
        for url in [
            "http://auth.roblox.com",
            "https://roblox.com.evil.example",
            "https://evil.example/.roblox.com",
            "https://user@auth.roblox.com",
        ] {
            assert!(checked_url(url).is_err());
        }
    }
    #[test]
    fn redacts_hba_and_expired_sessions() {
        assert!(matches!(
            classify(
                StatusCode::UNAUTHORIZED,
                br#"{"errors":[{"code":9002,"subcode":2}]}"#
            ),
            Error::BrowserVerification
        ));
        assert!(matches!(
            classify(StatusCode::UNAUTHORIZED, b"{}"),
            Error::SessionExpired
        ));
    }
    #[test]
    fn webhook_host_is_exact() {
        assert!(validate_webhook("https://discord.com/api/webhooks/1/abc").is_ok());
        assert!(validate_webhook("https://discord.com.evil.example/api/webhooks/1/abc").is_err());
    }
}
