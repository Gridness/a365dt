# a365dt

a365dt downloads user-selected Anime365 releases while keeping episode and translation choices explicit.

## Language

**Series**:
An Anime365 title that contains episodes.
_Avoid_: Anime

**Episode**:
A selectable installment of a Series, identified by its Anime365 episode ID and displayed episode label.
_Avoid_: File, video

**Episode range**:
One or more inclusive numeric intervals requested from a Series. Overlapping intervals form their union. Missing whole-number Episodes require explicit confirmation, and fractional Episodes inside the intervals form an optional subset that is included only by explicit choice.
_Avoid_: Download batch

**Translation**:
One Anime365 media release for exactly one Episode, characterized by its kind, language, and authors. A RAW release is also a Translation in Anime365 terminology.
_Avoid_: Translation track

**Subtitle asset**:
A separate styled subtitle file exposed by Anime365 for a subtitle Translation. Its absence means the Translation's subtitles are contained in the video.
_Avoid_: Translation, caption

**Translation track**:
A set of Translations with the same kind, language, and authors across an Episode range. Its coverage is the subset of requested Episodes for which it contains exactly one Translation; choosing incomplete coverage explicitly reduces the Download batch.
_Avoid_: Translation, fallback

**Resolution plan**:
A mapping from every selected Episode to a chosen media resolution, consisting of one preferred resolution and any explicitly chosen exceptions.
_Avoid_: Automatic quality, silent fallback

**Download batch**:
The selected Episodes from one Series, paired with one Translation track and one Resolution plan.
_Avoid_: Queue, playlist

**Verified download**:
Downloaded Episode media that passed its transfer completion checks and was finalized successfully.
_Avoid_: Existing file, finished transfer

**Muxed download**:
A Verified download whose separate video and Subtitle asset are packaged in one container without rendering the subtitles into the video.
_Avoid_: Burned-in subtitles, re-encoded video
