# agents.md

notes for assistants working on this repo.

## what this is

a discord desktop client in rust, ui in [gpui](https://gpui.rs), network in
`src/discord`. it logs in with a **user token**, which breaks discord's tos. don't
add anything that turns this into a bot client or that hides the warning in the readme.

## build

system deps (debian/ubuntu):

```sh
sudo apt install build-essential pkg-config libssl-dev libxkbcommon-dev \
  libwayland-dev libx11-dev libxcb1-dev libxkbcommon-x11-dev \
  libfontconfig1-dev libvulkan-dev
cargo build --release
```

- `gpui` is a normal crates.io dep (`0.2`), not a git dep.
- `[patch.crates-io]` vendors a one-line `xattr` fix (current `libc` dropped `ENOATTR`
  on linux). don't delete it.
- there are no unit tests. a change counts as verified when `cargo build --release`
  passes and the headless smoke test still works:

```sh
DISCORD_TOKEN='...' cargo run --bin headless -- 15
DISCORD_TOKEN='...' cargo run --bin headless -- 20 --send 'hello'
```

- the ui needs a working vulkan driver; llvmpipe is fine on a headless box.
- windows can't be cross-compiled from linux (gpui wants the msvc toolchain). ci does it.

## layout

```
src/main.rs             the whole ui: login screen, rail, channel list, messages, composer
src/discord/api.rs      rest client
src/discord/gateway.rs  websocket gateway, reconnects forever with backoff
src/discord/model.rs    serde models + display helpers (display_name, initials, ...)
src/discord/mod.rs      worker thread, Command/Event enums
src/bin/headless.rs     cli smoke test, no window
vendor/xattr            the crates.io patch
dist/                   checked-in linux binary + SHA256SUMS
```

## the worker

`discord::spawn_worker(token, command_rx, event_tx)` starts one thread that owns a
tokio runtime, the rest client and the gateway task. ui code only talks to it through
`Command` and `Event` over `async_channel`.

- one worker per **login attempt**. `Rustcord::begin_login` builds fresh channels and
  replaces `self.commands`; dropping the old sender makes the old worker exit.
- `Rustcord::session` is bumped on every login and sign out. event listener tasks
  compare it and drop events from a worker that was already replaced. keep that if you
  add more paths that start or stop a login.
- `Event::LoginFailed` is only for a rejected token: the ui stays on the login screen
  and shows the error. `Event::Error` is for everything else and renders the error page.
- new `Event` variants must be handled in `Rustcord::handle_event`. `headless.rs` has a
  catch-all arm, but add a useful print there if the event matters for smoke tests.

## ui notes

- gpui has no text input widget here. both inputs are hand-rolled on `on_key_down` plus
  `read_from_clipboard`: typing, backspace, `ctrl`/`cmd+v` paste, `enter` submits.
  `on_login_key` (token) and `on_composer_key` (message) are near-duplicates, so change
  both when touching input behavior.
- the login field and the composer share `input_focus`, so keys still go to the right
  place immediately after a successful login. don't give them separate handles.
- colors are the consts at the top of `main.rs`; text is `{}` interpolation into `div`
  children, no widget layer.
- messages scroll to bottom through `ScrollHandle` on new/sent messages.

## tokens

read in this order: `$DISCORD_TOKEN`, the token file
(`$XDG_CONFIG_HOME/rustcord/token`, `~/.config/rustcord/token`, `%APPDATA%\rustcord\token`),
then the login screen. pasted tokens stay in memory only.

never write a token to disk, log it, put it in an error message, or commit one.

## releases

- pushing a `v*` tag makes `.github/workflows/build.yml` build windows/linux/macos and
  attach the binaries to a github release. the version in `Cargo.toml` should match.
- `dist/rustcord-linux-x86_64` is a checked-in stripped linux build for people who don't
  want to compile. after rebuilding it, refresh the checksum:

```sh
strip dist/rustcord-linux-x86_64
sha256sum dist/rustcord-linux-x86_64 > dist/SHA256SUMS
```

## style

- lowercase, terse comments that explain why, not what.
- rustfmt is not enforced and the tree is not formatted clean. match the surrounding
  code, don't reformat files you're not otherwise changing.
- conventional commit messages (`feat:`, `fix:`, `chore:`). keep the diff small.
