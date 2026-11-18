# Ridgewire

Ridgewire is a read-only command-line tool for agents that gather web pages, repository discussions, public platform data and media transcripts. It is an English-language Rust rewrite based on Agent Reach 1.6.1. The executable is called `ridgewire`.

[![Rust checks](https://github.com/Rahulchaube1/Ridgewire/actions/workflows/check.yml/badge.svg)](https://github.com/Rahulchaube1/Ridgewire/actions/workflows/check.yml)

## Build and first run

Install a stable Rust toolchain with Cargo, then run the following commands in a terminal. The first build downloads dependencies from crates.io.

```sh
git clone https://github.com/Rahulchaube1/Ridgewire.git
cd Ridgewire
cargo test --all-targets
cargo build --release
cargo run --release -- channels
cargo run --release -- doctor
cargo install --path .
ridgewire run github search "Rust speech recognition" --limit 3
ridgewire route https://github.com/rust-lang/rust/issues/1
ridgewire read https://github.com/rust-lang/rust/issues/1
```

## Batch queries

A batch shares one HTTP client and its connection pool. The scheduler overlaps independent jobs and emits one JSON object per input in original order. A slow first job can delay the first output. Failures appear in their own result rows. Use one job for a provider that requires sequential access.

```sh
ridgewire batch --jobs 2 <<'JOBS'
{"channel":"github","action":"search","target":"whisper.cpp","limit":3}
{"channel":"v2ex","action":"hot","limit":3}
JOBS
```

The batch reader accepts up to 256 jobs or one MiB of input. Concurrency defaults to four and accepts 1 through 16. The HTTP reader and subprocess output reader cap individual responses at four MiB. Successful actions return a `data` field with the provider payload. Job failures return an English `error` string. Exit codes are 0 for success, 1 for failed jobs and 2 for command or input errors.

## Channels

Run `ridgewire run CHANNEL ACTION [TARGET] [--limit N]`. The limit is 1 through 50 for list requests. A provider can return fewer results. Omit the target for hot lists.

| Channel | Actions | Dependency or input |
| --- | --- | --- |
| web | read, search | Reads send the URL to Jina Reader. Search sends the query to Exa and requires EXA_API_KEY. |
| github | read, search | Public GitHub REST API. Accepts owner/repo and issue or PR URLs. GITHUB_TOKEN is optional. |
| v2ex | hot, node, read, user | V2EX API. Read accepts a numeric topic ID or topic URL. |
| xueqiu | quote, search, hot_posts, hot_stocks | Xueqiu endpoints. Quotes accept platform symbols. Restricted endpoints can return a provider error. |
| youtube | read, search, subtitle, transcribe | yt-dlp. Subtitle returns WebVTT, default language en. Set RIDGEWIRE_CAPTION_LANGUAGE for another track. |
| bilibili | search, read, hot, subtitle | bili executable. Read and subtitle accept BV identifiers or canonical video URLs. |
| xiaoyuzhou | transcribe | Public episode URL, curl and the local transcription tools below. |
| twitter | search, profile, tweets, thread | OpenCLI. Thread accepts a status URL or numeric ID. |
| reddit | search, read, subreddit | OpenCLI. Supply a post URL to read. |
| instagram | search, profile, posts | OpenCLI. Profile and posts accept usernames. |
| facebook | search, profile | OpenCLI. Search accepts a query and profile accepts a username. |
| linkedin | jobs, read | OpenCLI. Read accepts a job URL. |
| boss | search, read | OpenCLI. Read accepts a zhipin.com job detail URL. |
| xiaohongshu | search, read, comments | OpenCLI. Read and comments take a note URL. |

The web, GitHub, V2EX and Xueqiu handlers use Rust HTTP requests. The other handlers require the named executables on PATH. Configure OpenCLI and its browser connection with that tool before using a browser channel. Provider text retains its source language.

Transcription requires ffmpeg, whisper.cpp's whisper-cli and a compatible local model. Set RIDGEWIRE_WHISPER_MODEL to the model file. Audio is downloaded into a per-request temporary directory, decoded to mono PCM and passed to local inference. This edition does not require a cloud speech API key. Large media, inaccessible episodes and missing caption tracks can fail.

## Manage channels

```sh
ridgewire remove instagram
ridgewire doctor
ridgewire add instagram
```

These commands change channel availability for the current user. They do not install provider executables. Set RIDGEWIRE_CONFIG_DIR to isolate the configuration for a project. Channel state uses a file lock and atomic replacement. Doctor checks for executable filenames and configuration, and does not contact providers.

## Migration and verification

The upstream base is commit `c70ae6da5d39bc90e09ad2ae0575ccc89f31e284` of Panniantong/Agent-Reach. The MIT notice is in LICENSE. Upstream source is available at [the pinned revision](https://github.com/Panniantong/Agent-Reach/tree/c70ae6da5d39bc90e09ad2ae0575ccc89f31e284).
 
The catalog covers 14 channels and 40 named actions. Command syntax, JSON schemas and authentication differ from Agent Reach. Ridgewire delegates browser sessions to OpenCLI. The upstream browser manager, session migration, automatic installer, updater and Groq integration are outside this edition.

## How requests work

Use `ridgewire route URL` to see the channel and action selected by the URL router, even while offline. Use `read` to execute that route. For other actions, `run` selects an adapter directly. Both paths apply channel settings, input validation and output limits. The batch scheduler uses the same dispatcher. Native adapters reuse a Rustls-backed HTTP client. Media and browser adapters invoke an executable with an argument vector.

## Troubleshooting

| Symptom | What to check |
| --- | --- |
| Could not start a tool | Run `ridgewire doctor`, install the executable listed for the channel, and restart your shell after changing PATH. |
| Exa search requires a key | Set EXA_API_KEY in the process environment. GitHub search uses a different API and accepts GITHUB_TOKEN. |
| HTTP 403 or 429 | Check the provider’s access rules, credentials and rate limits. Reduce batch concurrency or retry later. |
| Missing subtitles | Select an available track using RIDGEWIRE_CAPTION_LANGUAGE or run local transcription with a model. |
| Unexpected URL routing | Run `ridgewire route URL`. Unsupported URL shapes may go to the public web reader. |
| Channel disabled | Run `ridgewire add CHANNEL`. |

## Development and releases

`src/main.rs` defines commands, input limits and batch execution. `adapters.rs` handles routing and provider operations. `transport.rs` bounds network and subprocess output, `media.rs` handles media extraction and transcription, and `state.rs` persists channel settings. The integration tests in `tests/cli.rs` run the actual executable.

To contribute, fork the repository, make a focused change, run `cargo test --all-targets`, and open a pull request describing the behavior and tests. Include sample input and expected JSON for adapter changes. Redact credentials and private URLs from issue reports.

GitHub Actions runs the offline tests, builds release binaries for Linux, macOS and Windows, and attaches the binaries to successful build jobs. The Linux job also reads this repository using GitHub’s live API. The workflow page reports the status of each check. External browser sessions, media tools, paid APIs and performance require separate end-to-end validation.

## License

Ridgewire is released under the MIT License. Copies and redistributed builds must include the copyright and permission notice in [LICENSE](LICENSE).
