# Maintainer notes

What the people and agents who maintain Orbit need to know that the code and the other docs don't say: how work flows, the big decisions and why they were made, what is still open, and the gotchas that cost time. Agents working on Orbit read [CLAUDE.md](../CLAUDE.md) first; this page is the longer background.

## How work flows

The user maintains Orbit with a coding agent (Claude Code, mostly in cloud sessions). The agent designs and builds; the user decides direction and anything that can't be undone.

### The review loop on Orbit's own pull requests

Until Orbit takes the job over itself (see [Where Orbit is going](#where-orbit-is-going)), an outside review bot called Milo reviews every pull request that is marked ready and posts its findings as PR comments.

1. Open the pull request, mark it ready for review when the work is done, and watch its comments.
2. Blocking findings: fix them and push.
3. Non-blocking findings: decide whether to make each one, and say what you chose and why in a PR comment. Milo is a second opinion; the agent still makes the call.
4. Each push starts a new review, so the cycle repeats.
5. Merge once CI is green and Milo says **CLEAR TO MERGE** or **CODE REVIEW PASSED**. Both clear it: CODE REVIEW PASSED means the code checks out but Milo didn't run the tests itself, and it isn't always edited to CLEAR TO MERGE afterwards, so don't wait for that.
6. A **BLOCKED** review means the review couldn't finish or a required step failed. Read its exact reasons and act on them: fix failing tests or a merge conflict yourself, and for a setup or access problem tell the user once what they need to do (for example, Milo's test machine is offline, or its harness points at an old address). Never merge while blocked; wait for the next review of the same head.
7. Once cleared, the agent merges (squash) and moves on.

Before merging, merge the latest `master` into the branch and run the checks again. GitHub's "can merge" and a review of the branch alone don't catch two pull requests that each pass but break together (two PRs once each grew the same fixed-size list, and `master` stopped compiling).

### Rules the user set

- **Orbit is public.** In anything that reaches the repository or GitHub (code, comments, docs, UI text, commits, PR titles and bodies, issues), say "you" or "the user", never the maintainer's name. Leave out private machine names, home folder paths and links to private tools. When porting code from another of the user's projects, describe it on Orbit's own terms, with no names from the project it came from.
- **Never add a push guard.** No required reviews or required checks the user didn't ask for, in Orbit or in Orbit's own repository. Pushes should flow freely.
- **Changes come from issue reports.** Outside pull requests aren't taken (see [CONTRIBUTING.md](../CONTRIBUTING.md)).
- **Big work goes in stages.** An epic issue with one sub-issue or part per pull request. Each stage starts on the user's go.
- **Decisions that change the goal are the user's.** Offer a recommendation and keep building on it unless the step can't be undone.

### What a cloud session can't do

- Push a tag, delete a branch, or force-push. The user tags releases (`git checkout master && git pull && git tag vX.Y.Z && git push origin vX.Y.Z`; always pull first, as a tag once landed on a stale commit). Branches are deleted on merge by a repository setting. After a merge, reset the working branch to `master` and push normally.
- Reach the repository's old address (`analogproblems/orbit`); use `jasondwhyte/Orbit`.
- Run `npm run check` without the Tauri libraries: `apt-get install libwebkit2gtk-4.1-dev libgtk-3-dev librsvg2-dev libayatana-appindicator3-dev`.

### Writing pull requests

- Avoid `#N` examples in PR bodies and commit messages: GitHub links them to real issues. Use code spans.
- A pull request sometimes stays on an old head after a push (no CI, no new review). Check its head after each push; updating the PR's base to the same `master` makes it pick the push up. No empty commit needed.
- Asking the user to review their own pull request fails ("cannot be requested from pull request author"); assigning them works.

## Where Orbit is going

- **Orbit replaces the outside reviewer.** Orbit's review loop was built to mirror Milo, an always-on reviewing agent, and to go beyond it: Milo only runs the review loop, while Orbit's buddy Bit also manages CI, security, releases and the bigger picture. Once Orbit does the job, Milo goes away. So never design Orbit around Milo as a lasting outside reviewer, and prefer paths that don't depend on one provider.
- **Bit is a DevOps manager, not a doer.** It watches reviews, CI, security and releases across repositories and organizations, works with each project's own agent (the project agent stays the authority), and talks to that agent through GitHub: one edited `orbit-ops` comment per pull request and one `orbit` issue per problem off a pull request, with the reply grammar in the answer-orbit-reviews skill. What Bit may do:
  - **Do it:** its own comments and issues, and one rerun of a check that failed by bad luck.
  - **Ask first:** a second rerun, anything in other people's repositories, repository or organization settings, pushing a tag, publishing a draft release.
  - **Never:** merge or approve, push to an agent's branch, add push guards unasked, change trusted accounts, show secrets.
- **Models advise, rules act.** Everything Bit does on its own is plain rules. Model calls that could change behaviour (quick calls, second look, Bit's own questions) start in Watch, are scored against what happened, and only act once the user switches them On. Lessons change what Bit says and suggests, never loosen what it may do. Letting a well-scored kind of proposal run without a tap is a later call for the user.
- **Always-on.** The user chose "a computer you keep on" (`orbit-server` as a service) over a hosted container, used from a phone or laptop over Tailscale.

## Decisions and why

- **Reviews are provider-agnostic** and run on the model Orbit picks. For Codex, Orbit uses Codex's own review mode because its rubric is more aggressive at finding problems; it skips pre-existing bugs, which is accepted.
- **Reviewer flags never loosen.** When Claude reviews proved read-only, the fix was for Orbit to run setup and checks itself in a sandbox, not to widen Claude's allow list.
- **Codex extras are off** (other MCP servers, plugins, apps, memories) for every Codex thread Orbit starts, and must be sent again on resume.
- **The reviewer stays outside the dev container.** It reaches in only through Orbit's "run this in the box" tool (per repository, off by default), so no sign-in is ever copied into a container.
- **Bit writes its own memory** (20 general lines, 10 per repository) without asking, with a delete button per line. Bit may also set up a repository, always behind a tap that shows the exact command.
- **Bit's looks**: four moods map onto four of the orb looks' six states (happy is idle, busy is working, worried is awaiting input, sleepy is paused); "thinking" while Bit answers is a possible follow-up.
- **Decisions page**: built from a separate tool that was folded into Orbit. An integration that needed an API key Orbit would have to hold was left out.
- **Claude Code mods** were dropped: they can't draw in cloud sessions, where the maintainer works.
- **Licence** is Apache 2.0.
- **Release builds** are unsigned (macOS ad-hoc signed, not notarized; Windows unsigned). Signing needs paid certificates; the user's call later.
- **Avoid file names that differ only by case**: they broke the macOS and Windows builds once.

## Still open

- Always-on Orbit has not been tried on a real always-on computer, and push alerts to a closed page have not been tried on a real phone.
- Antigravity reviews have only run against the fake `agy`, never the real CLI; treat them as experimental. Its restrictions are documented, not changed.
- Code signing for macOS and Windows.
- The phone layout: repository setup might become desktop-only.
- Flaky tests: `remote::tests::a_request_still_in_flight_does_not_hold_up_settings` (connection reset in cloud containers). CI has also failed now and then in `engine::cli_scans_tests`, `engine::checks_tests` and `claude::tests` on timing; not looked into yet.

## Testing and tooling gotchas

- `App.test.tsx` sets a 15-second test timeout because long Settings flows ran past 5 seconds on slower CI runners. Keep long app flows in separate tests.
- Astryx Typeahead writes a picked choice's label into its input one frame later. Wait for `input.value` to equal the label before clearing or typing.
- Astryx NumberInput renders an empty `role=alert`; App tests use a `banner()` helper.
- The fake `gh` (`src-tauri/src/gh_fixture.cjs`) answers from files in its folder: `job-<id>.json` and `log-<id>.txt` for jobs (anything else is 404; an unknown endpoint answers `[]`), `history-<oid>.json`, `setup-<oid>.json`, runner files, `queued.json`, and `hold` to pause one read. Engine tests share one check link unless given distinct ones with the `links` helper in `engine/ops_tests.rs`.
- `remote_tests.rs` hands each test its own port below the ephemeral range, but a CI runner once gave one away to another connection anyway. Tests that turn browser access on use `turn_on`, which moves to another port when that happens, and wait with `until_closed` after turning it off, since stopping doesn't wait for the listener to close.
- `src-tauri/target/debug/incremental` grows to about 13 GB and can fill a cloud disk; deleting it is safe.
- Never `pkill -f` a pattern in the same shell command that contains it: it kills its own shell. Kill by process id.
- The cloud clone is shallow; `git fetch --unshallow` before comparing releases.
- Draft releases answer 404 by tag; list releases instead.

## README screenshots

The pictures in `docs/screenshots/` are taken from the browser demo with [`scripts/readme-screenshots.cjs`](../scripts/readme-screenshots.cjs). Start `npm run dev`, then run `node scripts/readme-screenshots.cjs <out-dir> 1.5` with Playwright available. Desktop shots are 1440×900 at 1.5×, the phone shot 390×844 at 2×. Shrink each to 256 colours (for example Pillow's `quantize(256, MEDIANCUT)`) before committing.

## Setting up a new Claude project for Orbit

If the maintainer's Claude project moves to a new account, the project's memory, files and instructions stay behind. Everything above is what that memory held. In the new project:

1. Add this repository.
2. Paste the project instructions below.
3. Connect GitHub for the new account and make sure the Claude GitHub app can reach the repository.

Project instructions:

```markdown
# Review workflow (applies to every thread)

- You (Claude) own the design and implementation of code.
- Milo reviews every PR you mark ready for review and posts findings as comments. Treat it as a second perspective; you still make the choices.
- After opening a PR, mark it ready for review when it is done, and watch its comments (subscribe to PR activity).
- Blocking findings: act on them and push the fix.
- Non-blocking findings: decide whether to implement each one, and note the choice and the reason in a PR comment.
- Each push restarts Milo's review, so the review/fix cycle repeats until Milo posts CLEAR TO MERGE or CODE REVIEW PASSED.
- Once CI is green and Milo says either one, merge the PR yourself and move on to the next step. Never merge on BLOCKED.
- Never add a push guard: pushes should flow freely.
- Read CLAUDE.md and docs/maintainer-notes.md first.
```
