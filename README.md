# rustcord

A discord desktop client written in rust with [gpui](https://gpui.rs) (zed's gpu-accelerated ui framework).

yes this is slop, no i don't care

## what works

- logs in with a **user token** and shows the account in the sidebar
- server rail with your guilds plus a `DM` button
- channel list per server, dm/group-dm list for dms
- message history per channel, live updates over the discord gateway
- sending messages, with local echo of what you sent

## warning

This uses a **user token**, which is against discord's terms of service and can get the
account banned. Bots are the sanctioned path, but they can't see friends, dm lists, etc.
Use at your own risk.

The token is never stored in the repo. It is read from:

1. `$DISCORD_TOKEN`
2. a token file: `$XDG_CONFIG_HOME/rustcord/token` (or `~/.config/rustcord/token`) on
   linux/macos, `%APPDATA%\rustcord\token` on windows

## prebuilt binary

`dist/rustcord-linux-x86_64` is a stripped release build for linux x86_64 (glibc).
It is not static, it needs these shared libs at runtime:

```sh
libxkbcommon.so.0 libxkbcommon-x11.so.0 libxcb.so.1 libXau.so.6 libXdmcp.so.6
```

plus a working vulkan driver (gpui renders through vulkan; llvmpipe/mesa works).

```sh
chmod +x rustcord-linux-x86_64
DISCORD_TOKEN='...' ./rustcord-linux-x86_64
```

checksums are in `dist/SHA256SUMS`. There is no windows build; gpui's windows
backend needs the msvc toolchain and can't be cross-compiled from here.

## build

gpui is not on crates.io and links against system graphics libraries. On debian:

```sh
sudo apt install build-essential pkg-config libssl-dev libxkbcommon-dev \
  libwayland-dev libx11-dev libxcb1-dev libxkbcommon-x11-dev \
  libfontconfig1-dev libvulkan-dev
```

then:

```sh
cargo build --release
```

There is a `[patch.crates-io]` entry that vendors a one-line fix for `xattr`, which no
longer compiles against current `libc` (`ENOATTR` was removed on linux).

## run

```sh
export DISCORD_TOKEN='...'
cargo run --release
```

or write the token to the config path above.

## headless smoke test

No window needed, useful for checking auth/gateway/rest:

```sh
DISCORD_TOKEN='...' cargo run --bin headless -- 15
DISCORD_TOKEN='...' cargo run --bin headless -- 20 --send 'hello from rustcord'
```

## layout

```
src/discord/api.rs      rest client (users, guilds, channels, messages, send)
src/discord/gateway.rs  websocket gateway: identify, heartbeat, dispatch events
src/discord/model.rs    serde models for users, guilds, channels, messages
src/discord/mod.rs      background worker thread + command/event channels
src/main.rs             gpui ui
src/bin/headless.rs     cli smoke test
```
