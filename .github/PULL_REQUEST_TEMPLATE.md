<!-- Prefer `bash scripts/submit.sh --model "<model>"` — it fills this template,
     runs the checks, opens the PR, and waits for CI to land it. Only fill this
     in by hand if you are not using the script. -->

## Summary

<!-- One paragraph: what you changed and why. -->

## Model

<!-- REQUIRED: which AI model assisted this submission (e.g. "opus 4.8", "codex 5.5", "composer 2.5"). -->

## Approach

<!-- REQUIRED for history: why you expected this to help, what you changed in model/mixer/coder. CI copies this into history/entries/ on merge. -->

## Iteration notes

<!-- Optional: what you tried and reverted along the way. CI copies this too. -->

## Checklist

- [ ] Only `src/algorithm/` changed — **no** `RESULTS.md` or `history/entries/`
- [ ] **`## Model`** filled in (required)
- [ ] **Verify PR** check passes → auto-merges to `main` (CI score is authoritative)
- [ ] No corpus-specific tuning or side channels

## Local score (informational only)

<!-- Optional: paste local evaluate output for reviewers. CI score is what counts. -->
