use std::time::Duration;

use anyhow::{Context as _, Result, bail};
use reqwest::StatusCode;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};

use super::model::{Channel, Guild, Message, User};

const API_BASE: &str = "https://discord.com/api/v10";
const USER_AGENT: &str = "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36";
const MAX_ATTEMPTS: usize = 3;

#[derive(Clone)]
pub struct Api {
    http: reqwest::Client,
    token: String,
}

impl Api {
    pub fn new(token: impl Into<String>) -> Self {
        let http = reqwest::Client::builder()
            .user_agent(USER_AGENT)
            .build()
            .expect("failed to build http client");
        Self {
            http,
            token: token.into(),
        }
    }

    async fn send<T: DeserializeOwned>(
        &self,
        method: reqwest::Method,
        path: &str,
        body: Option<Value>,
    ) -> Result<T> {
        for attempt in 1..=MAX_ATTEMPTS {
            let mut request = self
                .http
                .request(method.clone(), format!("{API_BASE}{path}"))
                .header(reqwest::header::AUTHORIZATION, &self.token);
            if let Some(body) = body.as_ref() {
                request = request.json(body);
            }

            let response = request.send().await.context("request failed")?;
            let status = response.status();
            let text = response.text().await.unwrap_or_default();

            if status == StatusCode::TOO_MANY_REQUESTS {
                let retry_after = serde_json::from_str::<Value>(&text)
                    .ok()
                    .and_then(|value| value["retry_after"].as_f64())
                    .unwrap_or(1.0);
                if attempt == MAX_ATTEMPTS {
                    bail!("rate limited by discord ({retry_after:.1}s)");
                }
                let wait = Duration::from_secs_f64(retry_after.min(5.0));
                tokio::time::sleep(wait).await;
                continue;
            }

            if !status.is_success() {
                bail!("discord returned {status}: {}", truncate(&text, 300));
            }

            return serde_json::from_str(&text)
                .with_context(|| format!("failed to parse response: {}", truncate(&text, 300)));
        }
        unreachable!("retry loop always returns")
    }

    pub async fn current_user(&self) -> Result<User> {
        self.send(reqwest::Method::GET, "/users/@me", None).await
    }

    pub async fn guilds(&self) -> Result<Vec<Guild>> {
        self.send(reqwest::Method::GET, "/users/@me/guilds", None)
            .await
    }

    /// Direct message and group DM channels.
    pub async fn private_channels(&self) -> Result<Vec<Channel>> {
        self.send(reqwest::Method::GET, "/users/@me/channels", None)
            .await
    }

    pub async fn guild_channels(&self, guild_id: &str) -> Result<Vec<Channel>> {
        let mut channels: Vec<Channel> = self
            .send(
                reqwest::Method::GET,
                &format!("/guilds/{guild_id}/channels"),
                None,
            )
            .await?;
        channels.sort_by_key(|channel| channel.position.unwrap_or(i64::MAX));
        Ok(channels)
    }

    /// Returns messages in chronological order (oldest first).
    pub async fn messages(&self, channel_id: &str, limit: u32) -> Result<Vec<Message>> {
        let mut messages: Vec<Message> = self
            .send(
                reqwest::Method::GET,
                &format!("/channels/{channel_id}/messages?limit={limit}"),
                None,
            )
            .await?;
        messages.reverse();
        Ok(messages)
    }

    pub async fn send_message(&self, channel_id: &str, content: &str) -> Result<Message> {
        self.send(
            reqwest::Method::POST,
            &format!("/channels/{channel_id}/messages"),
            Some(json!({ "content": content, "tts": false })),
        )
        .await
    }
}

fn truncate(text: &str, max: usize) -> &str {
    match text.char_indices().nth(max) {
        Some((idx, _)) => &text[..idx],
        None => text,
    }
}
