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
| C64 | `.sid` | **v0.2** — files are listed but skipped for now |

Duration behaviour mirrors the shell radio: `.spc` tracks get a 60 s minimum
(many rips carry tiny ID666 tags and the audio loops), every track is capped
at 15 min, and a 5 s fade-out is applied. Dead air is skipped: a track that
goes silent ends early.

Known limitation: `.vgm` files that use the NES APU (rare — NES music is
normally `.nsf`) open but play silence; game-music-emu's VGM player covers the
SN76489/YM2413/YM2612 chips.

## Building

```sh
cargo build --release           # needs: rust, a C++ compiler, libopenmpt
```

game-music-emu 0.6.6 is vendored under `vendor/gme/` and built statically by
`build.rs` (Nuked YM2612 core, no zlib — `.vgz` is gunzipped in Rust).

## Licenses

- MiNERVA-FM code: MIT (see `LICENSE`)
- game-music-emu: LGPL-2.1-or-later (vendored source in `vendor/gme/`,
  license in `LICENSES/`)
- emu2413: MIT (Mitsutaka Okazaki)

## Family

- `minerva-radio.sh` — the original shell/tmux radio this look comes from
- [MiNERVA-FM-Local](https://github.com/TheWhyteWolf/MiNERVA-FM-Local) — the
  browser/CRT edition sharing the same engines compiled to WebAssembly
