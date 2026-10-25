# Languages

The iPhone release of Call of Duty: World at War: Zombies offers English and
French. It was built for six languages: its data has a string table for
German, Italian and Spanish too, but those three were never translated and
the game refuses to load them. codwawzpc lets the game use them, with
translations made for this project.


## What shipped

`_main.dz` holds the game's text in `localisation/`:

| File | Game id | Contents |
| --- | --- | --- |
| `english_us.dat` | 0 | English |
| `english_uk.dat` | 1 | English; the game's English |
| `french.dat` | 2 | French |
| `italian.dat` | 3 | UK English |
| `german.dat` | 4 | UK English |
| `spanish.dat` | 5 | UK English |
| `LocaliseHashes.dat` | | The id of each of the 732 strings |

`italian.dat`, `german.dat` and `spanish.dat` are byte for byte
`english_uk.dat` except for their first string, `Boilerplate UK`, which reads
`Boilerplate IT`, `DE` and `ES`: placeholders made ready for translations
that never came. The US and UK tables differ in that same first string and
in one achievement description, where the US table repeats the description
after it. The 1.4.1 release has the same seven files, with markers that show
the three placeholder tables were kept up to date; see the
[field report](languages-and-voices.md) for that, the voices, and
everything else found in the game.

The code was ready for all six. The game maps the device language (English,
French, German, Spanish, Italian) to these ids, looks pictures with text in
them up by these names, and loads `localisation/<name>.dat` when the language
changes. Two things stop it:

- `CLocalise::SetLanguage` accepts only ids 0 to 2 on iPhone (with the
  iPad configuration's `SingleLanguage=1`, only UK English).
- Options → Language lists only English and French.

Each table is `LOC0`, a buffer size, the length of the strings, the string
count, an offset per string, the encoding name (`utf-8`) and the strings,
each ended by a NUL. The game allocates its string buffer once, sized by the
first table it loads, 47,494 bytes for all six files, and always reads that
many bytes into it.

## What codwawzpc changes

`src/codwawzpc/language.rs` patches the game when it opens its first archive:

- SetLanguage accepts ids 3 to 5.
- The string table loader is replaced. For Italian, German and Spanish it
  copies this project's translation (`src/codwawzpc/localisation/*.txt`, in
  the executable) into the game's table, after checking each string's id
  against the game's `LocaliseHashes.dat`. If the translation does not fit
  the game's buffer, it gets a larger one. Any other table is loaded by the
  game's own loader, so English and French are the game's own.
- `CGuiFrameLocalised`, which holds one picture per translated language
  (`english_us`, `english_uk`, `french`) and shows the one named after the
  current language, shows the English picture for the new languages
  instead of none.
- The **Language** setting is written into the saved game's options the
  first time the frontend starts. That is where the game keeps the language
  chosen on its Language screen and applies it from.

Nothing in the game's files changes.

## Choosing a language

```sh
codwawzpc run --language=german     # or italian, spanish, english, french
codwawzpc run --language=game       # the default: the game's own choice
```

In the launcher it is Language in Settings & controls → Display & sound. It
is remembered in `settings.plist` as `Language`.

- **game**, the default, keeps the language chosen on the game's own Language
  screen: English for a new save, or whatever is saved, including a language
  set earlier with `--language`.
- Any other value is set at every launch. The game's Language screen still
  lists only English and French. Choosing one there switches at once and
  lasts until the next launch.

## The translations

The translations were written for this project by an AI assistant (Claude)
and checked in the game and against the English, but have not yet been
reviewed by native speakers; corrections are welcome.

All 732 strings are translated except the credits' names and job titles,
which stay in English as they do in the French release; their few
connecting phrases ("Created by", "Special Thanks To:") are translated.

- German uses *du*, Italian *tu*, and Spanish is the Spanish of Spain, with
  *tú* and *vosotros*.
- Names stay as they are: maps, weapons (Ray Gun, Wunderwaffe DG-2,
  Bouncing Betty), Perk-a-Cola drinks, Pack-a-Punch, and achievement names
  that are already German ("Das ist mein Haus", "Wunderkind"). Other
  achievement names are translated, as in French.
- Richtofen speaks German in the German translation. In Italian and Spanish
  he keeps the German words of the English ("Verdammt", "schnell",
  "Höllenschweine").
- The power-ups are Maximale Munition, Sofort-Kill, Doppelte Punkte,
  Atombombe; Munizioni max, Uccisione istantanea, Punti doppi, Bomba
  atomica; Munición máxima, Muerte instantánea, Puntos dobles, Bomba atómica.
- Distances are metric (15 m, 16 km), prices are in euros, and rankings use
  1., 1° and 1.º.

The text was checked in the game for fit: labels next to values (German
"Modus:" rather than "Schwierigkeit:"), the touch-control diagrams, and the
three-line notes under Single Player were shortened where they overlapped.
Hand-wrapped help pages are no wider than the English ones. The game's fonts have
the Latin-1 characters (ä, ß, ñ, ¿, ¡, ì, ò…) but not typographic quotes or
dashes, so the translations use `"` and `-`.

### Not translated

- Pictures with words in them stay English: the game only has English and
  French versions of them.
- Online play, profiles and map purchases no longer work, but their text is
  translated.

## Editing a translation

Each file has one line per string in the game's order: the string's id in
hex, a tab and the text, with `\n` for a line break and `\\` for a
backslash. Keep printf placeholders (`%d`, `%s`, `%%`) exactly as in English.
The files contain no English; to see it next to the translation, run:

```sh
python3 scripts/review_translation.py --show german
```

It reads the English from your imported game and also checks every
translation: the ids and their order, the placeholders, and characters the
game cannot draw. `cargo test` checks the files' format, their ids against
each other and the characters. Rebuild after editing; the translations are
compiled into the executable.
