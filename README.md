# gmk104-aura

[🇧🇷 Português](README.pt-BR.md) · 🇺🇸 English

Makes the **ZUOYA GMK104** keyboard follow the color that **ASUS Aura Sync / Armoury Crate** applies to
the motherboard. When Aura is in **Smart** mode (color by CPU temperature), the keyboard turns green,
yellow or red together with the board; when Aura shows a fixed color, the keyboard copies it.

A single ~350 KB `.exe`: no installer, no admin rights, no window. Just a round icon next to the clock
showing the keyboard's current color.

> Armoury Crate and other plugins (Corsair, etc.) are left untouched. The program only **reads** what
> Aura has already decided and repeats it on the keyboard.

## Install (regular users)

1. Download `gmk104-aura.exe` from the [Releases](../../releases/latest) page.
2. Put it in a permanent folder (e.g. `C:\Users\you\gmk104-aura\`) and double-click it.
3. The first time it asks **"Start together with Windows?"**. Answer **Yes** and you're done.

From then on it starts at every logon. The icon next to the clock shows the current color; click it
for the menu:

| Item | What it does |
|---|---|
| GMK104 keyboard: connected / not found | connection state |
| Color: green — Smart mode (Aura log) | current color and where it came from |
| ☑ Start with Windows | toggles autostart (per user, no admin) |
| Open log | opens `%LOCALAPPDATA%\gmk104-aura\gmk104-aura.log` |
| Exit | quits (the keyboard keeps the last color) |

**SmartScreen warning.** The executable is not code-signed (certificates are expensive). On first
run Windows may show "Windows protected your PC": click **More info → Run anyway**. The code is open,
and the `.exe` in Releases is built automatically by GitHub Actions from this repository.

**Uninstall.** Untick "Start with Windows" in the icon menu and click "Exit". Then delete the `.exe`
and, optionally, the `%LOCALAPPDATA%\gmk104-aura` folder (just a log and a marker file).

> The UI is currently in Portuguese (menu, dialogs, log). English strings are planned.

## Requirements

- Windows 10/11 with **Armoury Crate** and **Aura Sync** working (`LightingService` running).
- **ZUOYA GMK104** keyboard (USB `320F:5055`; the 2.4 GHz receiver `320F:5088` is also accepted).
- Nothing else: no Python, drivers, VIA or admin rights.

## What it follows (and what it doesn't)

- **Fixed color** on the motherboard → same color on the keyboard.
- **Smart** (thermal) mode → green / yellow / red according to the band Aura itself picked, using the
  colors and thresholds of your profile (default 65 °C / 85 °C). It switches by band, no gradient,
  exactly like Aura; with the CPU sitting on a threshold, board and keyboard flip together.
- **Animated effects** are imitated, not streamed frame by frame (LightingService exposes no frames).
  Each Aura effect maps to the closest thing the keyboard can do, with speeds calibrated by webcam on
  a ROG STRIX X570-E:

  | Aura effect | On the keyboard |
  |---|---|
  | Static | same solid color |
  | Breathing | native breathing effect, same color, closest of 5 speed levels (Aura's `freq` is in Hz) |
  | Color cycle | hue rotation computed by the program: 12.5 s per revolution, starts at green 2.5 s after the switch, so it stays in step with the board |
  | Rainbow | native color-gradient effect (TRAVERSE); the board takes 14.5 s per revolution, the keyboard's slowest level 8 s |
  | Strobing | brightness pulsed by the program at the script frequency (2.4 Hz); your brightness level is restored afterwards |
  | Starry night | native starry effect, same color, slowest level |
  | Dark (off) | keyboard off |
  | Smart | by CPU-temperature band, from the LightingService log |
  | Music, Immersion, Adaptive color, downloaded effects (Halloween...) | not reproducible (live audio/screen input): the keyboard keeps its last color |

  Phase is never guaranteed for native keyboard effects (the keyboard runs its own animation); the
  color cycle is the exception, since both sides start from a known point.
- Keyboard **brightness** is never changed; use the keyboard's Fn keys.
- If you switch the keyboard effect with Fn, the program restores the single-color effect on the next
  refresh (5 s).
- The keyboard still does not show up in Aura Sync, SignalRGB or OpenRGB.

## How it works (for the curious)

```
LightingService (Aura)                          gmk104-aura                        GMK104
 LastProfile.xml ───── mode and base color ──┐
 LightingService.log ─ band color (Smart) ───┼─► picks the color ──VIA Raw HID──► color
                                             │   (20 checks/s,
                                             │    sends only on change)
```

1. **Profile and script**: `C:\Program Files (x86)\LightingService\LastProfile.xml` holds the mode
   (key `100` = effect described by a script; other values = fixed hardware mode, black = off) and the
   board's base color. `script\LastScript.xml` is the effect itself, written as an "AURA 3.0" wave
   script: the viewport group whose device type is `Mainboard_Master` points to an `effectList` entry
   whose key prefix names the effect (`StaticSingleEff`, `BreathSingleEff`, `ColorCycleSingleEff`,
   `RainbowSingleEff`, `StrobingSingleEff`, `StarSingleEff`, `Smartmode`...) and whose `wave` carries
   the frequency (Hz) and channel (`ALPHA` = brightness, `HUE`). Captured samples of every effect live in
   `fixtures/aura-scripts/`.
2. **Band color**: in Smart mode LightingService writes to the world-readable log
   `%ProgramData%\ASUS\ARMOURY CRATE Diagnosis\LightingService\LightingService.log` a line
   `[ParseInitColor] parsed value => hue=…, saturation=…, lightness=…` every time the band changes.
   The program tails that file (and the `.log.1`–`.log.3` backups right after a rotation).
   That is why **no admin rights** are needed: reading the temperature straight from the ASUS service
   would require them.
3. **Keyboard**: VIA Raw HID channel (`usage page 0xFF60`), channel 3 (`rgb_matrix`): effect 6 (solid
   color) and color as `hue/sat` 0..255. It only sends when the color changes, plus a refresh every
   5 s, so it does not wear the keyboard's memory. Reconnects by itself if the keyboard is unplugged.

Measured footprint: ~5 MB of RAM and ~1 % of one core, or less.

## Build from source

Needs [Rust](https://rustup.rs) (MSVC toolchain) and the Visual Studio build tools (`hidapi` compiles
a bit of C; `rc.exe` from the Windows SDK embeds the icon).

```bash
cargo build --release
```

The binary lands in `target\release\gmk104-aura.exe`. Command-line options, handy for debugging:
`-v` (also writes the log to the terminal), `--no-tray` (no icon, runs in the terminal),
`--duration N` (quits after N s), `--fps N`, `--refresh S`.

Layout: `src/keyboard.rs` (VIA HID), `src/aura_profile.rs` (profile XML), `src/aura_log.rs`
(LightingService log), `src/follower.rs` (main loop), `src/tray.rs` (tray icon, plain Win32),
`src/autostart.rs` (Run key), `src/logger.rs`.

### Calibration tools (`fixtures/calib/`)

Python + OpenCV scripts used to measure speeds with a webcam, kept so other boards/keyboards can be
calibrated the same way: `snapshot.py` (pick regions), `measure.py` (hue over time, detects the
profile change), `measure_brightness.py` (breathing period), `measure_keyboard_breath.py` /
`measure_keyboard_wave.py` (period of each keyboard speed level for a given native effect),
`survey_keyboard_effects.py` (what each of the 19 native effects does). `fixtures/run_fixtures.ps1`
replays every captured Aura script on the keyboard. Measured constants are in `src/follower.rs`.

### Python version (reference)

`python/` holds the original implementation, used to discover the protocol. It does the same, plus an
extra temperature source through the ASUS COM service (`aaHM.acpiHmData2`, elevated only). Needs
`pip install -r python/requirements.txt` and runs with `python python/run.py -v`. The scripts in
`python/tools/` (startup shortcut / scheduled task) are legacy: the `.exe` handles that itself.

## Protocols discovered

### VIA Raw HID (used)

32-byte reports (report id 0) on interface `0xFF60/0x61`:

| Command | Bytes | Notes |
|---|---|---|
| set value | `07 03 id data…` | echoed in the reply |
| get value | `08 03 id` | value in bytes 3.. |
| id 1 brightness | 0..4 | firmware clamps to 4; higher values **turn the keyboard off** |
| id 2 effect | 6 = solid color | full list in the keyboard's VIA JSON |
| id 3 speed | 0..4 | |
| id 4 color | `hue sat` | 0..255 each |

### GMK87 protocol (gmk87-node), also accepted by the GMK104

Interface `usage page 0xFF1C`, report id `0x04`, 64 bytes, 16-bit checksum = sum of bytes 3..63 in
bytes 1..2; cmd in byte 3, length in 4, position in 5..7, data from 8. Read config: `0x01`, `0x03`×10,
`0x02`, `0x05`×12 (data in reply bytes 4..7). Write: `0x01`, `0x06` (48 bytes), `0x02`. Layout: `[1]`
effect, `[2]` brightness, `[6..8]` RGB, `[9]` hue, `[21]` winlock, `[35..41]` BCD clock, `[43..44]`
frame delay. It writes the whole config (likely flash), so it is not used.

## License

MIT. Not affiliated with ASUS or ZUOYA; product names belong to their owners.
