use std::time::Duration;

use anyhow::{Context as _, Result};
use async_channel::Sender;
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use tokio_tungstenite::{connect_async, tungstenite::Message as WsMessage};

use super::Event;
use super::model::{Message, User};

const GATEWAY_URL: &str = "wss://gateway.discord.gg/?v=10&encoding=json";
const MAX_BACKOFF_SECS: u64 = 30;

/// Keeps a gateway connection alive forever, reconnecting with exponential
/// backoff and forwarding dispatch events to the UI.
pub async fn run(token: String, events: Sender<Event>) {
    let mut backoff = 1;
    loop {
        match connect_and_run(&token, &events).await {
            Ok(()) => {
                backoff = 1;
                let _ = events.send(Event::Status("gateway reconnecting".into())).await;
            }
            Err(err) => {
                let _ = events
                    .send(Event::Status(format!("gateway disconnected: {err:#}")))
                    .await;
            }
        }
        tokio::time::sleep(Duration::from_secs(backoff)).await;
        backoff = (backoff * 2).min(MAX_BACKOFF_SECS);
    }
}

async fn connect_and_run(token: &str, events: &Sender<Event>) -> Result<()> {
    let (mut socket, _) = connect_async(GATEWAY_URL)
        .await
        .context("could not reach the discord gateway")?;

    let mut sequence: Option<i64> = None;
    let mut heartbeat_interval: Option<Duration> = None;
    let mut next_heartbeat: Option<tokio::time::Instant> = None;

    loop {
        tokio::select! {
            incoming = socket.next() => {
                let Some(incoming) = incoming else {
                    return Ok(());
                };
                let text = match incoming.context("gateway read error")? {
                    WsMessage::Text(text) => text,
                    WsMessage::Ping(payload) => {
                        socket.send(WsMessage::Pong(payload)).await.ok();
                        continue;
                    }
                    WsMessage::Close(_) => return Ok(()),
                    _ => continue,
                };

                let payload: Value = serde_json::from_str(text.as_str())
                    .context("invalid gateway payload")?;
                if let Some(s) = payload.get("s").and_then(Value::as_i64) {
                    sequence = Some(s);
                }

                match payload.get("op").and_then(Value::as_u64) {
                    Some(10) => {
                        let interval_ms = payload["d"]["heartbeat_interval"]
                            .as_u64()
                            .unwrap_or(41_250);
                        let interval = Duration::from_millis(interval_ms);
                        heartbeat_interval = Some(interval);
                        next_heartbeat = Some(tokio::time::Instant::now() + interval);

                        let identify = json!({
                            "op": 2,
                            "d": {
                                "token": token,
                                "capabilities": 16_381,
                                "properties": {
                                    "os": "Linux",
                                    "browser": "Chrome",
                                    "device": "",
                                    "system_locale": "en-US",
                                    "browser_user_agent": "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36",
                                    "browser_version": "131.0.0.0",
                                    "os_version": "",
                                    "referrer": "",
                                    "referring_domain": "",
                                    "referrer_current": "",
                                    "referring_domain_current": "",
                                    "release_channel": "stable",
                                    "client_build_number": 350_000,
                                    "client_event_source": null
                                },
                                "presence": {
                                    "status": "online",
                                    "since": 0,
                                    "activities": [],
                                    "afk": false
                                },
                                "compress": false
                            }
                        });
                        socket
                            .send(WsMessage::text(identify.to_string()))
                            .await
                            .context("failed to send identify")?;
                    }
                    Some(11) => {}
                    Some(1) => {
                        send_heartbeat(&mut socket, sequence).await?;
                        if let Some(interval) = heartbeat_interval {
                            next_heartbeat = Some(tokio::time::Instant::now() + interval);
                        }
                    }
                    Some(7) => return Ok(()),
                    Some(9) => return Err(anyhow::anyhow!("gateway rejected the session")),
                    Some(0) => {
                        let event_name = payload["t"].as_str().unwrap_or_default();
                        match event_name {
                            "READY" => {
                                if let Ok(user) = serde_json::from_value::<User>(payload["d"]["user"].clone()) {
                                    let _ = events.send(Event::Ready(user)).await;
                                }
                            }
                            "MESSAGE_CREATE" => {
                                match serde_json::from_value::<Message>(payload["d"].clone()) {
                                    Ok(message) => {
                                        let _ = events.send(Event::MessageCreated(message)).await;
                                    }
                                    Err(err) => {
                                        let _ = events
                                            .send(Event::Status(format!("bad message payload: {err}")))
                                            .await;
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                    _ => {}
                }
            }
            _ = tokio::time::sleep_until(next_heartbeat.unwrap_or_else(tokio::time::Instant::now)),
                if next_heartbeat.is_some() =>
            {
                send_heartbeat(&mut socket, sequence).await?;
                if let Some(interval) = heartbeat_interval {
                    next_heartbeat = Some(tokio::time::Instant::now() + interval);
                }
            }
        }
    }
}

async fn send_heartbeat<S>(socket: &mut S, sequence: Option<i64>) -> Result<()>
where
    S: SinkExt<WsMessage> + Unpin,
    S::Error: std::error::Error + Send + Sync + 'static,
{
    let payload = json!({ "op": 1, "d": sequence });
    socket
        .send(WsMessage::text(payload.to_string()))
        .await
        .context("failed to send heartbeat")?;
    Ok(())
}
