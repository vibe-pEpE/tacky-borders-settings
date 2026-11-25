# Tacky Borders Settings

A small settings window for [tacky-borders](https://github.com/luke-you/tacky-borders), so you can change your window borders without editing `config.yaml` by hand.

## Features

- Live preview of active and inactive borders
- Border width, offset and corner style
- Active and inactive colors: Windows accent, a single color, or a two-color gradient with an angle
- Turn animations and effects (shadow/glow) on or off

## Safe editing

- Only the settings you change are rewritten. Comments, window rules and every other option in your `config.yaml` stay exactly as they were.
- Before the first save, a backup is written to `config.yaml.bak`.
- Settings this app can't edit yet, such as theme-aware dark/light colors, are left untouched.

tacky-borders reloads the config on its own (`watch_config_changes: True`), so changes show up as soon as you click **Save**.

## Usage

1. Install and run tacky-borders once, so it creates `%USERPROFILE%\.config\tacky-borders\config.yaml`.
2. Download `tacky-borders-settings.exe` and run it. It needs no installation.

## Building

```bash
cargo build --release
```

Cross-compiling from Linux:

```bash
rustup target add x86_64-pc-windows-gnu
sudo apt install gcc-mingw-w64-x86-64
cargo build --release --target x86_64-pc-windows-gnu
```

## License

MIT
