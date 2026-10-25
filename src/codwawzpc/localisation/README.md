# Translations

German, Italian and Spanish strings for the game, which shipped with
untranslated placeholders for these three languages. They are compiled into
the executable and loaded by [`../language.rs`](../language.rs).

Each `.txt` file has one line per string, in the game's order: the string's
id from the game's `LocaliseHashes.dat` in hex, a tab, then the text. `\n`
is a line break and `\\` a backslash; lines starting with `#` are comments.
Keep the printf placeholders of the English (`%d`, `%s`, `%%`) and use only
characters the game's fonts have (Latin-1, `•` and `€`).

`python3 scripts/review_translation.py --show <language>` prints the English
from your imported game next to each translation and checks them. See
[docs/localisation.md](../../../docs/localisation.md) for the conventions
the translations follow.
