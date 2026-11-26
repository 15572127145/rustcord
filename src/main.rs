use async_channel::Sender;
use gpui::{
    App, Application, Bounds, Context, FocusHandle, FontWeight, KeyDownEvent, MouseButton, Render,
    ScrollHandle, SharedString, Window, WindowBounds, WindowOptions, div, prelude::*, px, rgb,
    size,
};
use rustcord::discord::{self, Command, Event, model::Channel, model::Guild, model::Message, model::User};

const BG_MAIN: u32 = 0x313338;
const BG_SIDEBAR: u32 = 0x2b2d31;
const BG_RAIL: u32 = 0x1e1f22;
const BG_INPUT: u32 = 0x383a40;
const BG_HOVER: u32 = 0x3f4147;
const ACCENT: u32 = 0x5865f2;
const TEXT: u32 = 0xdbdee1;
const MUTED: u32 = 0x949ba4;
const DANGER: u32 = 0xf23f43;

const AUTHOR_COLORS: [u32; 8] = [
    0xf23f43, 0xf0b232, 0x23a55a, 0x00a8fc, 0x5865f2, 0xeb459e, 0x9b59b6, 0xe67e22,
];

fn author_color(id: &str) -> u32 {
    let hash = id
        .bytes()
        .fold(0u32, |acc, byte| acc.wrapping_mul(31).wrapping_add(byte as u32));
    AUTHOR_COLORS[hash as usize % AUTHOR_COLORS.len()]
}

struct Rustcord {
    user: Option<User>,
    guilds: Vec<Guild>,
    dms: Vec<Channel>,
    channels: Vec<Channel>,
    selected_guild: Option<String>,
    selected_channel: Option<String>,
    messages: Vec<Message>,
    input: String,
    input_focus: FocusHandle,
    messages_scroll: ScrollHandle,
    commands: Sender<Command>,
    status: String,
    error: Option<String>,
    loading: bool,
}

impl Rustcord {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let (command_tx, command_rx) = async_channel::unbounded();
        let (event_tx, event_rx) = async_channel::unbounded();
        let mut error = None;
        let mut status = "connecting to discord...".to_string();

        match load_token() {
            Some(token) => discord::spawn_worker(token, command_rx, event_tx),
            None => {
                error = Some(
                    "no token found. set DISCORD_TOKEN or write it to the rustcord config dir"
                        .to_string(),
                );
                status = "offline".to_string();
            }
        }

        let input_focus = cx.focus_handle();
        window.focus(&input_focus);

        let events = event_rx;
        cx.spawn(async move |this, cx| {
            while let Ok(event) = events.recv().await {
                if this
                    .update(cx, |this, cx| {
                        this.handle_event(event);
                        cx.notify();
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();

        Self {
            user: None,
            guilds: Vec::new(),
            dms: Vec::new(),
            channels: Vec::new(),
            selected_guild: None,
            selected_channel: None,
            messages: Vec::new(),
            input: String::new(),
            input_focus,
            messages_scroll: ScrollHandle::new(),
            commands: command_tx,
            status,
            error,
            loading: false,
        }
    }

    fn send_command(&self, command: Command) {
        if self.commands.send_blocking(command).is_err() {
            // worker thread is gone; nothing useful to do here
        }
    }

    fn handle_event(&mut self, event: Event) {
        match event {
            Event::Ready(user) => {
                self.user = Some(user);
                self.status = "connected".to_string();
                self.error = None;
                self.loading = true;
                self.send_command(Command::FetchGuilds);
                self.send_command(Command::FetchPrivateChannels);
            }
            Event::Guilds(guilds) => {
                self.guilds = guilds;
                self.loading = false;
            }
            Event::PrivateChannels(channels) => {
                self.dms = channels;
                if self.selected_channel.is_none() {
                    if let Some(first) = self.dms.first() {
                        self.select_channel(first.id.clone());
                    }
                }
            }
            Event::Channels { guild_id, channels } => {
                if self.selected_guild.as_deref() == Some(guild_id.as_str()) {
                    self.channels = channels;
                    if let Some(first) = self
                        .channels
                        .iter()
                        .find(|channel| channel.is_text_like())
                    {
                        self.select_channel(first.id.clone());
                    }
                }
            }
            Event::Messages {
                channel_id,
                messages,
            } => {
                if self.selected_channel.as_deref() == Some(channel_id.as_str()) {
                    self.messages = messages;
                    self.messages_scroll.scroll_to_bottom();
                }
            }
            Event::MessageCreated(message) => {
                if self.selected_channel.as_deref() == Some(message.channel_id.as_str()) {
                    self.push_message(message);
                    self.messages_scroll.scroll_to_bottom();
                }
            }
            Event::MessageSent(message) => {
                if self.selected_channel.as_deref() == Some(message.channel_id.as_str()) {
                    self.push_message(message);
                    self.messages_scroll.scroll_to_bottom();
                }
            }
            Event::Error(err) => {
                self.error = Some(err);
                self.loading = false;
            }
            Event::Status(status) => {
                self.status = status;
            }
        }
    }

    fn push_message(&mut self, message: Message) {
        if self.messages.iter().any(|existing| existing.id == message.id) {
            return;
        }
        self.messages.push(message);
    }

    fn select_channel(&mut self, channel_id: String) {
        if self.selected_channel.as_deref() == Some(channel_id.as_str()) {
            return;
        }
        self.selected_channel = Some(channel_id.clone());
        self.messages.clear();
        self.send_command(Command::FetchMessages {
            channel_id,
            limit: 50,
        });
    }

    fn select_guild(&mut self, guild_id: String) {
        if self.selected_guild.as_deref() == Some(guild_id.as_str()) {
            return;
        }
        self.selected_guild = Some(guild_id.clone());
        self.selected_channel = None;
        self.channels.clear();
        self.messages.clear();
        self.send_command(Command::FetchChannels(guild_id));
    }

    fn select_dms(&mut self) {
        if self.selected_guild.is_none() {
            return;
        }
        self.selected_guild = None;
        self.selected_channel = None;
        self.channels.clear();
        self.messages.clear();
        if let Some(first) = self.dms.first() {
            self.select_channel(first.id.clone());
        }
    }

    fn on_composer_key(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let keystroke = &event.keystroke;
        let paste = (keystroke.modifiers.control || keystroke.modifiers.platform)
            && keystroke.key == "v";

        if paste {
            if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
                self.input.push_str(text.replace('\n', " ").trim());
            }
        } else {
            match keystroke.key.as_str() {
                "enter" => {
                    self.send_input();
                    cx.notify();
                    return;
                }
                "backspace" => {
                    self.input.pop();
                }
                "space" => self.input.push(' '),
                _ => {
                    if let Some(text) = keystroke.key_char.as_ref() {
                        if !keystroke.modifiers.control && !keystroke.modifiers.platform {
                            self.input.push_str(text);
                        }
                    }
                }
            }
        }
        let _ = window;
        cx.notify();
    }

    fn send_input(&mut self) {
        let content = self.input.trim().to_string();
        let Some(channel_id) = self.selected_channel.clone() else {
            return;
        };
        if content.is_empty() {
            return;
        }
        self.input.clear();
        self.send_command(Command::SendMessage {
            channel_id,
            content,
        });
    }

    fn current_channel_name(&self) -> Option<String> {
        let selected = self.selected_channel.as_ref()?;
        if let Some(guild_id) = self.selected_guild.as_ref() {
            let _ = guild_id;
            return self
                .channels
                .iter()
                .find(|channel| &channel.id == selected)
                .map(Channel::display_name);
        }
        self.dms
            .iter()
            .find(|channel| &channel.id == selected)
            .map(Channel::display_name)
    }

    fn render_rail(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let dm_selected = self.selected_guild.is_none();

        div()
            .flex()
            .flex_col()
            .w(px(72.))
            .h_full()
            .py_3()
            .gap_2()
            .items_center()
            .bg(rgb(BG_RAIL))
            .child(
                div()
                    .id("dm-button")
                    .flex()
                    .items_center()
                    .justify_center()
                    .size(px(48.))
                    .rounded_full()
                    .when(dm_selected, |this| this.bg(rgb(ACCENT)))
                    .when(!dm_selected, |this| this.bg(rgb(BG_SIDEBAR)))
                    .text_color(rgb(TEXT))
                    .text_xs()
                    .font_weight(FontWeight::BOLD)
                    .cursor_pointer()
                    .hover(|style| style.bg(rgb(ACCENT)))
                    .on_click(cx.listener(|this, _: &gpui::ClickEvent, _window, _cx| {
                        this.select_dms();
                    }))
                    .child("DM"),
            )
            .child(div().w(px(32.)).h(px(1.)).bg(rgb(0x35373c)))
            .children(self.guilds.iter().map(|guild| {
                let selected = self.selected_guild.as_deref() == Some(guild.id.as_str());
                let guild_id = guild.id.clone();
                let guild_button_id = SharedString::from(format!("guild-{guild_id}"));
                div()
                    .id(guild_button_id)
                    .flex()
                    .items_center()
                    .justify_center()
                    .size(px(48.))
                    .rounded_full()
                    .when(selected, |this| this.bg(rgb(ACCENT)))
                    .when(!selected, |this| this.bg(rgb(BG_SIDEBAR)))
                    .text_color(rgb(TEXT))
                    .text_xs()
                    .font_weight(FontWeight::BOLD)
                    .cursor_pointer()
                    .hover(|style| style.bg(rgb(ACCENT)))
                    .on_click(cx.listener(move |this, _: &gpui::ClickEvent, _window, _cx| {
                        this.select_guild(guild_id.clone());
                    }))
                    .child(if guild.initials().is_empty() {
                        "?".to_string()
                    } else {
                        guild.initials()
                    })
            }))
    }

    fn render_channels(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let title = match self.selected_guild.as_ref() {
            Some(guild_id) => self
                .guilds
                .iter()
                .find(|guild| &guild.id == guild_id)
                .map(|guild| guild.name.clone())
                .unwrap_or_else(|| "server".to_string()),
            None => "direct messages".to_string(),
        };

        let entries: Vec<(String, String, bool)> = if self.selected_guild.is_some() {
            self.channels
                .iter()
                .filter(|channel| channel.is_text_like())
                .map(|channel| {
                    (
                        channel.id.clone(),
                        format!("# {}", channel.display_name()),
                        self.selected_channel.as_deref() == Some(channel.id.as_str()),
                    )
                })
                .collect()
        } else {
            self.dms
                .iter()
                .map(|channel| {
                    (
                        channel.id.clone(),
                        format!("@ {}", channel.display_name()),
                        self.selected_channel.as_deref() == Some(channel.id.as_str()),
                    )
                })
                .collect()
        };

        div()
            .flex()
            .flex_col()
            .w(px(240.))
            .h_full()
            .bg(rgb(BG_SIDEBAR))
            .child(
                div()
                    .px_4()
                    .py_3()
                    .border_b_1()
                    .border_color(rgb(BG_RAIL))
                    .text_color(rgb(TEXT))
                    .text_sm()
                    .font_weight(FontWeight::BOLD)
                    .child(title),
            )
            .child(
                div()
                    .id("channels")
                    .flex()
                    .flex_col()
                    .flex_1()
                    .p_2()
                    .gap_1()
                    .overflow_y_scroll()
                    .children(entries.into_iter().map(|(id, label, selected)| {
                        let button_id = SharedString::from(format!("channel-{id}"));
                        div()
                            .id(button_id)
                            .px_2()
                            .py_1()
                            .rounded_sm()
                            .text_sm()
                            .text_color(if selected { rgb(0xffffff) } else { rgb(MUTED) })
                            .when(selected, |this| this.bg(rgb(BG_HOVER)))
                            .cursor_pointer()
                            .hover(|style| style.bg(rgb(BG_HOVER)).text_color(rgb(TEXT)))
                            .on_click(cx.listener(move |this, _: &gpui::ClickEvent, _window, _cx| {
                                this.select_channel(id.clone());
                            }))
                            .child(label)
                    })),
            )
            .child(
                div()
                    .px_3()
                    .py_2()
                    .border_t_1()
                    .border_color(rgb(BG_RAIL))
                    .text_xs()
                    .text_color(rgb(MUTED))
                    .child(match self.user.as_ref() {
                        Some(user) => format!("{}  ·  {}", user.display_name(), self.status),
                        None => self.status.clone(),
                    }),
            )
    }

    fn render_messages(&self, window: &Window, cx: &mut Context<Self>) -> impl IntoElement {
        if let Some(error) = self.error.as_ref() {
            return div()
                .flex()
                .flex_col()
                .flex_1()
                .items_center()
                .justify_center()
                .gap_2()
                .p_6()
                .child(
                    div()
                        .text_color(rgb(DANGER))
                        .text_sm()
                        .font_weight(FontWeight::BOLD)
                        .child("something went wrong"),
                )
                .child(
                    div()
                        .text_color(rgb(MUTED))
                        .text_sm()
                        .text_center()
                        .child(error.clone()),
                )
                .into_any_element();
        }

        let Some(channel_name) = self.current_channel_name() else {
            return div()
                .flex()
                .flex_1()
                .items_center()
                .justify_center()
                .text_color(rgb(MUTED))
                .text_sm()
                .child(if self.loading {
                    "loading servers..."
                } else {
                    "pick a channel on the left"
                })
                .into_any_element();
        };

        div()
            .flex()
            .flex_col()
            .flex_1()
            .h_full()
            .child(
                div()
                    .flex()
                    .items_center()
                    .px_4()
                    .h(px(48.))
                    .border_b_1()
                    .border_color(rgb(BG_RAIL))
                    .text_color(rgb(TEXT))
                    .text_sm()
                    .font_weight(FontWeight::BOLD)
                    .child(channel_name),
            )
            .child(
                div()
                    .id("messages")
                    .flex()
                    .flex_col()
                    .flex_1()
                    .overflow_y_scroll()
                    .track_scroll(&self.messages_scroll)
                    .py_2()
                    .children(self.messages.iter().map(|message| {
                        let name = message.author.display_name().to_string();
                        let color = author_color(&message.author.id);
                        let content = if message.content.trim().is_empty() {
                            "[no text content]".to_string()
                        } else {
                            message.content.clone()
                        };
                        div()
                            .flex()
                            .flex_col()
                            .w_full()
                            .px_4()
                            .py_1()
                            .hover(|style| style.bg(rgb(0x2e3035)))
                            .child(
                                div()
                                    .flex()
                                    .flex_row()
                                    .items_baseline()
                                    .gap_2()
                                    .child(
                                        div()
                                            .text_sm()
                                            .font_weight(FontWeight::BOLD)
                                            .text_color(rgb(color))
                                            .child(name),
                                    )
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(rgb(MUTED))
                                            .child(message.short_time()),
                                    ),
                            )
                            .child(div().text_sm().text_color(rgb(TEXT)).child(content))
                    })),
            )
            .child(self.render_composer(window, cx))
            .into_any_element()
    }

    fn render_composer(&self, window: &Window, cx: &mut Context<Self>) -> impl IntoElement {
        let focused = self.input_focus.is_focused(window);
        let placeholder = self
            .current_channel_name()
            .map(|name| format!("message {name}"))
            .unwrap_or_else(|| "pick a channel first".to_string());

        div()
            .p_3()
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .px_3()
                    .py_2()
                    .rounded_md()
                    .bg(rgb(BG_INPUT))
                    .track_focus(&self.input_focus)
                    .on_key_down(cx.listener(Self::on_composer_key))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _: &gpui::MouseDownEvent, window, _cx| {
                            window.focus(&this.input_focus);
                        }),
                    )
                    .child(
                        div()
                            .flex_1()
                            .text_sm()
                            .text_color(if self.input.is_empty() { rgb(MUTED) } else { rgb(TEXT) })
                            .child(if self.input.is_empty() {
                                placeholder
                            } else {
                                self.input.clone()
                            }),
                    )
                    .when(focused, |this| {
                        this.child(div().w(px(2.)).h(px(16.)).bg(rgb(TEXT)))
                    }),
            )
    }
}

impl Render for Rustcord {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_row()
            .size_full()
            .bg(rgb(BG_MAIN))
            .text_color(rgb(TEXT))
            .child(self.render_rail(cx))
            .child(self.render_channels(cx))
            .child(self.render_messages(window, cx))
    }
}

fn load_token() -> Option<String> {
    if let Ok(token) = std::env::var("DISCORD_TOKEN") {
        let token = token.trim().to_string();
        if !token.is_empty() {
            return Some(token);
        }
    }

    std::fs::read_to_string(token_path()?)
        .ok()
        .map(|token| token.trim().to_string())
        .filter(|token| !token.is_empty())
}

/// `%APPDATA%\rustcord\token` on windows, `$XDG_CONFIG_HOME|~/.config/rustcord/token` elsewhere.
fn token_path() -> Option<std::path::PathBuf> {
    let base = if cfg!(windows) {
        std::env::var_os("APPDATA").map(std::path::PathBuf::from)
    } else {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(std::path::PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|home| std::path::PathBuf::from(home).join(".config")))
    }?;
    Some(base.join("rustcord").join("token"))
}

fn main() {
    discord::install_crypto_provider();
    Application::new().run(|cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(1180.), px(760.)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: Some(gpui::TitlebarOptions {
                    title: Some(SharedString::from("rustcord")),
                    ..Default::default()
                }),
                ..Default::default()
            },
            |window, cx| cx.new(|cx| Rustcord::new(window, cx)),
        )
        .expect("failed to open window");
        cx.activate(true);
    });
}
