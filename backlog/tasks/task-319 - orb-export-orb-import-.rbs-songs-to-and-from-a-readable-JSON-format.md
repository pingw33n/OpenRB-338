---
id: TASK-319
title: 'orb export / orb import: .rbs songs to and from a readable JSON format'
status: To Do
assignee:
  - '@agent-complex'
created_date: '2026-11-27T07:48:00Z'
labels:
  - cli
  - docs
dependencies: []
references:
  - crates/rbs/src/lib.rs
  - crates/orb-ui/src/pattern_json.rs
  - docs/RBS42.txt
priority: medium
type: feature
ordinal: 290700
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Why: the user wants .rbs songs in a readable, editable text form (requested 2026-11-27). The main goal: an agent can write a whole song, or only a set of patterns, as text, and turn it into a song that ReBirth and OpenRB-338 load. Today the only text form is a single pattern: the clipboard JSON of Edit > Copy/Paste Pattern (crates/orb-ui/src/pattern_json.rs, task-287, version 1).

What to build:
- `orb export SONG.rbs [-o OUT.json]`: writes the song as JSON (stdout by default). It reads RBS 4.x and ReBirth 1.x songs, like `orb info`.
- `orb import IN.json -o OUT.rbs [--base SONG.rbs]`: writes an RBS 4.2 song through rbs::Song::write.
- `orb import --check IN.json`: validates without writing.

The format (the supervisor's recommendation, accepted unless the user changes it in the format review below):
- Strict JSON, with "$schema" pointing at a JSON Schema (draft 2020-12). The schema is committed in the repo and published with the site, e.g. https://pingw33n.github.io/OpenRB-338/schema/song-v1.json. The document also carries a format name and a "version": 1.
- Why JSON and not YAML/TOML/a DSL: agents write JSON most reliably; JSON Schema gives editor completion and validation; YAML's implicit typing (no, 010) and whitespace errors are pitfalls; TOML gets verbose for 4 devices x 32 patterns x 16 steps; a custom DSL has no tooling.
- Compact, so a whole song stays small for an agent's context:
  - each drum instrument's 16 steps as one lane string ("x...x...x...x..." for the 808; the 909's levels as distinct characters);
  - a 303 step as a short token (note plus flags for up, down, accent, slide; a pause character);
  - patterns keyed by their bank names (A1..D8);
  - the song track's events with positions in bars and steps and named controls and values, not raw ctrl bytes.
  Any byte the compact form cannot express falls back to its number, as the clipboard JSON does, so nothing is lost.
- Lossless: everything rbs::Song holds is in the document, including the Head/Glob/Usri fields, the mixer, the effects, the track trailers and the unknown chunks (raw bytes as hex or base64 in a clearly raw section).
- Partial documents: every field may be omitted; an omitted field takes a documented default (an empty new song), or the --base song's value. A pattern set is a document with only patterns; `--base` merges it into an existing song.
- Errors: each refusal names the JSON path and the reason (e.g. /patterns/303-1/A3/steps/5: note "H" is not a note name), exit code 1.
- Clipboard: Paste Pattern also accepts a pattern object taken from an export. Whether Copy Pattern switches to the export's pattern form is part of the format review.

Agent-facing documentation: docs/song_json.md describes what every field means musically, written so an agent can compose a song from it alone:
- knob ranges and what they do;
- the 303's notes, octaves and transpose;
- what accent, slide and flam do;
- the pattern banks;
- what each song-track control does and how pattern changes and mutes are written;
- the tempo and shuffle units;
- a worked example of a short song.

Format review first: before implementing, the agent writes the format proposal into the task notes (the schema outline plus a sample of a short song with one pattern per device and a few song-track events) and stops for the user's review; the supervisor relays it. Implement only after the user approves.

Not needed: separate editing commands (set step, transpose). An agent edits the JSON directly; import/export plus --base, --check and good errors are enough. `orb info` and `orb render` already give the check loop.

Context: crates/rbs/src/lib.rs (Song, parse, write, TRACK_NAMES), docs/RBS42.txt, docs/song_load.md, crates/orb-cli/src/info.rs (1.x reading), crates/orb-ui/src/pattern_json.rs.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The user approved the format proposal (schema outline and sample) recorded in the notes before implementation
- [ ] #2 orb export then orb import gives a byte-identical .rbs for every RBS 4.x song of the corpus (tests/refs/manifest.json) and, with ORB_THIRDPARTY_SONGS_DIR set, every 4.x third-party song; a 1.x song imports to a 4.2 song whose orb render hashes as the original's (the counts in the notes)
- [ ] #3 Every export validates against the committed JSON Schema (tested), and the schema is published with the site
- [ ] #4 A partial document (only patterns, or only some fields) imports with the documented defaults, and --base merges it into an existing song; both tested
- [ ] #5 Invalid input is refused with the JSON path and the reason; --check validates without writing; tested
- [ ] #6 Paste Pattern accepts a pattern object taken from an export (tested)
- [ ] #7 docs/song_json.md documents every field's musical meaning with a worked example; a song written by hand from the doc alone imports and renders (the example is a test)
<!-- AC:END -->
