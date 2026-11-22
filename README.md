# NeoYTLiveStats

Fullscreen WinForms overlay that shows live YouTube numbers for a channel and a video:
subscribers, channel views and video views, refreshed every second.

```
SUBS            KANAAL VIEWS        VIDEO VIEWS
1.234           4.567.890           12.345
```

## What it does

- Polls `https://www.youtube.com/@<handle>/about` for the channel (subscribers, views, video count) and
  `/youtubei/v1/player` for the video view count, roughly once per second.
- Channel views are only published by YouTube now and then, so the overlay syncs a baseline from the
  about page and then advances it with the live video-view delta.
- Dutch number formatting (`1.234.567`) everywhere, with parsing that accepts both `1.234.567` and `1,234,567`.
- **White flash** every N new subscribers, **rainbow flash** every N new video views and channel views
  (defaults: 10 / 100 / 100).
- Writes the current numbers to `%TEMP%\neoyt_last.json` on every successful poll (handy for debugging).

## Repository layout

| Path | What |
|---|---|
| `Program.cs` | entry point, argument parsing, saved-settings merge |
| `Form1.cs` | the overlay itself: layout, polling loop, counters, flash effects, status line |
| `SettingsForm.cs` | the ⚙ dialog, including the *Test* button that runs the fallback chain |
| `YtApi.cs` | every YouTube source, the parsers and number formatting |
| `AppSettings.cs` | `settings.json` model, load/save and normalisation |
| `AppIcon.cs` | loads the icon embedded from `app.ico` |
| `SourceHealth.cs` | the `sources.log` writer/rotator |
| `installer/` | NSIS script plus the icon and portable-ZIP build scripts |
| `dist/` | published exe, installer, portable ZIP (not committed) |

## Sources and fallbacks

Every number has more than one source, so a markup or payload change degrades into a fallback
instead of a blank screen. The chain is tried in order until subscribers **and** channel views are
known (for a video: until the view count is known):

| Channel | Video |
| --- | --- |
| 1. about page HTML | 1. InnerTube `player` (WEB client) |
| 2. InnerTube `browse` with about params | 2. watch page HTML |
| 3. InnerTube `browse` without params | 3. InnerTube `player` (ANDROID client) |
| 4. InnerTube `browse` with the MWEB client | |

Note that the InnerTube channel payload no longer carries the channel-wide view total at all — only
subscribers and the video count — so a fallback can recover subscribers while channel views keep the
last known value. That case is named in the status line (`Geen kanaal views in deze bron (innertube-…)`)
instead of quietly reusing the old number as if it were fresh.

A source counts as *changed* when it answers but the payload contains nothing the parser recognises.
In that case `FormatChanged` is set and the error text says so (`… (formaat gewijzigd?)`), which is how
"YouTube changed something" is told apart from "the network is down". The status line shows which
source produced the current numbers (`src html/vid: player`, `it-about`, `watch-html`, …).

**Staleness is never silent.** When a poll yields no number, the last known value stays on screen but
dims to grey after 15 seconds, and the live badge gains a `· stale` marker with the age of the last
successful update (`v12s c3s`). Nothing is replaced by `!` or `—` while an old value is still known.

The InnerTube key in `YtApi.cs` (`AIzaSyAO_FJ2SlqU8Q4STEHLGCilw_Wa8ESU4OY`) is YouTube's public web
client key, the same one the web player ships with. It is not a secret and not tied to an account.

## Source health log

Every source outcome is appended to `%APPDATA%\NeoYTLiveStats\sources.log`, but only when the outcome
for that source *changes* within a run. The file therefore reads as a history of breakage rather than
one line per poll; each start of the app writes one baseline line per source it uses, so restarting is
visible in the log too.

```
2026-11-07 13:20:11  kanaal   @NeofastFTL_YT       html               OK    subs=1160 views=412134
2026-11-07 13:41:02  kanaal   @NeofastFTL_YT       html               LEEG  cijfers niet in de pagina
2026-11-07 13:41:02  kanaal   @NeofastFTL_YT       innertube-about    OK    subs=1160 views=-
2026-11-07 13:44:35  video    vOCHvWdeWTE          player             FOUT  HTTP 429
```

`OK` means numbers were found, `LEEG` that the source answered but nothing was recognisable (a format
change), and `FOUT` that the request itself failed. The log rotates to `sources.log.1` past 256 KB.

## Settings screen

Click the **⚙** button in the top bar or press **F2** to open the settings dialog. It holds:

| Field | Meaning |
| --- | --- |
| Kanaalhandle | channel handle, with or without `@` |
| Video-id of link | bare video id or any watch/shorts/live/youtu.be URL |
| Wit-flits per N subs | subscriber step that triggers the white flash |
| Regenboog per N video views | video-view step that triggers the rainbow flash |
| Regenboog per N kanaal views | channel-view step that triggers the rainbow flash |

**Test** runs the fallback chain for the handle and video currently in the dialog, without saving, and
reports which source answered — for example `Kanaal via html: 1.160 subs · 412.134 kanaal views` and
`Video via player: 420 views`. A source that answers without recognisable numbers is shown with its
reason (`— Geen subscribers in deze bron (html)`), so a dead fallback is visible before it is saved.

Saving writes `%APPDATA%\NeoYTLiveStats\settings.json` and applies immediately. If the handle or the
video changed, the counters are re-baselined and a fresh poll runs right away, so a different channel
does not fire a flash on its existing numbers.

## Run

```bash
dotnet run
dotnet run -- --handle NeofastFTL_YT --video vOCHvWdeWTE
dotnet run -- -f            # start in fullscreen
```

| Argument | Meaning |
| --- | --- |
| `--handle`, `-h` | channel handle without or with `@` |
| `--video`, `-v` | video id or full URL (`watch?v=`, `/shorts/`, `/live/`, `youtu.be/`) |
| `--fullscreen`, `-f` | start borderless maximized |

Keys: **F2** opens settings, **F11** toggles fullscreen, **Esc** leaves fullscreen.

Without arguments and without a saved settings file the defaults are
`--handle NeofastFTL_YT --video vOCHvWdeWTE`. Argument values override the saved settings for that run
only; they are not written back to `settings.json`.

## Building on this machine

The project is `net8.0-windows` WinForms, so it only targets Windows.

**Linux SDKs present:** `~/.dotnet/dotnet --list-sdks` reports **8.0.428** and **10.0.404**
(`dotnet` on `PATH` is 10.0.404). The Windows build is produced with the same SDK inside the
`~/.wine-dotnet` prefix, after restoring with the Linux SDK. The repository carries that sequence as
a script:

```bash
./build-wine.sh              # -> dist/wine-x64/NeoYTLiveStats.exe
./build-wine.sh --dry-run    # print the three commands instead
```

By hand:

```bash
~/.dotnet/dotnet restore NeoYTLiveStats.csproj -r win-x64 -p:EnableWindowsTargeting=true
WINEPREFIX=~/.wine-dotnet wine 'C:\dotnet\dotnet.exe' publish NeoYTLiveStats.csproj \
  -c Release -r win-x64 --self-contained true --no-restore \
  -p:PublishSingleFile=true -p:EnableCompressionInSingleFile=true \
  -p:IncludeNativeLibrariesForSelfExtract=true -p:DebugType=none -o dist/wine-x64
```

That yields `dist/wine-x64/NeoYTLiveStats.exe` (about 145 MB uncompressed; the NSIS setup exe
compresses it to about 60 MB). On Windows the same command works with
a normal `dotnet publish`; on this machine the Wine step is needed because the Windows targeting
packs and the WinForms runtime pack are resolved by the Windows SDK.

### Running it under Wine

```bash
./run-wine.sh --handle NeofastFTL_YT --video vOCHvWdeWTE
```

`./run-wine.sh` is a one-line wrapper around
`WINEPREFIX=~/.wine-apps wine dist/wine-x64/NeoYTLiveStats.exe "$@"`.

Wine has no `WSALookupServiceBegin` implementation, so Chromium-style network-change warnings can
appear on stderr; they are harmless, and the overlay itself runs normally (verified with live
YouTube data).

## Publish (single self-contained exe)

```bash
dotnet publish -c Release -r win-x64 --self-contained true \
  -p:PublishSingleFile=true -p:EnableCompressionInSingleFile=true
```

Requires the .NET 8 SDK (`net8.0-windows`, Windows only because of WinForms).

## Installer

A per-user installer is built with NSIS from `installer\NeoYTLiveStats.nsi`:

```bash
dotnet publish -c Release -r win-x64 --self-contained true \
  -p:PublishSingleFile=true -p:EnableCompressionInSingleFile=true \
  -p:IncludeNativeLibrariesForSelfExtract=true -p:DebugType=none -o dist\publish
makensis installer\NeoYTLiveStats.nsi
```

> **On this machine:** `makensis` is not installed, so the setup exe cannot be rebuilt here. The
> copies already in `dist/` (`NeoYTLiveStats-1.0.0-setup.exe`, `NeoYTLiveStats-1.0.0-portable.zip`)
> were built earlier on a Windows machine and are still the ones to ship. Install NSIS 3
> (`apt install nsis` on Debian/Ubuntu) to rebuild locally.

The icon in `app.ico` is embedded in the exe and used for the window, the taskbar, the Start Menu
shortcut and the Apps & features entry; the installer's own icon comes from the same file. It is
generated (16–256 px, one drawing per size) by `installer\make-icon.ps1`:

```bash
powershell -NoProfile -ExecutionPolicy Bypass -File installer\make-icon.ps1
```

That writes `dist\NeoYTLiveStats-1.0.0-setup.exe`, which installs into
`%LOCALAPPDATA%\Programs\NeoYTLiveStats` without asking for administrator rights, adds a Start Menu
shortcut and registers an entry under *Apps & features* pointing at the bundled `Uninstall.exe`
(`Uninstall.exe /S` uninstalls silently, `setup.exe /S` installs silently). Settings and the source
health log stay in `%APPDATA%\NeoYTLiveStats` and are not removed by uninstalling.

The same publish output is also packed into a portable ZIP next to the setup exe
(`dist\NeoYTLiveStats-1.0.0-portable.zip`: the exe plus this README, no shortcuts and no entry under
*Apps & features*). Unpack it anywhere and run `NeoYTLiveStats.exe`; it still uses
`%APPDATA%\NeoYTLiveStats` for settings and the source health log, so the portable and installed builds
share them. Rebuild it after publishing with:

```bash
powershell -NoProfile -ExecutionPolicy Bypass -File installer\make-portable.ps1
```

## Screenshots

Screenshots belong in `docs/screenshots/`; none are committed yet. Three captures show what the app
looks like in use:

| File to add | What to capture |
|---|---|
| `docs/screenshots/overlay.png` | the overlay running with live numbers on all three counters |
| `docs/screenshots/settings.png` | the ⚙ dialog right after pressing *Test*, with the source line visible |
| `docs/screenshots/stale.png` | the greyed counters with the `· stale` badge after a failed poll |

```html
<!-- Uncomment once the files exist:
![Overlay](docs/screenshots/overlay.png)
![Settings and test](docs/screenshots/settings.png)
![Stale state](docs/screenshots/stale.png)
-->
```

## Troubleshooting

* **All counters show `—`** — the first poll has not finished, or no source answered. Open the
  settings dialog and press *Test*: it names the failing source.
* **A counter never changes** — channel views are baseline-plus-delta; a quiet channel legitimately
  moves slowly. Check `%TEMP%\neoyt_last.json` and `sources.log` for what the last poll returned.
* **`FOUT HTTP 429`** — YouTube is rate-limiting; the overlay backs off and keeps showing the last
  known numbers, marked stale.
* **Nothing appears** — the exe is a fullscreen/borderless overlay; press **Esc** to leave fullscreen
  and **F11** to toggle it again.

## Note on how this source was recovered

The original sources were lost; only the published build survived (`bin/Release/net7.0-windows/win-x64`,
a truncated `obj` folder and the single-file `NeoYTLiveStats.exe` on the Desktop). The project was
restored by extracting the deflated `NeoYTLiveStats.dll` out of the single-file bundle and decompiling it,
then rebuilding the sources around it: `Program.cs` (entry point and argument parsing), `YtApi.cs`
(YouTube/InnerTube scraping and number parsing), `Form1.cs` (layout, polling and flash effects) and,
added afterwards, `AppSettings.cs` + `SettingsForm.cs` for the settings screen.

## Credits

* UI: **Windows Forms** on .NET 8 (MIT). No third-party NuGet packages — HTTP, JSON and parsing use
  the base class library.
* Numbers and page markup belong to **YouTube**; this is a read-only overlay that scrapes public
  pages and the public web client's InnerTube endpoint.
* App icon and installer icon were drawn for this project (`installer/make-icon.ps1`).
* The recovery/decompilation work that produced this source tree was done for this repository.

## License

MIT - see [LICENSE](LICENSE).
