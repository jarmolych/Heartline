# Heartline - Claude instructions

## Usage pacing (Pro plan)
A session should never be cut off by the usage limit in the middle of a change. Pause only
between steps, at a clean point. This decides where to stop; it never shrinks or rushes the work
inside a step.

- **Budget:** about 40% of the 5-hour usage window per session, unless the user names another
  number.
- **Before each plan step, sub-step or agent launch:** check usage.
  - In the desktop app, use the usage tool (`get_usage`).
  - In the CLI, ask the user to run `/usage` and tell you the 5-hour percentage.
- **If the next piece could push past the budget** (a big refactor or a Fable run can take 15-25%
  on its own):
  1. Finish the current piece: it builds, the tests pass, nothing is left half-edited.
  2. Commit it to the task branch with a clear message, push the branch and open or update its PR
     (see "Git workflow").
  3. Write a short handoff: what's done, what's next, and exact file and function names. Put it in
     the relevant plan in `docs/plans/` under a "Progress" heading.
  4. Stop and tell the user.
- **Split big steps** into pieces that each end in a passing build, such as "frame store + tests",
  then "split analyze", so there's always a clean place to stop.
- **Effort:** use medium or high effort for normal coding. Fable and max effort are for planning
  and reviews (the `planner` agent), not long implementation runs.
- **Publishing:** whenever work stops at a clean point, push the task branch and open or update its
  pull request (never push to `main`). Keep wrap-up testing light: build + unit tests.
- **Overriding the rule:** if the user says to ignore the pacing rule, keep going.

## Git workflow: branches and pull requests, never `main`
Two people (and their Claudes) work on this repo. `main` only changes through pull requests merged
on GitHub, so it always builds and every feature can be reverted with one click.

- **Hooks:** the repo's `.githooks/` block commits on `main` and pushes to `main`. At the start of
  a session, check `git config core.hooksPath`. If it isn't `.githooks`, run
  `git config core.hooksPath .githooks` and tell the user you did.
- **Before editing any file:**
  1. Run `git status` and `git branch --show-current`.
  2. If you're on `main`: `git pull --ff-only`, then create a branch for the task with
     `git switch -c <type>/<short-name>`.
     - Types: `feat/` (new feature), `fix/` (bug), `tune/` (balance or feel), `art/` (visuals),
       `docs/` (plans and docs), `chore/` (tooling).
     - Short names are kebab-case and say what it does, such as `feat/rally-crack-lob` or
       `fix/lan-firewall-prompt`.
  3. If you're already on a task branch for this same task, keep using it. If the new request is
     unrelated, switch to `main`, pull, and start a new branch. Don't pile unrelated work into one
     branch.
- **While working:** commit to the task branch in small, building steps.
- **When a piece is done** (it builds and the unit tests pass):
  1. Push: `git push -u origin HEAD`.
  2. Open a PR with `gh pr create --fill --base main`, or a short title and a body saying what
     changed, why, and how it was tested.
  3. If the PR already exists, just push; it updates itself.
  4. Give the user the PR link.
- **Merging is the user's call,** done on GitHub with **Squash and merge** (one commit per feature
  on `main`). Never merge PRs yourself unless the user asks you to in that conversation.
- **Never:** commit to `main`, push to `main`, force-push, or bypass the hooks with
  `--no-verify`, unless the user explicitly tells you to for that one action.
- **Starting new work:** `git switch main && git pull --ff-only` first, so the branch starts from
  the latest `main`.

## Art direction
In the player's words: what makes the original striped sun great is that it has a CRT-style
texture and faded edges, so it looks right as a flat 2D image, it is synced to the song, and it is
always pulsing or moving. So:
- flat shapes with scan bars cut through them, soft glowing edges, a bass pulse, constant motion,
  and something on every element that reacts to the music
- no shaded surface textures (the player rejected a textured planet and moon), no thin spiky
  silhouettes, nothing static
- nothing flat and unlit: give objects a lit face, a dark side and an outline
- everything flows: nothing in the vista may step or flicker. Shape it from continuous time and
  from `Renderer::spec_ago` (blended spectrum history), never from a raw snapshot or a
  `floor(time)`. Silhouettes get soft edges, scan bars and a rim of light (`propPaint`,
  `striped_layer`), not a flat sprite look.
- skies stay in the striped sun's language. The player likes the cloud, lightning and wind
  weather, the sea, and details such as the palms. They rejected the turning wire-globe sky, the
  equaliser "bars" skyline and the tall arch ("cylinder oval") sky.
- the camera stays put: a quiet stretch of a song, or the gap between two songs, must not change
  the perspective
- menus (modelled on a Risk of Rain 2 screenshot): panels with a header and lines between
  groups, icons before text, condensed, soft rounded edges, a smooth font (never the pixel one).
  Long rows of choices scroll with the far ones hidden, long song and library names must fit, and
  nothing may need a modifier key.
- look at a screenshot next to the sunset before shipping a visual change

## Game design rules
- **The player chooses the difficulty; it never changes in secret.** Never make the game harder
  because the player did well (the old "rage" did, and it felt wrong). Difficulty comes only
  from what the player picked: the difficulty tier, mutators, and the depth they choose to dive
  to.

## Checking your own work
Each of these takes seconds. Replace `MUSIC` with a folder of songs on this PC and `FILE` with one
song.
- Unit tests: `"$USERPROFILE/.cargo/bin/cargo.exe" test --release`
- Balance: `heartline.exe --sim MUSIC --sample 6 --bot-delay 120` (`--diff N`, `--song-idx N` for
  a later depth, `--give N` to start with N upgrades). Compare hits per minute with the last
  numbers in `PROJECT_STATE.md`.
- What the analysis hears: `heartline.exe --analyze FILE` (sections, phases, layers, ear candy).
- Screenshots: `heartline.exe FILE --mute --selftest tmp/s --start 40 --shots 45,60 --secs 25
  --size 1280x720`, then read the PNGs. Menu screens: no file, `--wall --screen
  setup|options|codex|live|results`; `--library PATH` uses a folder or playlist as the library
  for that launch (handy for long names). Looks: `--mood scene,weather,floor,prop,ground`,
  `--mood-to ...` (sky transition), `--purgatory`; add `--drift` with a file to see the vista
  while a song plays. `--swap 1|-1` trades the first song two seconds in. Codex:
  `--screen codex --mode PAGE --give ENTRY`. Launch opening: `FILE --screen launch`.
- Player sync notes: while playing, T taps along and G / B / C mark good / off / ear candy; the
  notes go to `heartline-notes.txt` beside the exe. `heartline.exe --notes-from FILE` reads them.
- None of this tells you how the game feels or sounds. Say so when reporting.

## Project basics
- Rust is not on PATH: `"$USERPROFILE/.cargo/bin/cargo.exe"`.
- Keep CRLF line endings, use pure-Rust dependencies, and keep level generation deterministic.
- Specialist agents live in `.claude/agents/`. Plans live in `docs/plans/`.
