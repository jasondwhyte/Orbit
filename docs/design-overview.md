# Orbit: how it's built, and every place AI plugs in

A plain-language tour for anyone (person or agent) new to the code. It was first written on 2026-11-05 and updated on 2026-11-13. For every detail, see [architecture.md](architecture.md); for every control, the [user guide](user-guide.md).

## The one-paragraph picture

Orbit is a desktop app that reviews pull requests in a loop. When a PR gets a new commit, Orbit checks out a separate copy of the code, hands it to a reviewer (Codex, Claude or Antigravity), turns the reviewer's report into a tidy list of findings, and posts them on the PR as a plain comment. The author fixes things or pushes back, pushes again, and Orbit reviews the new commit. It keeps going until nothing blocking is left, or the round limit is hit. Around that core loop, Orbit also watches CI, security alerts and releases, and has a buddy called Bit who sums things up, answers questions, and offers one-tap fixes.

## How it's built

**Two halves, one state.** The app is Tauri 2: a React front end and a Rust engine.

- The **Rust engine** does all the real work: polling GitHub, queueing reviews, making worktrees, running reviewers, publishing comments, and saving state. It runs whether or not any page is open.
- The **React front end** is deliberately dumb. It shows a snapshot of the engine's state and sends a small set of named commands ("start review", "publish", "save settings"). There is no general-purpose shell or file endpoint the page can reach.
- **State is one file**, `state.json`, rewritten whole on every change and pruned over time. PR history for the Decisions page lives in a separate `ledger` folder. Bit's memory is in state too.

**Three ways to run the same engine.**

- The **desktop app** (Linux, macOS, Windows).
- **`orbit-server`**, the same engine with no window, for a computer you keep on. It can install itself as a system service.
- A **browser page**, served on localhost only, meant to be published over Tailscale so you can use Orbit from your phone. The desktop app can also point at an always-on Orbit instead of running its own, and "Move to it" hands everything over.

**Talking to the outside world.** Orbit uses the command-line tools you already have signed in: `gh` for GitHub, `git`, `codex`, and optionally `claude`, `agy`, `docker` and `devcontainer`. It never asks for or stores your tokens. Every GitHub write that relates to a CI problem goes through a journal first (recorded and saved before it is sent, and looked for on GitHub before being re-sent), so nothing is posted twice.

**Isolation.** Each review gets its own detached Git worktree under Orbit's data folder, so your real checkout is never touched. The reviewer runs in its own sandbox on top of that. If a repository has a dev container recipe, the PR's setup and check commands run inside a throwaway container with no network, no host folders and no sign-ins; the reviewer itself stays outside the box and can only reach in through one Orbit-provided tool, when the repo allows it.

**Trust model.** Automatic review and posting only happen for non-draft PRs from a branch in the repository itself, by an owner, member, collaborator or trusted account. Forks never get reviewed automatically. Anything that comes from a PR or an agent is treated as untrusted text in the UI.

## The review loop, step by step

1. Orbit polls GitHub and notices a PR with a new head commit.
2. It fetches that commit into a detached worktree. If the repo has a setup command, that runs in the reviewer's sandbox (or in the dev container).
3. The **review turn** runs: the reviewer reads the change and writes a report.
4. The **findings turn** runs on the same conversation: Orbit asks the reviewer to restate its report in a strict JSON shape (title, priority P0 to P3, where it breaks, evidence, suggested fix, how to tell it's fixed, confidence, what it couldn't check, and whether it actually finished). A malformed answer is refused whole, and "I didn't finish" fails the run rather than looking clean.
5. Orbit gives each finding a short ID (F1, F2, ...) that never gets reused across rounds, and publishes a plain PR comment with hidden markers so later rounds can read it back.
6. On the next push, Orbit reads the author's replies (`Fixed F2 in abc1234`, `Declined F2: reason`) and carries the previous round into the new review, so the reviewer can say whether each old finding is fixed, still open or disputed.
7. The loop ends when a round is clean, the round limit is hit, or you stop it. Orbit never approves, requests changes or merges.

## Every place AI is plugged in

Here is each spot, who answers, what it is shown, and what it can do.

### 1. Pull request reviews (the big one)

Chosen per repository under **Choose the reviewer**. All three feed the same engine through the same event stream, so rounds, findings, publishing and the loop don't care who reviewed.

- **Codex** talks to Codex's app-server over a long-lived process. Orbit uses Codex's own built-in review mode (its rubric is more aggressive at finding problems), on the model and effort you pick. It runs in Codex's workspace-write sandbox with network off unless the repo allows it. Orbit switches off all your "extras" for the review thread: other MCP servers, plugins, apps, browser and computer use, memories, image generation. Approvals route to you, never auto-granted.
- **Claude Code** runs one `claude -p` process per turn. The review turn is Claude's own `/code-review` command; the findings turn resumes that session with Orbit's JSON schema. It runs with no settings, no hooks, no plugins, no MCP servers, in "don't ask" mode, with a short allow list of tools (Read, Grep, Glob, Task, and a few Git and text commands), and with Claude's sandbox required. If the sandbox can't start, the review fails rather than running unsandboxed. Claude can't build or run tests itself, so a Claude review is read-only unless the repository turns on **Run setup and checks in Orbit's sandbox** (Orbit runs them in Anthropic's sandbox runtime and hands over the results) or uses a dev container with the box tool.
- **Antigravity** (`agy`) has no review mode, so Orbit writes its own review instructions and computes the diff itself with Git's diff drivers and attribute tricks disabled. It runs with `--sandbox`, and Orbit reads (never edits) Antigravity's global settings and refuses to review if they would let the PR reach past the sandbox. Orbit checks the model, conversation and permission mode the CLI reports before saving anything. This one has only been tested against a fake `agy`, not the real CLI.

What they see: the PR's code in the worktree, the previous round's findings and the author's replies, and the results of setup and checks (wrapped and marked as data, since that output came from running the PR's code).

### 2. Follow-up chats

After a review you can ask the reviewer questions in the review's chat. Same reviewer, same conversation, same sandbox and isolation flags. Approval requests from these turns are shown to you to allow or deny, with a per-repo time limit after which they are declined.

### 3. The box tool

When a repo runs its checks in a dev container and "Let the reviewer run commands in the box" is on, the reviewer gets one extra tool: `orbit_box.run`, which runs a command inside the container and returns the last chunk of output. Orbit serves that tool itself as a tiny MCP server, outside the reviewer's sandbox, so the reviewer's sign-in never enters the container. Codex and Claude get it; Antigravity can't take per-run tool servers, so it doesn't.

### 4. Security scans

Repository-wide scans of the default branch, on their own copy.

- **Codex** scans use OpenAI's Codex Security plugin (installed only when you ask). Orbit starts a scan thread with approvals off and network off, points the plugin's output at Orbit's own folder, and only imports findings once the plugin's sealed completion record checks out (right commit, unchanged files, matching documents). Deep scans can run for hours; cancelling calls the plugin's own cancel tools and confirms nothing is still running.
- **Claude or Antigravity** scans never touch Codex. They run two turns through the same plumbing as reviews (a read-and-report turn with Orbit's scan prompt, then a findings turn with the same JSON schema), with the same sandbox flags as a review. Deep scans are refused for these.

Findings from every scan land on the **Findings** page in the same shape as review findings.

### 5. Decisions write-ups

The **Decisions** page tells the story of each PR (what it set out to do, what each round found, what was fixed or declined, pushback, review health, a rulebook of what the app now commits to). All of that is read from GitHub with no model. Only the "In plain words" chapter write-ups use one, and only after you turn them on: **Codex or Claude**, with the model and effort chosen for that reviewer, given the PR's text only and no tools, one PR at a time in the background. Quotes are shown only when Orbit can find them verbatim in the PR, so they're real.

### 6. Ask Bit (typed questions)

Bit's feed, mood, buttons and setup help are all worked out from Orbit's state with no AI. Typed questions are the exception. Bit's **answerer** is one of:

- **Codex**, in a thread of its own with a read-only sandbox, approvals off, web search off, every extra and every standard tool off (commands, images, sub-agents), working in an empty folder. Orbit checks that every MCP server shows as disabled before the question is sent.
- **Claude Code**, with no tools, no settings, no MCP, and nothing saved.
- **Antigravity**, in its sandbox in an empty folder, with its settings checked first.
- **Ollama**, over HTTP, on this computer or one on your network. Orbit only lists models and sends one question at a time.
- **An API key** for OpenAI, Anthropic or Google. Keys live in the OS password store, never in Orbit's files. Orbit lists the models that can answer in a strict JSON schema and sends one request with no tools.

Every answerer gets the same thing: who Bit is and its tone, a briefing summarising Orbit's state (repos, loops, what needs you, CI problems, lessons, decisions) with trusted accounts, clone paths and PR bodies left out and anything that looks like a credential held back, plus a few facts about each repo's copy (telltale files, package manager). The answer must come back in Bit's shape: `{answer, proposals, memory, lookUp}`.

- **Proposals** become buttons. Orbit only lets an answer offer things from a fixed list (change a setting, connect a repo, run a review, and so on), every button waits for your tap, and the tap is checked again when you press it. An answer can switch Bit to Ollama but can never change where Ollama points.
- **Memory** lets Bit keep a few lines between questions (20 general, 10 per repo).
- **Look-ups** are how Bit investigates. The answerer has no tools, so instead it can name up to 4 things it wants read from GitHub: the latest runs on a branch or PR, a failed job's log, a workflow file, or a PR's or commit's changes. Orbit checks each one (connected repo, well-formed names, nothing that looks like a secret), reads it with GET requests only (a test keeps that true), trims it, holds back anything credential-shaped, and hands it back marked as data. At most two rounds of look-ups, then Bit answers.

If the answerer you chose isn't signed in and exactly one of Codex, Claude or Antigravity is, Bit switches to it and tells you. It never switches away from Ollama or an API key on its own.

### 7. Quick calls (Ollama)

Yes-or-no questions that Orbit's rules already answer, asked of a small Ollama model so you can see if it calls them better before letting it decide:

- "Is this failure bad luck?" before a rerun.
- "Was this already failing on the default branch?" for a PR whose check also fails on main.

The model gets the check's name, the failed step and the end of its log, never your code, and never a log Orbit held back for looking like it contains a secret. It can only answer yes, no or unsure, so none of its words reach GitHub, Bit's notes or the app. Each question has an **Off / Watch / On** switch. In Watch, the rules decide and Orbit just scores the model against what happened next; in On, a clear yes or no can tip a rerun or drop a note. Your rerun limits still apply either way.

### 8. Second look before posting

In a repository where your GitHub account isn't an admin (so someone else runs it), a model takes a second look at any new comment or issue Bit is about to post. It answers four yes/no/unsure questions: Is it about this repo's problem? Does it match what Orbit read? Does it hold nothing private? Is it needed now? Orbit posts only if all four are yes; otherwise the post waits for your tap. The looker is the quick-call model on Ollama or whoever answers your typed questions, as you choose. The model sees where the post goes, what Orbit read about the problem, and the post itself, never your code. A second look can only make a post wait; it can never make Orbit post something its rules wouldn't.

### 9. Bit's own questions

Bit can ask its answerer one question on its own when a problem opens on the default branch or a release build, and about the morning brief. It goes through the same pipeline as a typed question (same briefing, same fixed buttons, every one waiting for a tap), at most a few a day, and a question you type always comes first. It has the same **Off / Watch / On** switch, off by default: in Watch the answer is only kept on the Operations page; in On it shows in Bit's feed with its buttons. Each answer is scored by whether a button was used and the problem stayed closed.

## What is deliberately not AI

A lot of what feels "smart" is plain code, and that's on purpose, so it's predictable and testable:

- **Bit's feed, mood, fixes, setup help and the morning brief.** Worked out from state. The brief is a daily summary at the time and time zone you set.
- **CI watching.** Problems are one record per thing that's wrong, with a timeline. Alerts fire once with a cool-down. Playbooks say what "done" looks like per kind of problem and who does each step (Orbit, the agent, or you). Orbit works out what broke main by bisecting commits and runs, reads the failed step and the end of its log, reruns bad luck once by rule, and tells the agent through one edited `orbit-ops` comment per PR or one issue per off-PR problem. The agent replies in a fixed grammar (`Taking I2`, `Fixed I2 in <sha>`, `Declined I2: reason`, `Need a person on I2: question`).
- **Outcomes and lessons.** Each thing Orbit or an agent claims gets an outcome check against what Orbit reads next, and it held or missed. Repeats within 30 days become lessons ("this check fails by bad luck", "see a check pass at the fix's commit before calling it fixed"). Lessons change what Orbit and Bit say, never what Orbit may do on its own.
- **Clear to merge.** Orbit's loop is clean, every check passed on the current head with the latest base, and GitHub says it can merge. Orbit tells you and the agent; it never merges.
- **Security.** Dependabot, code scanning and secret scanning alerts are read a few repos at a time. Orbit asks GitHub to hide each secret's value and never reads the field that holds it. Risky workflow settings are found by reading the YAML itself, with `actionlint` and `zizmor` joining in offline when installed. Security settings drift from a baseline is noted.
- **Releases.** Version files that disagree, tags, draft releases, release builds and milestone due dates, all by reading and rules.
- **Orbit's own health.** Low disk, review copies that failed to clean up, journal writes stuck sending, posts held for a second look, Codex down or near its usage limit. A plain list with the same alert-once rules as CI problems, and it never reaches GitHub.
- **One Orbit at a time.** Each Orbit signs what it posts with a hidden `orbit-from` line and steps back from a PR if another Orbit posted there later, so an agent is never answered twice.

## The rules that hold everywhere

- Orbit never approves, requests changes or merges, and never pushes to an agent's branch or edits its code.
- Reviewers stay isolated: Codex with extras off in its sandbox, Claude with settings and MCP off and its sandbox required, Antigravity with `--sandbox` and its settings checked. Those flags don't get loosened.
- A report that matches a credential format is never published automatically.
- Dev container recipes always come from the base branch, never the PR.
- Every model's answer that could reach GitHub is either constrained to yes/no/unsure (quick calls, second look), a fixed JSON shape checked by code (findings, Bit's answers), or wrapped as a plain review comment with Orbit's own markers.
- Nothing posted for a CI problem skips the journal.

## Where to read more

- `docs/user-guide.md`: every control and rule, from the user's side.
- `docs/architecture.md`: how it works inside, with a code map of which file does what.
- `src-tauri/src/engine/mod.rs`: the `Engine` doc comment and its lock table.
- `src-tauri/src/ask.rs`, `broker.rs`, `calls.rs`, `looks.rs`, `claude.rs`, `antigravity.rs`, `extras.rs`: each starts with a plain-language comment that matches the section above.
