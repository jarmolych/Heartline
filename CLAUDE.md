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

## Project basics
- Rust is not on PATH: `"$USERPROFILE/.cargo/bin/cargo.exe"`.
- Keep CRLF line endings, use pure-Rust dependencies, and keep level generation deterministic.
- Plans live in `docs/plans/`.

## Agents: use them rarely
Almost all progress on this game has come from one session working directly and checking its own
work with the tools below. Sub-agents start cold, re-read the project, and on 2026-11-14 running
several at once used up the plan's allowance twice. So:
- **Do the work inline by default.** Do not spawn an agent unless the user asks for one, and
  never run more than one at a time.
- **Three agents remain** in `.claude/agents/`, each for work that is truly separate and large:
  `planner` (a written plan for a multi-day feature, before any code), `audio-dsp-engineer`
  (`analysis.rs` signal-processing work) and `graphics-programmer` (`render.rs` shader work).
- Removed on 2026-11-15 because the lead does these jobs better inline: `playtester` (use the
  commands below), `code-reviewer` (use `/code-review`), `gameplay-programmer` and
  `network-programmer`.

## Checking your own work (this replaces a playtester)
Run these yourself after a change; each takes seconds.
- Unit tests: `"$USERPROFILE/.cargo/bin/cargo.exe" test --release`
- Balance: `heartline.exe --sim "E:\Libraries\Music\Straftat" --sample 6 --bot-delay 120`
  (`--diff N`, `--song-idx N` for a later depth, `--give N` to start with N upgrades). Compare
  hits per minute with the last numbers in `PROJECT_STATE.md`.
- What the analysis hears: `heartline.exe --analyze FILE` (sections, phases, layers, ear candy).
- Screenshots: `heartline.exe FILE --mute --selftest tmp/s --start 40 --shots 45,60 --secs 25
  --size 1280x720`, then read the PNGs. Menu screens: no file, `--wall --screen setup|options`.
  Looks: `--mood scene,weather,floor,prop`, `--mood-to ...` (sky transition), `--purgatory`.
- None of this tells you how the game feels or sounds. Say so when reporting.
