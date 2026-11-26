use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct User {
    pub id: String,
    pub username: String,
    #[serde(default)]
    pub global_name: Option<String>,
    #[serde(default)]
    pub discriminator: Option<String>,
    #[serde(default)]
    pub bot: bool,
}

impl User {
    pub fn display_name(&self) -> &str {
        self.global_name
            .as_deref()
            .filter(|name| !name.is_empty())
            .unwrap_or(&self.username)
    }

    pub fn initials(&self) -> String {
        self.display_name()
            .chars()
            .next()
            .map(|c| c.to_uppercase().to_string())
            .unwrap_or_else(|| "?".to_string())
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct Guild {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub icon: Option<String>,
}

impl Guild {
    pub fn initials(&self) -> String {
        self.name
            .split_whitespace()
            .filter_map(|word| word.chars().next())
            .take(2)
            .collect::<String>()
            .to_uppercase()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(from = "u8")]
pub enum ChannelKind {
    Text,
    Dm,
    Voice,
    GroupDm,
    Category,
    Other,
}

impl From<u8> for ChannelKind {
    fn from(value: u8) -> Self {
        match value {
            0 => ChannelKind::Text,
            1 => ChannelKind::Dm,
            2 => ChannelKind::Voice,
            3 => ChannelKind::GroupDm,
            4 => ChannelKind::Category,
            _ => ChannelKind::Other,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct Channel {
    pub id: String,
    #[serde(rename = "type", default = "default_channel_kind")]
    pub kind: ChannelKind,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub position: Option<i64>,
    #[serde(default)]
    pub parent_id: Option<String>,
    #[serde(default)]
    pub recipients: Vec<User>,
}

fn default_channel_kind() -> ChannelKind {
    ChannelKind::Other
}

impl Channel {
    pub fn display_name(&self) -> String {
        if let Some(name) = self.name.as_ref().filter(|name| !name.is_empty()) {
            return name.clone();
        }
        if self.recipients.is_empty() {
            return "unknown-channel".to_string();
        }
        self.recipients
            .iter()
            .map(|user| user.display_name().to_string())
            .collect::<Vec<_>>()
            .join(", ")
    }

    pub fn is_text_like(&self) -> bool {
        matches!(self.kind, ChannelKind::Text)
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct Message {
    pub id: String,
    #[serde(default)]
    pub channel_id: String,
    #[serde(default)]
    pub content: String,
    #[serde(default)]
    pub timestamp: String,
    pub author: User,
    #[serde(default)]
    pub referenced_message: Option<Box<Message>>,
}

impl Message {
    /// Discord timestamps look like `2026-11-25T12:45:43.123000+00:00`.
    pub fn short_time(&self) -> String {
        let time = self.timestamp.get(11..16).unwrap_or("");
        if time.is_empty() {
            String::new()
        } else {
            format!("{time} UTC")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_dm_channel_with_recipients() {
        let json = r#"{
            "id": "1", "type": 1, "last_message_id": "9", "flags": 0,
            "recipients": [
                {"id": "2", "username": "discord", "global_name": "Discord", "discriminator": "0000", "bot": true}
            ]
        }"#;
        let channel: Channel = serde_json::from_str(json).unwrap();
        assert_eq!(channel.kind, ChannelKind::Dm);
        assert_eq!(channel.display_name(), "Discord");
        assert!(!channel.is_text_like());
    }

    #[test]
    fn parses_guild_text_channel() {
        let json = r#"{"id":"3","type":0,"name":"general","position":2,"parent_id":"4"}"#;
        let channel: Channel = serde_json::from_str(json).unwrap();
        assert_eq!(channel.kind, ChannelKind::Text);
        assert_eq!(channel.display_name(), "general");
        assert!(channel.is_text_like());
    }

    #[test]
    fn message_time_is_utc_hhmm() {
        let json = r#"{
            "id": "5", "channel_id": "6", "content": "hi",
            "timestamp": "2026-11-25T12:45:43.123000+00:00",
            "author": {"id": "7", "username": "max"}
        }"#;
        let message: Message = serde_json::from_str(json).unwrap();
        assert_eq!(message.short_time(), "12:45 UTC");
        assert_eq!(message.author.display_name(), "max");
    }

    #[test]
    fn user_falls_back_to_username_without_global_name() {
        let json = r#"{"id":"8","username":"wild.panther","global_name":null}"#;
        let user: User = serde_json::from_str(json).unwrap();
        assert_eq!(user.display_name(), "wild.panther");
        assert_eq!(user.initials(), "W");
    }
}
