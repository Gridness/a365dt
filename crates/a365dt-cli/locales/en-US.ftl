test-value = Value: { $value }

language-unsupported = Language { $language } is not supported.
language-use-english = Continue in English?
language-invalid = Invalid language value: { $language }.
language-missing = `--lang` requires a language value.
language-suggestion = Perhaps you meant: { $language }
language-error-with-suggestion =
    { $message }
    Perhaps you meant: { $suggestion }

cli-about = Download Anime365 episodes without guessing translations
cli-command-cache = Manage the local cache
cli-command-cache-prune = Clear the local cache
cli-command-completions = Generate shell completions
cli-command-doctor = Check a365dt, Anime365, cache, and telemetry health
cli-command-purge = Permanently remove all local a365dt application data
cli-command-telemetry = Inspect or control local usage telemetry
cli-command-telemetry-clear = Clear collected telemetry without changing collection state
cli-command-telemetry-disable = Stop collecting local telemetry
cli-command-telemetry-enable = Resume collecting local telemetry
cli-command-telemetry-show = Show every collected field and its current value
cli-command-help = Print this message or the help of the given subcommand(s)
cli-option-query-or-url = Anime365 title or catalogue URL
cli-option-query = Search for a title even when it matches a command name
cli-option-output = Download directory
cli-option-jobs = Number of concurrent downloads
cli-option-debug = Show technical error details
cli-option-lang = Override the language for this run
cli-option-help = Print help
cli-option-version = Print version
cli-option-completion-shell = Shell to generate completions for
cli-option-purge-yes = Purge without asking for confirmation

cli-help-root =
    Download Anime365 episodes without guessing translations
    Usage: a365dt [OPTIONS] [QUERY_OR_URL]... [COMMAND]
    Commands:
      cache         Manage the local cache
      completions   Generate shell completions
      doctor        Check a365dt, Anime365, cache, and telemetry health
      purge         Permanently remove all local a365dt application data
      telemetry     Inspect or control local usage telemetry
      help          Print help for a command
    Options:
      --query <QUERY>...       Search even when the title matches a command
      -o, --output <DIR>       Download directory [default: .]
      -j, --jobs <JOBS>        Concurrent downloads [default: 4]
          --debug              Show technical error details
          --lang <LANG>        Language for this run
      -h, --help               Print help
      -V, --version            Print version
cli-help-cache =
    Manage the local cache
    Usage: a365dt cache <COMMAND>
    Commands:
      prune   Clear the local cache
      help    Print help for a command
    Options:
          --lang <LANG>   Language for this run
      -h, --help          Print help
      -V, --version       Print version
cli-help-cache-prune =
    Clear the local cache
    Usage: a365dt cache prune [OPTIONS]
    Options:
          --lang <LANG>   Language for this run
          --debug         Show technical error details
      -h, --help          Print help
      -V, --version       Print version
cli-help-completions =
    Generate shell completions
    Usage: a365dt completions <SHELL>
    Arguments:
      <SHELL>   Shell to generate completions for
    Options:
          --lang <LANG>   Language for this run
          --debug         Show technical error details
      -h, --help          Print help
      -V, --version       Print version
cli-help-doctor =
    Check a365dt, Anime365, cache, and telemetry health
    Usage: a365dt doctor [OPTIONS]
    Options:
          --debug         Show technical error details
          --lang <LANG>   Language for this run
      -h, --help          Print help
      -V, --version       Print version
cli-help-purge =
    Permanently remove all local a365dt application data
    Usage: a365dt purge [OPTIONS]
    Options:
      -y, --yes           Purge without asking for confirmation
          --debug         Show technical error details
          --lang <LANG>   Language for this run
      -h, --help          Print help
      -V, --version       Print version
cli-help-telemetry =
    Inspect or control local usage telemetry
    Usage: a365dt telemetry <COMMAND>
    Commands:
      clear     Clear collected telemetry
      disable   Stop collecting local telemetry
      enable    Resume collecting local telemetry
      show      Show every collected field
      help      Print help for a command
    Options:
          --lang <LANG>   Language for this run
      -h, --help          Print help
      -V, --version       Print version
cli-help-telemetry-clear =
    Clear collected telemetry without changing collection state
    Usage: a365dt telemetry clear [OPTIONS]
    Options:
          --lang <LANG>   Language for this run
          --debug         Show technical error details
      -h, --help          Print help
      -V, --version       Print version
cli-help-telemetry-disable =
    Stop collecting local telemetry
    Usage: a365dt telemetry disable [OPTIONS]
    Options:
          --lang <LANG>   Language for this run
          --debug         Show technical error details
      -h, --help          Print help
      -V, --version       Print version
cli-help-telemetry-enable =
    Resume collecting local telemetry
    Usage: a365dt telemetry enable [OPTIONS]
    Options:
          --lang <LANG>   Language for this run
          --debug         Show technical error details
      -h, --help          Print help
      -V, --version       Print version
cli-help-telemetry-show =
    Show every collected field and its current value
    Usage: a365dt telemetry show [OPTIONS]
    Options:
          --lang <LANG>   Language for this run
          --debug         Show technical error details
      -h, --help          Print help
      -V, --version       Print version

cli-error-conflict = Argument { $argument } cannot be used with { $other }.
cli-error-subcommand = Unrecognized subcommand: { $value }.
cli-error-value = Invalid value { $value } for { $argument }.
cli-error-required = Required argument not provided: { $argument }.
cli-error-missing-subcommand = Command { $command } requires a subcommand.
cli-error-equals = Use an equals sign to assign a value to { $argument }.
cli-error-too-many = Too many values for { $argument }.
cli-error-too-few = Not enough values for { $argument }.
cli-error-argument = Unexpected argument: { $argument }.
cli-error-utf8 = An argument contains invalid UTF-8.
cli-error-generic = Invalid command line.
cli-error-with-help =
    { $message }
    Run `a365dt --help` for usage.
cli-error-with-suggestion =
    { $message }
    Perhaps you meant: { $suggestion }
    Run `a365dt --help` for usage.

cancelled = Cancelled.
interrupted = Interrupted.
error-context = { $context }: { $message }
ui-prompt-display-error = Could not display the input prompt.
ui-prompt-read-error = Could not read input from the terminal.
ui-input-closed = Input closed before a response was entered.
ui-secret-display-error = Could not display the secure input prompt.
ui-secret-read-error = Could not read secure terminal input.
ui-confirm-yes-no = Enter yes or no.
selector-no-choices = No choices are available.
selector-choose-range = Choose 1–{ $last }:
selector-choose-listed = Choose one of the listed numbers.
selector-position-empty = 0 of 0
selector-position = { $first }–{ $last } of { $total }
selector-no-matches = No matches
selector-filter = Filter or #number:
selector-terminal-error = Could not use the interactive terminal.

query-command-conflict = `--query` cannot be combined with a command. Remove the command or search terms.
purge-confirm = Permanently remove all local a365dt application data and saved credentials?
purge-cancelled = Purge cancelled.
purge-error = Could not remove all local a365dt application files.
purge-success = Local a365dt application data removed
ctrl-c-listen-error = Could not listen for Ctrl+C.
app-heading = a365dt  ◆  Anime365 downloader
cache-clear-error = Could not clear the local cache.
cache-clear-success = Local cache cleared
auth-validating = Validating Anime365 access…
auth-success = Authenticated
series-selected = Selected { $title }
translation-selected = Selected { $kind }-{ $language } by { $authors }
media-loading = Loading available media…
subtitles-embedded =
    { $count ->
        [one] 1 episode has subtitles contained in the MP4.
       *[other] { $count } episodes have subtitles contained in the MP4.
    }
mux-confirm = Mux separate ASS subtitles into MKV after download?
mux-unavailable = ffmpeg is unavailable; keeping MP4 and ASS files separate.
output-create-error = Could not create output directory { $path }.
output-directory = Output: { $path }
media-task-error = An internal task stopped while loading episode media.
summary-heading = Batch summary
summary-downloaded = Downloaded
summary-skipped = Skipped
summary-failed = Failed
summary-interrupted = Interrupted
summary-size = Size
summary-elapsed = Elapsed
summary-output = Output
summary-error = { $episode }: { $error }
summary-resume = Run the same command again to resume preserved .part files.
command-suggestions =
    Unknown command or subcommand.
    Perhaps you meant:{ $commands }
    Use `--query` to search for the entered words instead.

auth-keychain-token = Using Anime365 access token from macOS Keychain.
auth-token-unavailable =
    No Anime365 access token was found and a365dt cannot prompt here.
    Run a365dt in an interactive terminal, or provide the token through the ANIME365_ACCESS_TOKEN process environment variable.
auth-opening = Opening { $url }
auth-browser-error = Could not open the browser automatically.
auth-sign-in = If Anime365 says authorization is required, sign in and reload the page.
auth-token-prompt = Paste access token:
auth-token-empty = The Anime365 access token cannot be empty.
auth-keychain-read-error-detail = Could not read the Anime365 access token from macOS Keychain: { $error }
auth-keychain-save-confirm = Save this token in macOS Keychain?
auth-keychain-save-error = Could not save the Anime365 access token in macOS Keychain.
auth-keychain-save-success = Saved access token in macOS Keychain.
auth-keychain-remove-error = Could not remove the Anime365 access token from macOS Keychain.

api-client-init-error = Could not initialize the secure HTTP client.
api-too-many-translations = Anime365 returned too many translations.
api-missing-data = Anime365 did not return the requested API data.
api-request-error = The request to the Anime365 API failed.
api-response-error = Anime365 returned a response a365dt could not read.
api-service-error = Anime365 error { $code }: { $message }
api-status-error = Anime365 rejected the API request (HTTP { $status }).
api-invalid-media-url = Anime365 returned an invalid media URL.
api-invalid-poster-url = Anime365 returned an invalid poster URL.
api-media-request-error = The request to the Anime365 media server failed.

select-no-series = No matching Anime365 series found.
select-search-results = Search results
select-unknown-type = Unknown type
select-episodes-unknown = ? episodes
select-episodes =
    { $count ->
        [one] 1 episode
       *[other] { $count } episodes
    }
select-episode-ranges = Episode ranges (examples: 1-12,16-18; 0-12.5; 5):
select-unavailable-episodes = Unavailable episodes: { $episodes }
select-missing-action = How should a365dt proceed?
select-continue-available = Continue with available episodes
select-enter-different = Enter a different selection
select-cancel = Cancel
select-fractional-confirm = Include fractional episodes { $episodes }?
select-empty = The selection contains no episodes.
select-no-translations = No translations cover any selected episode.
select-coverage = { $count }/{ $total } episodes
select-translations = Translations
select-skip-missing-confirm = Download only available episodes and skip { $episodes }?
select-no-resolutions = Anime365 returned no downloadable resolutions.
select-preferred-resolution = Preferred resolution
select-no-resolution-for = No downloadable resolution for episodes { $episodes }.
select-resolution-fallback = Fallback for episodes { $episodes }
select-missing-media-url = Anime365 omitted the { $height }p media URL
select-invalid-range = Enter ascending ranges no wider than 10,000 episodes after merging overlaps.
select-invalid-episode-number = Episode numbers must be non-negative numbers.

search-prompt = Search title or Anime365 catalogue URL:
search-cached-missing = That cached Anime365 series no longer exists.
search-label = Search title or paste Anime365 URL
search-loading = Loading title…
search-removed-missing = That cached title no longer exists; removed it.
search-input-task-error = The terminal input task stopped.
search-invalid-url = Enter an official Anime365 series catalogue URL.
search-series-missing = That Anime365 series no longer exists.

tip-label = Tip
tip-query = Use `--query` when a Series title matches a command name.
tip-doctor = Run `a365dt doctor` to inspect Anime365, cache, telemetry, and build health.
tip-ffmpeg = Install [FFmpeg](https://ffmpeg.org/) to mux separate ASS subtitles into MKV files.
tip-resume = Run the same command again to resume preserved `.part` downloads.
tip-jobs = Use `--jobs N` to change download concurrency.
tip-telemetry = Use `a365dt telemetry show` to inspect locally collected fields.
tip-output = Use `--output DIR` to choose the download location.
tip-cow = There is no cow level.
upgrade-available = 💫 Upgrade available:
upgrade-label = Upgrade
upgrade-manual = Download { $url } and replace { $executable }.

download-task = Download task
download-task-error = An internal download task stopped unexpectedly.
download-stopping = Stopping cleanly; flushing partial files…
download-cancel-channel-error = The cancellation channel closed; stopping active downloads.
download-not-started = Not started because the download was interrupted.
download-mkv-exists = MKV already exists.
download-refresh-failed = { $episode } • refresh failed: { $error }
download-partial-saved = Interrupted; the resumable partial file was saved.
download-retry = { $episode } • retry { $attempt }/{ $retries }
download-subtitle-failed = Subtitle download failed
download-muxing = { $episode } • muxing
download-resume-size-missing = The media server did not provide the file size needed to resume the download.
download-network-interrupted = The media download was interrupted by a network error.
download-invalid-resume = The media server returned invalid resume information.
download-invalid-content-range = invalid Content-Range: { $value }
download-http-rejected = The media server rejected the download (HTTP { $status }).
download-empty-file = The media server returned an empty file.
download-incomplete-file = The downloaded file was incomplete ({ $received } of { $expected } bytes).
download-file-io-error = Could not read or write a download file.
progress-episodes = episodes
progress-batch = Batch
progress-eta = ETA
mux-start-error = Could not start ffmpeg.
mux-combine-error = ffmpeg could not combine the video and subtitles.
mux-verify-error = Could not verify the muxed MKV file.
mux-empty-error = ffmpeg created an empty MKV file.
mux-write-error = Could not finish writing the muxed MKV file.
mux-save-error = Could not save the muxed MKV file.
mux-remove-video-error = The MKV was saved, but the source video could not be removed.
mux-remove-subtitle-error = The MKV was saved, but the source subtitles could not be removed.

unavailable = Unavailable
unavailable-no-observations = Unavailable (no observations)
never = Never
telemetry-cleared = Local telemetry cleared
telemetry-disabled = Local telemetry disabled
telemetry-enabled = Local telemetry enabled
telemetry-directory-error = Could not resolve the local telemetry directory.
telemetry-directory-create-error = Could not create the local telemetry directory.
telemetry-lock-open-error = Could not open the local telemetry lock.
telemetry-lock-error = Could not lock the local telemetry.
telemetry-read-error = Could not read the local telemetry.
telemetry-invalid-error = Could not read the local telemetry because it is invalid. Run `a365dt telemetry clear` to reset it.
telemetry-schema-unsupported = Local telemetry schema { $version } is unsupported. Run `a365dt telemetry clear` to reset it.
telemetry-prepare-error = Could not prepare the local telemetry.
telemetry-store-error = Could not store the local telemetry.
telemetry-opt-out-inspect-error = Could not inspect the local telemetry opt-out.
telemetry-opt-out-directory-error = Could not resolve the local telemetry opt-out directory.
telemetry-opt-out-create-error = Could not create the local telemetry opt-out directory.
telemetry-disable-error = Could not disable the local telemetry.
telemetry-enable-error = Could not enable the local telemetry.
telemetry-data-inspect-error = Could not inspect the local telemetry data.
telemetry-heading = Local telemetry
telemetry-collection = Collection
telemetry-state-disabled = Disabled
telemetry-state-enabled = Enabled
telemetry-data = Data
telemetry-opt-out = Opt-out
telemetry-schema = Schema
telemetry-first-observation = First observation
telemetry-last-observation = Last observation
telemetry-last-enabled = Last enabled
telemetry-last-disabled = Last disabled
telemetry-last-cleared = Last cleared
telemetry-first-download = First download
telemetry-last-download = Last download
telemetry-counters-heading = Collected counters
telemetry-no-counters = No counters recorded
telemetry-statistics-heading = Calculated statistics
telemetry-performance-heading = Performance observations
telemetry-no-performance = No performance observations recorded
telemetry-operation = Operation
telemetry-count = Count
telemetry-total = Total
telemetry-average = Average
telemetry-median = Median
telemetry-work-units = Work units
telemetry-samples-heading = Recent samples
telemetry-no-samples = No samples recorded
telemetry-metric = Metric
telemetry-samples = Samples

doctor-heading = a365dt doctor
doctor-section-health = Health
doctor-section-statistics = Statistics
doctor-section-build = Build
doctor-section-debug = Debug diagnostics
doctor-status-line = { $symbol } { $status }
doctor-status-healthy = Healthy
doctor-status-warning = Attention needed
doctor-status-error = Unhealthy
doctor-value-remedy = { $value } — { $remedy }
doctor-remedy-server-error = Check the network or Anime365 status, then retry
doctor-remedy-server-slow = Retry; check the network if latency remains elevated
doctor-series-cache = Series cache
doctor-fresh = Fresh
doctor-stale = Stale
doctor-not-created = Not created yet
doctor-unreadable = Unreadable
doctor-remedy-refresh-cache = Run a title search to refresh it
doctor-remedy-create-cache = Run a title search to create it
doctor-remedy-reset-cache = Run `a365dt cache prune` to reset it
doctor-remedy-enable-telemetry = Run `a365dt telemetry enable` to resume observations
doctor-remedy-reset-telemetry = Run `a365dt telemetry clear` to reset it
doctor-catalogue-hit-rate = Catalogue hit rate
doctor-api-requests = API requests
doctor-media-requests = Media requests
doctor-cache-retrieval = Cache retrieval
doctor-search = Search
doctor-search-throughput = Search throughput
doctor-downloads = Downloads
doctor-download-volume = Download volume
doctor-command-usage = Command usage
doctor-remedy-reset-observations = Reset local telemetry and collect new observations
doctor-historical = {" (historical)"}
doctor-search-rate = { $rate } Series/s{ $suffix }
doctor-remedy-run-searches = Run searches with telemetry enabled
doctor-remedy-run-downloads = Run downloads with telemetry enabled
doctor-download-volume-value = { $batches } batches · { $episodes } Episodes · { $bytes }{ $suffix }
doctor-command-count = { $commands } commands{ $suffix }
doctor-last-cache-update = Last cache update
doctor-cached-series = Cached Series
doctor-remedy-cache-prune = Run `a365dt cache prune`
doctor-version = Version
doctor-commit = Commit
doctor-profile = Profile
doctor-platform = Platform
doctor-compiler = Compiler
doctor-server-endpoint = Server endpoint
doctor-server-response = Server response
doctor-server-response-value = { $status } · { $latency }
doctor-no-http-response = No HTTP response
doctor-latency-threshold = Latency warning threshold
doctor-server-detail = Server detail
doctor-cache-age = { $age } old · TTL { $ttl }
doctor-missing = Missing
doctor-missing-lowercase = missing
doctor-cache-path = Cache path
doctor-cache-detail = Cache detail
doctor-telemetry-data-value = { $path } · { $size }
doctor-operation-latency = Per-operation latency
doctor-remedy-collect-telemetry = Collect telemetry by running searches or downloads
doctor-latency-operation = Latency · { $operation }
doctor-usage-counters = Usage counters
doctor-remedy-run-commands = Run commands with telemetry enabled
doctor-counter = Counter · { $counter }
doctor-telemetry-detail = Telemetry detail
doctor-telemetry-overhead = Telemetry overhead
doctor-telemetry-overhead-value = enabled { $enabled } ns · disabled { $disabled } ns · added { $added } ns
doctor-performance-value = average { $average } · median { $median } · { $count } observations{ $suffix }
doctor-remedy-run-activity = Run searches or downloads with telemetry enabled
doctor-performance-detail = average { $average } · median { $median } · total { $total } · { $samples } samples · { $work_units } work units
doctor-rate-value = { $percent }% · { $total } observations{ $suffix }
doctor-server-http-unavailable = Unavailable (HTTP { $status })
doctor-server-read-error = Response could not be read
doctor-server-available = Available · { $latency }
doctor-server-available-slow = Available · { $latency } · elevated latency
doctor-server-timeout = Unavailable · timed out
doctor-server-request-error = Unavailable · request failed
doctor-cache-directory-error = Could not resolve the OS cache directory.
