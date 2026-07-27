# a365dt

[![CI](https://github.com/Gridness/a365dt/actions/workflows/ci.yml/badge.svg)](https://github.com/Gridness/a365dt/actions/workflows/ci.yml)
[![Coverage](https://codecov.io/gh/Gridness/a365dt/graph/badge.svg?branch=main)](https://codecov.io/gh/Gridness/a365dt)
[![License](https://img.shields.io/github/license/Gridness/a365dt)](LICENSE)

Download Anime365 episodes with the translation and video quality you want.

`a365dt` searches the Anime365 catalogue, lets you choose episodes,
translation, and resolution, then downloads the selected releases in
parallel. Interrupted downloads can be resumed by running the same command
again.

## Install

### Homebrew

The Homebrew package supports Apple Silicon macOS and ARM64 or x86-64 Linux:

```console
brew install Gridness/oosama/a365dt
```

Install the optional `ffmpeg-full` dependency to mux subtitles into MKV files:

```console
brew install Gridness/oosama/a365dt --with-ffmpeg-full
```

### Release binary or Cargo

Download a binary for Linux, macOS, or Windows from the
[latest release](https://github.com/Gridness/a365dt/releases/latest), or
install from source with Rust:

```console
cargo install --git https://github.com/Gridness/a365dt --bin a365dt
```

[FFmpeg](https://ffmpeg.org/) is optional. When it is available, `a365dt` can
mux separate ASS subtitles into MKV files.

## macOS

- The prebuilt macOS release and Homebrew formula support Apple Silicon.
- Your Anime365 access token can be stored securely in macOS Keychain.
- Series posters appear inline in compatible terminals such as iTerm2,
  Kitty, Ghostty, WezTerm, Rio, and Warp.

## Usage

Start the interactive search:

```console
a365dt
```

Prefill it with a title or open an Anime365 catalogue URL directly:

```console
a365dt "Frieren"
a365dt "https://anime365.ru/catalog/road-of-naruto-30887/"
```

Choose an output directory and the number of concurrent downloads:

```console
a365dt --output ~/Videos --jobs 8 "Frieren"
```

On first use, `a365dt` opens Anime365 so you can obtain an access token. For
non-interactive use, provide it through `ANIME365_ACCESS_TOKEN`.

Other commands:

```console
a365dt completions zsh
a365dt cache prune
a365dt --help
```

## Development

The Rust workspace lives in `crates/`. Run the repository checks through
`just`:

```console
just fmt
just clippy -p a365dt-cli
just test -p a365dt-cli
```

## License

Licensed under the [Apache License 2.0](LICENSE).
