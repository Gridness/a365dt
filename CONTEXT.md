# a365dt

a365dt downloads user-selected Anime365 releases while keeping episode and translation choices explicit.

## Language

**Run language**:
The language a365dt uses for every a365dt-authored user-facing phrase during one execution. Opaque external data such as Anime365 content, paths, URLs, commands, versions, and third-party errors is not part of the Run language.
_Avoid_: UI locale, system language

**English fallback**:
The en-US phrasing used when no supported preferred language is available or when the selected language cannot supply a valid phrase.
_Avoid_: Default locale

**Interactive session**:
An a365dt run in which a person searches for a Series and makes download choices. Help, version reporting, shell-completion generation, and maintenance commands are not Interactive sessions.
_Avoid_: Launch, invocation

**Tip**:
A short, single-line piece of a365dt guidance shown at the beginning of an Interactive session. Its source text is Markdown.
_Avoid_: Hint, startup message

**Available update**:
A published stable a365dt release whose version is semantically higher than the running version.
_Avoid_: Latest version, new version

**Installation channel**:
The distribution route through which the running a365dt executable was installed: Homebrew, WinGet, Cargo, or manual when no managed route can be identified.
_Avoid_: Installation type, package manager

**Series**:
An Anime365 title that contains episodes. The ru-RU term is «тайтл».
_Avoid_: Anime, сериал

**Series suggestion**:
A Series proposed as a likely match while the user searches by title.
_Avoid_: Search guess, search result

**Series search alias**:
Anime365-recognized shorthand or an alternative name for a Series that need not appear in its displayed title.
_Avoid_: Acronym, abbreviation

**Series catalogue**:
The collection of Series available for discovery on Anime365.
_Avoid_: Title index, title database

**Catalogue hit**:
A Series selection that reuses a Series already present in the persisted Series catalogue when the search starts. Direct URLs, cancelled searches, and failed searches are neither hits nor misses.
_Avoid_: Cache hit, API cache hit

**Episode**:
A selectable installment of a Series, identified by its Anime365 episode ID and displayed episode label. The ru-RU term is «эпизод».
_Avoid_: File, video, серия

**Episode range**:
One or more inclusive numeric intervals requested from a Series. Overlapping intervals form their union. Missing whole-number Episodes require explicit confirmation, and fractional Episodes inside the intervals form an optional subset that is included only by explicit choice.
_Avoid_: Download batch

**Translation**:
One Anime365 media release for exactly one Episode, characterized by its kind, language, and authors. A RAW release is also a Translation in Anime365 terminology; the ru-RU UI term is «перевод».
_Avoid_: Translation track

**Translation authors**:
The people or group credited for a Translation.
_Avoid_: Translation title

**Subtitle asset**:
A separate styled subtitle file exposed by Anime365 for a subtitle Translation. Its absence means the Translation's subtitles are contained in the video.
_Avoid_: Translation, caption

**Translation track**:
A set of Translations with the same kind, language, and authors across an Episode range. Its coverage is the subset of requested Episodes for which it contains exactly one Translation; the ru-RU UI deliberately also calls this «перевод».
_Avoid_: Translation, fallback

**Resolution plan**:
A mapping from every selected Episode to a chosen media resolution, consisting of one preferred resolution and any explicitly chosen exceptions. The ru-RU term for resolution is «разрешение».
_Avoid_: Automatic quality, silent fallback, качество

**Download batch**:
The selected Episodes from one Series, paired with one Translation track and one Resolution plan. The ru-RU UI term is «загрузка».
_Avoid_: Queue, playlist, пакет загрузки

**Verified download**:
Downloaded Episode media that passed its transfer completion checks and was finalized successfully.
_Avoid_: Existing file, finished transfer

**Muxed download**:
A Verified download whose separate video and Subtitle asset are packaged in one container without rendering the subtitles into the video.
_Avoid_: Burned-in subtitles, re-encoded video
