//! Headless smoke test: talks to discord without opening a window.
//!
//! Usage:
//!   DISCORD_TOKEN=... cargo run --bin headless [seconds] [--send "text"]
//!
//! Fetches the account, its guilds and DM channels, loads recent messages from
//! the first DM and (optionally) sends a message so you can watch the gateway
//! echo it back.

use std::time::{Duration, Instant};

use async_channel::{Receiver, Sender};
use rustcord::discord::{Command, Event, spawn_worker};

fn main() {
    let token = std::env::var("DISCORD_TOKEN").expect("set DISCORD_TOKEN");
    let args: Vec<String> = std::env::args().skip(1).collect();
    let seconds: u64 = args
        .iter()
        .find_map(|arg| arg.parse().ok())
        .unwrap_or(12);
    let outgoing = args
        .iter()
        .position(|arg| arg == "--send")
        .and_then(|index| args.get(index + 1))
        .cloned();

    let (command_tx, command_rx): (Sender<Command>, Receiver<Command>) = async_channel::unbounded();
    let (event_tx, event_rx): (Sender<Event>, Receiver<Event>) = async_channel::unbounded();
    spawn_worker(token, command_rx, event_tx);

    let deadline = Instant::now() + Duration::from_secs(seconds);
    let mut requested = false;
    let mut dm_channel: Option<String> = None;
    let mut sent = false;

    while Instant::now() < deadline {
        match event_rx.try_recv() {
            Ok(event) => {
                match &event {
                    Event::Ready(user) => {
                        println!("ready as {} ({})", user.display_name(), user.id);
                        command_tx.send_blocking(Command::FetchGuilds).unwrap();
                        command_tx.send_blocking(Command::FetchPrivateChannels).unwrap();
                    }
                    Event::Guilds(guilds) => {
                        println!("guilds: {:?}", guilds.iter().map(|g| &g.name).collect::<Vec<_>>());
                    }
                    Event::PrivateChannels(channels) => {
                        println!(
                            "dm channels: {:?}",
                            channels.iter().map(|c| c.display_name()).collect::<Vec<_>>()
                        );
                        if !requested {
                            requested = true;
                            if let Some(channel) = channels.first() {
                                dm_channel = Some(channel.id.clone());
                                command_tx
                                    .send_blocking(Command::FetchMessages {
                                        channel_id: channel.id.clone(),
                                        limit: 5,
                                    })
                                    .unwrap();
                            }
                        }
                    }
                    Event::Messages { messages, .. } => {
                        println!("loaded {} messages", messages.len());
                        for message in messages {
                            println!("  <{}> {}", message.author.display_name(), message.content);
                        }
                        if let Some(text) = outgoing.as_ref().filter(|_| !sent) {
                            sent = true;
                            if let Some(channel_id) = dm_channel.clone() {
                                println!("sending {text:?} to {channel_id}");
                                command_tx
                                    .send_blocking(Command::SendMessage {
                                        channel_id,
                                        content: text.clone(),
                                    })
                                    .unwrap();
                            }
                        }
                    }
                    Event::MessageSent(message) => {
                        println!("sent message {} via rest", message.id);
                    }
                    Event::MessageCreated(message) => {
                        println!(
                            "gateway event: <{}> {}",
                            message.author.display_name(),
                            message.content
                        );
                    }
                    Event::Error(err) => eprintln!("error: {err}"),
                    Event::Status(status) => println!("status: {status}"),
                    _ => {}
                }
            }
            Err(async_channel::TryRecvError::Empty) => {
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(async_channel::TryRecvError::Closed) => break,
        }
    }

    println!("done");
}
