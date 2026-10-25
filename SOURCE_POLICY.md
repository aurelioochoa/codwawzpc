# Source publication policy

This repository distributes the compatibility host and launcher. Users must
provide their own supported, unencrypted game IPA through the import command.
The project does not download the game.

Included material:

- Host and launcher source, scripts, documentation and synthetic test data.
- Generated project artwork, generated masters and portable integrity manifests.
- Achievement names, descriptions, IDs and other compatibility metadata, plus
  the project's German, Italian and Spanish translations.
- Licensed upstream runtime libraries, fonts and touchHLE test fixtures with
  their notices. These are open-source dependencies, separate from game data.

Excluded material:

- The game executable, IPA archives, extracted applications, map/data archives,
  official textures/icons/audio, original localization files and memory dumps.
- Downloaded official artwork and concept-art references.
- Gameplay screenshots and clips, personal saves, installations, logs and caches.
- Personal machine paths, credentials, private agent records, generation prompts
  and raw generation logs.
- Compiled host executables and other local build output.

Screenshot and video recording remain normal application features. They save to
each user's local application storage. This repository does not distribute any
user's captures. Local verification may create ignored UI captures and reports.

The initial public commit is an audited source snapshot with fresh history.
Upstream attribution, licenses and pinned dependency revisions are retained;
see [NOTICE.md](NOTICE.md).

Before publishing changes, stage the intended source files and run:

```sh
python3 scripts/verify_source.py
```

After committing, audit every commit reachable from the current branch:

```sh
python3 scripts/verify_source.py --history
```

The checker reads Git blobs, including binary metadata. It rejects private home
paths, common credentials, game/archive/capture paths and unreviewed binary
locations. Its positive controls check that known prohibited examples fail.
Artwork origin and dependency licensing also require human provenance review;
pattern checks alone cannot establish the origin or ownership of arbitrary data.
`.gitignore` protects local development files but does not untrack existing files
or remove them from history. Publish only the audited branch.
