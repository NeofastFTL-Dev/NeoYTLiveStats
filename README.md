# YTLiveStats

Fullscreen overlay that shows live YouTube numbers for a channel and a video: subscribers,
channel views and video views, refreshed every second.

```
SUBS            KANAAL VIEWS        VIDEO VIEWS
1.234           4.567.890           12.345
```

Written in Rust with [`eframe`/`egui`](https://github.com/emilk/egui) on the `glow` (OpenGL)
backend — the same stack as my other desktop apps. It used to be a WinForms app; the C# sources
are gone and this is the whole program now. The overlay itself is unchanged: same layout, same
Dutch labels, same flashing rules, same `settings.json`.

## What it does

- **Two independent pollers.** The channel (subscribers + total views) and the video (views) each
  refresh once per second on their own thread, so a slow channel fetch never freezes the video
  number or the UI.
- **Source fallback chains.** YouTube's markup changes; every number is fetched through a list of
  endpoints and the first one that yields a value wins:

  | Value | Order tried |
  | --- | --- |
  | Channel views / subscribers | channel *about* page → innertube `about` → innertube `browse` → innertube `mweb` |
  | Video views | watch page HTML → player → Android player |

- **Staleness dimming.** A number that has not been refreshed for 15 seconds is drawn dimmed, so
  you can see at a glance that it stopped updating instead of trusting a frozen value.
- **Change flashes.** New subscribers or views trigger a short flash; milestones (10, 100, 100
  steps, configurable) get a longer rainbow flash. Both are toggles in the settings window.
- **Source health log.** Every attempt at every endpoint goes into `sources.log` next to the
  settings, rotated at 256 KiB, so "why is this number wrong/empty" is answerable after the fact.
- **Last-poll dump.** The most recent state is also written to `ytl_last.json` in the temp
  directory, which makes it easy to check what the app currently believes.

Requires no API key: it parses YouTube's public pages/InnerTube responses.

## Running it

```bash
cargo run --release
```

Overlay controls: click-and-drag anywhere to move it, `Esc` closes, and the settings window is
opened from the gear button. Keyboard shortcuts and the window behaviour are the same as the old
build.

Flags:

```text
--fullscreen, -f        start in fullscreen overlay mode
--handle, -h <handle>   override the channel (e.g. NeofastFTL_YT)
--video, -v <id|url>    override the video (id or full watch URL)
--selftest              run the headless checks and exit
```

## Settings and files

`settings.json` keeps the same PascalCase keys as the .NET version, so an existing file is picked
up as-is:

| Platform | Location |
| --- | --- |
| Windows | `%APPDATA%\YTLiveStats\settings.json` |
| Linux/macOS | `$XDG_CONFIG_HOME/ytlivestats/settings.json` (or `~/.config/ytlivestats/…`) |

An old `NeoYTLiveStats` folder is migrated on first start. `sources.log` (and its `.1` rotation)
lives in the same directory.

## Building

```bash
cargo build --release            # binary in target/release/ytlivestats
cargo test                       # parser/formatter unit tests
cargo run --release -- --selftest
```

Linux builds need the usual egui/winit libraries (`libxkbcommon-dev`, `libwayland-dev`, the
`libxcb-*` and X11 dev packages, `libgl1-mesa-dev`). Windows builds need nothing beyond the Rust
toolchain; the icon is embedded into the exe through `build.rs` + `app.rc`.

CI (`.github/workflows/build.yml`) builds and tests on Linux, builds on Windows, runs the
self-test on both, and fails if the produced binary is missing or suspiciously small.

## Installer

`installer/YTLiveStats.nsi` is an NSIS script that packages `target/release/ytlivestats.exe`:

```powershell
cargo build --release
makensis installer\YTLiveStats.nsi      # writes dist\YTLiveStats-1.1.0-setup.exe
```

## License

MIT — see [LICENSE](LICENSE).
