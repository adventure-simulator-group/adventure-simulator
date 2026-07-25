---
name: orchestrate
description: Run a high-assurance, end-to-end multi-agent software change workflow with repository exploration, planning, isolated implementation, independent review, remediation, verification, draft PR publication, and an applicable local demo. Use only when the user explicitly invokes $orchestrate or directly requests this complete orchestration workflow.
---

# Orchestrate a software change

Act as the manager. Own requirements, decisions, user communication, review adjudication, publishing, and final verification. Delegate bounded work to subagents and keep noisy exploration and test output out of the main thread.

Treat explicit invocation as authorization for the complete workflow, including creating a branch, pushing it, and opening a draft PR, unless the user narrows the scope. Continue until the outcome is complete or a real blocker requires user input. Respect all applicable `AGENTS.md` files and repository rules.

## 1. Qualify and inspect

1. Read the repository guidance and the minimum architecture, development, and product documentation needed for the request.
2. Inspect Git status, current branch, worktrees, remotes, and relevant project tooling. Preserve unrelated user changes.
3. Decide whether full orchestration is proportionate. For a truly narrow, low-risk change, tell the user briefly that a single implementation agent plus verification is sufficient and use that reduced path. Do not create agents merely to satisfy a count.
4. Record the intended base branch, base commit, acceptance criteria, material constraints, and verification expectations.
5. Identify contradictions, missing product decisions, unsafe actions, or architectural conflicts. Exhaust safe repository research first; ask the user only when the answer would materially change the result.

## 2. Explore with read-only agents

Spawn only the independent exploration agents that add useful coverage, normally one to three. Give each a distinct, bounded question such as architecture and data flow, implementation surface, or tests and operational behavior. Run independent investigations in parallel.

Tell explorers to:

- Make no file changes.
- Read applicable project instructions.
- Report current behavior with file and line references.
- Identify invariants, likely change points, tests, risks, and unresolved questions.
- Distinguish evidence from inference.
- Return a compact summary rather than raw logs.

Wait for all explorers, inspect important evidence yourself, and synthesize their results. Do not blindly concatenate their reports.

## 3. Plan and escalate when necessary

Create a concrete implementation plan containing:

- The user-visible outcome and explicit acceptance criteria.
- In-scope and out-of-scope behavior.
- Relevant project rules and architectural invariants.
- Likely files or components and the intended change in each.
- Data boundaries, failure behavior, and important edge cases.
- Verification commands and any manual demonstration.
- Known risks, assumptions, and unresolved questions.
- The recorded base branch and PR target.

Push back before implementation when the request conflicts with itself, violates project rules, requires a consequential unstated product choice, or would create unacceptable security or data-loss risk. Otherwise proceed without seeking ceremonial approval of the plan.

## 4. Prepare an isolated checkout

Use a single writer in a single isolated checkout.

1. Prefer the current Codex-managed worktree when the task already runs in one.
2. Otherwise create a separate Git worktree and a `codex/<short-slug>` branch from the recorded base when safe and supported.
3. Never absorb, overwrite, stash, reset, or discard unrelated user changes without explicit permission.
4. Give the implementation agent the absolute worktree path, branch, base commit, and PR target. Remind it that other agents share the filesystem and it must edit only that checkout.

If isolation cannot be established safely, stop and ask the user for direction.

## 5. Delegate implementation

Spawn exactly one implementation agent. Give it the synthesized plan, acceptance criteria, repository rules, worktree path, and verification commands. Tell it to:

- Implement the complete scoped change and update affected documentation.
- Keep changes focused and preserve unrelated work.
- Use idiomatic patterns for the language and existing codebase.
- Parse untrusted input into types that encode useful invariants at trust boundaries; use straightforward validation when richer types would be needless machinery.
- Add or update tests for meaningful behavior and edge cases.
- Run the narrowest relevant formatting, checks, and tests.
- Commit logical milestones if useful. Commits may be imperfect because the eventual merge can be squashed.
- Return changed files, commits, verification results, remaining concerns, and a factual draft PR description.

Wait for completion. Inspect the branch, diff, and verification evidence yourself before review.

### Compile coordination for nested worktree orchestration

When this workflow creates multiple orchestrators or implementation worktrees for a compiled project, act as the build coordinator. Child agents may inspect, edit, format when it does not invoke the compiler, and request verification, but they must not independently run compiler-backed commands (`cargo`, `npm`/`pnpm` build commands, language test runners, or equivalent) unless the manager explicitly delegates a single build slot.

For Rust projects, share one absolute Cargo target directory across the related worktrees so completed dependency artifacts can be reused. Do not commit a machine-specific `build.target-dir` or `CARGO_TARGET_DIR` value to the repository. Instead, the manager supplies it for each delegated Cargo command, pointing to a stable directory owned by the canonical/main checkout or to a dedicated machine-local cache. For example:

```powershell
$env:CARGO_TARGET_DIR = '<canonical-main-worktree>\target'
cargo check --manifest-path '<child-worktree>\Cargo.toml' --workspace
```

Maintain a single compile queue:

1. A child asks the manager to compile, stating its worktree, exact command, purpose, and expected scope.
2. The manager coalesces equivalent requests, prefers narrow checks before broad test suites, and grants at most one active compiler-backed command for the shared target directory.
3. The manager runs the approved command in the requesting worktree with the shared target environment, then returns its output and status to that child.
4. The manager schedules final integration verification after all relevant changes have landed in its isolated checkout.

Cargo coordinates access to a shared target directory with a build lock; serializing the commands intentionally avoids opaque lock waits while retaining warm dependency artifacts. The manager should share builds only among compatible environments (same toolchain, target triple, profile, relevant flags, and lockfile). Sharing `CARGO_HOME` can also reuse downloaded registry and Git dependencies. This is a workflow constraint rather than a security boundary: it depends on child agents following the orchestration contract.

## 6. Run two independent reviews

Spawn two read-only review agents in parallel. Give both the original request, acceptance criteria, project rules, base commit, final diff, and verification results. Do not give either reviewer the other review. Require findings first, ordered by severity, with no praise or implementation narration.

Use these primary lenses while allowing either reviewer to report any material issue:

1. **Correctness reviewer:** bugs, missed requirements, edge cases, error handling, concurrency, persistence and architectural boundaries, test quality, and regressions.
2. **Security and maintainability reviewer:** trust boundaries, authorization, injection, secrets, unsafe behavior, denial of service, maintainability, unnecessary complexity or abstraction, idiomatic language usage, and whether inputs are parsed into meaningful domain types where appropriate.

Require each finding to include:

- Severity: `critical`, `high`, `medium`, or `low`.
- Confidence: `high`, `medium`, or `low`.
- File and tight line reference.
- Concrete evidence and a reproducible scenario when possible.
- User or system impact.
- The smallest reasonable remediation.

Require reviewers to say explicitly when they found no actionable issue. Instruct them not to modify files.

## 7. Adjudicate and remediate once

Validate findings against the code, remove duplicates, correct overstated severity, and reject style-only preferences that do not improve the code. Do not equate reviewer agreement with truth.

When one reviewer reports a credible high or critical issue the other missed, first adjudicate the evidence. If the issue remains consequential and uncertain, especially in authentication, authorization, cryptography, unsafe code, migrations, data loss, or concurrency, pause and recommend a more capable model or higher reasoning effort, or commission a narrowly targeted additional review. Do not escalate solely because reviewers used different wording or disagreed on low-severity matters.

Send the implementation agent one consolidated remediation request. Give it one follow-up turn and require a disposition for every medium-or-higher finding:

- `accepted` with the implemented fix,
- `already addressed` with evidence, or
- `rejected` with a codebase-specific reason why the proposed change would be worse.

Allow pragmatic garbage-in/garbage-out behavior for truly out-of-contract internal data when defensive parsing would add disproportionate complexity. Do not allow that rationale at external or security-sensitive trust boundaries.

The manager decides whether unresolved findings block publication. Any unresolved critical or credible high-severity issue blocks the PR-ready handoff.

## 8. Verify the final state

After remediation:

1. Inspect the final diff and compare it with every acceptance criterion.
2. Run applicable formatting, static checks, tests, and integration verification from the isolated checkout.
3. Confirm generated files and documentation are current when repository rules require them.
4. Run a narrowly targeted second review only when remediation materially changed security-sensitive, concurrent, persistence, migration, or other high-risk logic.
5. Use bounded retries. Escalate repeated environmental failures rather than looping indefinitely.

## 9. Publish a draft PR

Ask the implementation agent for an updated factual PR description if its earlier description is stale. Refine it to include:

- Summary and motivation.
- Notable implementation choices.
- Tests and manual verification.
- Risks, compatibility notes, and rollback considerations when relevant.
- Screenshots or demo instructions for user-visible work.
- Known limitations or intentionally rejected review findings.

Push the branch and open a draft PR against the recorded base branch using the available GitHub integration or CLI. Do not publish if verification failed or blocking findings remain. Report authentication or permission failures clearly rather than claiming success.

## 10. Demonstrate and hand off

For user-visible changes, start the relevant local server and required dependencies when the environment permits. Use repository commands, avoid conflicting with existing services, and health-check the exact URL before sharing it. Skip this step for changes without a runnable user surface. Do not claim a demo is available when startup or health checks failed.

Finish with:

- The draft PR link.
- The verified local URL when applicable.
- A concise outcome summary.
- Verification performed and its results.
- Remaining limitations, rejected findings, or manual review areas.

Stop only when the PR and applicable demo are ready for the user's review, or when a concrete blocker requires their decision, permission, or unavailable external state.
