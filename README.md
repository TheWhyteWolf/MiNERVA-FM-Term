# MiNERVA-FM — Terminal Edition

Retro video game music radio for your terminal. The third edition of the
MiNERVA radio family: a single native binary that re-creates the classic
three-pane look — now-playing header, audio-reactive spectrum, scrolling
ticker — with the music synthesised in-process. No tmux, no mpv, no external
players.

```
[ NOW PLAYING ] [RANDOM]  Vol 70%
ID:      116-9221-1
System:  Super Nintendo
Game:    Captain Commando
Track:   Aquarium
Artist:  Masaki Izutani
Colour:  pico8
----------------------------------------------------
  :   : :        :
: : : : :: : : :: : : : :: : :          :
: : : : :: : : :: : : : :: : : : :  : : :: :   :
*** [ MiNERVA RADIO ] Streaming SID, SPC, VGM, MOD…
```

## Usage

```sh
minerva-fm ~/Music/VGM          # scan a directory and go on air
minerva-fm                      # no args: play the built-in demo tracks
minerva-fm --list ~/Music/VGM   # show what a scan finds
```

| Key | Action |
|-----|--------|
| `n` | next track |
| `Space` | pause / resume |
| `c` | cycle colour scheme |
| `x` | cycle spectrum character |
| `↑` / `↓` | volume |
| `q` | quit |

Colour schemes: gameboy, gameboy_pocket, bbc_micro, pico8, cga, nes, minerva,
c64, zx_spectrum — rerolled per track like the original radio, lockable with
`--scheme`. See `minerva-fm --help` for the rest (`--volume`, `--order`,
`--subtunes`, `--max-track`, `--spc-min`, `--colors`, …).

## Formats

| Family | Extensions | Engine |
|--------|-----------|--------|
| SNES | `.spc` | game-music-emu (vendored, static) |
| Sega / general VGM | `.vgm` `.vgz` | game-music-emu |
| NES / others | `.nsf` `.nsfe` `.gbs` `.ay` `.kss` `.hes` `.gym` `.sap` | game-music-emu |
| Tracker | `.mod` `.xm` `.s3m` `.it` | libopenmpt (system) |
| Streamed | `.mp3` `.flac` `.wav` `.ogg` | symphonia (pure Rust) |
| C64 | `.sid` | libsidplayfp / reSIDfp (system) |

Duration behaviour mirrors the shell radio: `.spc` tracks get a 60 s minimum
(many rips carry tiny ID666 tags and the audio loops), every track is capped
at 15 min, and a 5 s fade-out is applied. Dead air is skipped: a track that
goes silent ends early.

Loudness: each emulator maps "full volume" to a different output level (a
full-scale SID sits ~16 dB below a full-scale SPC), so calibrated per-engine
gain trims align each format's typical loudness — measured with native 440 Hz
reference tones rendered through each engine plus RMS statistics over real
catalogue files. All trims are small cuts (no clipping risk); `--no-trim`
restores raw engine levels.

SID durations come from the HVSC Songlengths database when available: a
`Songlengths.md5` found anywhere under a scanned directory is picked up
automatically (`--songlengths FILE` overrides). Multi-subtune files (SID,
NSF, GBS, …) play a random subtune per spin — lock with `--subtunes first`.

Known limitation: `.vgm` files that use the NES APU (rare — NES music is
normally `.nsf`) open but play silence; game-music-emu's VGM player covers the
SN76489/YM2413/YM2612 chips.

## Building

```sh
cargo build --release           # needs: rust, a C++ compiler, libopenmpt, libsidplayfp
```

game-music-emu 0.6.6 is vendored under `vendor/gme/` and built statically by
`build.rs` (Nuked YM2612 core, no zlib — `.vgz` is gunzipped in Rust).

## Licenses

- MiNERVA-FM code: MIT (see `LICENSE`)
- game-music-emu: LGPL-2.1-or-later (vendored source in `vendor/gme/`,
  license in `LICENSES/`)
- emu2413: MIT (Mitsutaka Okazaki)
- libsidplayfp (system dependency): GPL-2.0-or-later — distributed binaries
  linking it are therefore effectively GPL-2.0-or-later as a combined work;
  the MiNERVA-FM source itself remains MIT

## Family

- `minerva-radio.sh` — the original shell/tmux radio this look comes from
- [MiNERVA-FM-Local](https://github.com/TheWhyteWolf/MiNERVA-FM-Local) — the
  browser/CRT edition sharing the same engines compiled to WebAssembly
