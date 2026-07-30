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

Interactive sessions show one bundled Tip and check GitHub for a newer stable
release at most once every ten minutes. Update checks time out after two
seconds and fail silently; when an update is available, a365dt shows
instructions for the detected installation channel.

Prefill it with a title or open an Anime365 catalogue URL directly:

```console
a365dt "Frieren"
a365dt "https://anime365.ru/catalog/road-of-naruto-30887/"
```

Use `--query` when a title is also a command name:

```console
a365dt --query telemetry
a365dt --query cache prune
```

Command names followed by more title words remain searches:

```console
a365dt cache this
a365dt telemetry this
```

Likely command or subcommand typos show the nearest commands instead.
Use `--query` to bypass that check and force a title search.

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
a365dt doctor
a365dt doctor --debug
a365dt purge
a365dt purge --yes
a365dt telemetry show
a365dt telemetry disable
a365dt update
a365dt --help
```

`a365dt update` performs a fresh, read-only update check. It prints upgrade
instructions for the detected installation channel when a newer stable release
is available.

`purge` removes a365dt cache, configuration, local data, and the saved macOS
Keychain token. It does not remove downloaded media or the installed program.

Local telemetry is enabled by default and records aggregate command,
catalogue, and download outcomes without titles, queries, identifiers, URLs,
paths, or tokens. It is never transmitted. Use `a365dt telemetry show` to
inspect every collected field, `disable` or `enable` to change collection
state, and `clear` to erase the history without changing that state.
Performance observations cover Anime365 API and media requests, Series cache
retrieval and storage, and local Series search indexing and ranking. They
retain lifetime counts and totals plus only the latest 101 latency samples.
A catalogue hit means the selected Series was already in the persisted
catalogue when search began. Download success includes both downloaded and
previously verified, skipped Episodes.

`a365dt doctor` checks Anime365 reachability and latency, cache freshness,
local telemetry health, recent performance, usage statistics, build details,
and whether a newer stable a365dt release is available. `--debug` adds paths,
raw per-operation latency, collection dates, update-check failures, and a local
telemetry-overhead benchmark. Statuses use `✓`, `●`, `○`, and `✗`; unavailable
fields include the command or action needed to restore them.

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
