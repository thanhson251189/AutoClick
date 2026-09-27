# Automatic Mouse and Keyboard

Windows recorder / player for mouse, keyboard, and on-screen pictures.

Version **0.3.0**. English and Vietnamese UI. Aimed at RobotSoft AMK behavior — the core recorder/player/scripting standard is covered; see "What this is not" for the still-missing heavy features.

## What works

- Record mouse, keys, wheel, **and drags** (press → drag → release becomes one Mouse Drag); multi-click runs (double/triple) are collapsed correctly; side mouse buttons are ignored
- Hotkeys (global on Windows): **F9** record on/off, **F10** play/pause, **F12** stop, **Ctrl+P** pause, **F7** step into, **F8** step over. Hotkey combos are never recorded as steps, so playback cannot re-trigger them
- Play, pause, repeat (once / N / duration / until stopped)
- Mouse Move with an optional duration (glide like AMK, 0 = instant)
- Smart Click and Search Picture (BMP). No match → no click. Full-virtual-screen search on multi-monitor setups
- If / Else, For (honors `step`, including negative), While, **Break / Continue**, **Switch / Case**, Label / Goto
- Key Down / Key Up to compose press-and-hold
- Set Clipboard / Get Clipboard (CF_UNICODETEXT) / **Wait Clipboard** (blocks until the text changes)
- **Random Mouse** position inside a rectangle; **Read / Write JSON** by dot path; **Read / Write Registry** (HKCU, HKLM string values); **Play Random Script** from a folder
- **Expression functions** in values and conditions (case-insensitive): string `LEN UPPER LOWER TRIM MID LEFT RIGHT REPLACE INSTR VAL STR ASC CHR`, math `ABS SQRT CEIL FLOOR ROUND POW MOD MIN MAX RAND`, date/time `NOW TODAY YEAR MONTH DAY HOUR MINUTE SECOND WEEKDAY TICKS`, system `FILE_EXISTS FILE_SIZE FILE_READ FILE_WRITE ENV CLIP`
- Wait Window: block until a window title appears (timeout + on-fail behavior)
- Wait until `HH:MM`: the current minute counts as arrived; a minute already passed is skipped
- Random number stays inside the inclusive range
- Activate / Close / Wait / **Minimize / Maximize / Restore** window by part of the title
- **Copy / Paste HTML** on the clipboard (Windows "HTML Format") via Set/Get Clipboard HTML
- **Rename Variable** (Tools): renames assignments, `{name}` references, and whole words in If/While/Switch expressions
- Variables: copy a name, `{name}` in text / keys / commands / paths / window titles, and math `n + 1` (`+ - * /`). Play Script shares variables with the caller
- Message Box, Call Function, Play Script (child scripts parse once per run)
- Scheduled tasks fire at `HH:MM` **while the app window is open**. Language, hotkeys, repeat, and tasks are restored next launch
- Headless: `automatic-mouse-keyboard --run file.amk` (exit code 1 when the run fails; logs stream to stdout)
- File → Write launcher (`.cmd`) next to the `.amk` (player exe must be on PATH)
- File → **Compile to EXE**: copies the player executable and embeds the script after a marker — a single-file EXE that plays the script on start (no install needed)

## What this is not

Driver-level input, transparent-pixel templates, background window ops, OCR, regex, Invoke DLL / WinAPI / COM, custom windows, AutoHotkey embed, database/Excel, multi-threading, custom icon/copyright in compiled EXEs.

## Build / run

Requires Rust and MSVC “Desktop development with C++”.

```
BUILD-RUST.bat
CHAY.bat
cargo test
cargo fmt --check
cargo clippy --all-targets -- -D warnings
```

See `HUONG-DAN.txt` for the Vietnamese user guide.
