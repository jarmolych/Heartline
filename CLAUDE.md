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
  2. Commit it with a clear message.
  3. Write a short handoff: what's done, what's next, and exact file and function names. Put it in
     the relevant plan in `docs/plans/` under a "Progress" heading.
  4. Stop and tell the user.
- **Split big steps** into pieces that each end in a passing build, such as "frame store + tests",
  then "split analyze", so there's always a clean place to stop.
- **Effort:** use medium or high effort for normal coding. Fable and max effort are for planning
  and reviews (the `planner` agent), not long implementation runs.
- **Publishing:** whenever work stops at a clean point, commit and push to `origin` (the user
  asked for this on 2026-11-14). Keep wrap-up testing light: build + unit tests.
- **Overriding the rule:** if the user says to ignore the pacing rule, keep going.

## Project basics
- Rust is not on PATH: `"$USERPROFILE/.cargo/bin/cargo.exe"`.
- Keep CRLF line endings, use pure-Rust dependencies, and keep level generation deterministic.
- Specialist agents live in `.claude/agents/`. Plans live in `docs/plans/`.
