pub mod api;
pub mod gateway;
pub mod model;

use async_channel::{Receiver, Sender};

use api::Api;
use model::{Channel, Guild, Message, User};

#[derive(Debug, Clone)]
pub enum Command {
    FetchGuilds,
    FetchPrivateChannels,
    FetchChannels(String),
    FetchMessages { channel_id: String, limit: u32 },
    SendMessage { channel_id: String, content: String },
}

#[derive(Debug, Clone)]
pub enum Event {
    Ready(User),
    Guilds(Vec<Guild>),
    PrivateChannels(Vec<Channel>),
    Channels { guild_id: String, channels: Vec<Channel> },
    Messages { channel_id: String, messages: Vec<Message> },
    MessageCreated(Message),
    MessageSent(Message),
    Error(String),
    Status(String),
}

/// Installs the rustls crypto provider used by both reqwest and the gateway.
/// Safe to call more than once.
pub fn install_crypto_provider() {
    let _ = rustls::crypto::ring::default_provider().install_default();
}

/// Spawns a background thread that owns the tokio runtime, the REST client and
/// the gateway connection. UI code talks to it through the command channel and
/// receives results through the event channel.
pub fn spawn_worker(token: String, commands: Receiver<Command>, events: Sender<Event>) {
    install_crypto_provider();
    std::thread::Builder::new()
        .name("discord-worker".to_string())
        .spawn(move || {
            let runtime = match tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
            {
                Ok(runtime) => runtime,
                Err(err) => {
                    let _ = events.send_blocking(Event::Error(format!(
                        "failed to start tokio runtime: {err}"
                    )));
                    return;
                }
            };

            runtime.block_on(async move {
                let api = Api::new(token.clone());

                let gateway_events = events.clone();
                tokio::spawn(async move {
                    gateway::run(token, gateway_events).await;
                });

                match api.current_user().await {
                    Ok(user) => {
                        let _ = events.send(Event::Ready(user)).await;
                    }
                    Err(err) => {
                        let _ = events
                            .send(Event::Error(format!("login failed: {err}")))
                            .await;
                    }
                }

                while let Ok(command) = commands.recv().await {
                    let api = api.clone();
                    let events = events.clone();
                    tokio::spawn(async move {
                        handle_command(api, command, events).await;
                    });
                }
            });
        })
        .expect("failed to spawn discord worker thread");
}

async fn handle_command(api: Api, command: Command, events: Sender<Event>) {
    let result = match command {
        Command::FetchGuilds => api.guilds().await.map(Event::Guilds),
        Command::FetchPrivateChannels => api.private_channels().await.map(Event::PrivateChannels),
        Command::FetchChannels(guild_id) => api
            .guild_channels(&guild_id)
            .await
            .map(|channels| Event::Channels { guild_id, channels }),
        Command::FetchMessages { channel_id, limit } => api
            .messages(&channel_id, limit)
            .await
            .map(|messages| Event::Messages {
                channel_id,
                messages,
            }),
        Command::SendMessage { channel_id, content } => api
            .send_message(&channel_id, &content)
            .await
            .map(Event::MessageSent),
    };

    let event = result.unwrap_or_else(|err| Event::Error(err.to_string()));
    let _ = events.send(event).await;
}
