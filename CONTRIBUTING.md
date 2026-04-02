# Contributing

Thanks for helping improve `Rivulet Gateway / 溪流网关`.

We are building toward an Apache-grade engineering bar:

- small, reviewable changes
- reproducible tests
- clear release artifacts
- explicit production-risk discussion

## Ground Rules

- Prefer focused pull requests over large mixed refactors.
- Do not add dependencies casually; justify operational and maintenance cost.
- Keep platform portability in mind for Windows, Linux x86_64, and Linux arm64.
- If behavior changes, update tests and docs in the same change.
- If packaging or release behavior changes, update workflow and validation docs too.

## Development Flow

1. Start from a clean branch.
2. Run relevant tests before proposing changes.
3. Add or update tests for new behavior.
4. Update docs for any user-visible, operational, or packaging change.
5. Use clear commit messages with a prefix such as `feat:`, `fix:`, or `docs:`.

## What Maintainers Expect In A PR

- Problem statement
- Scope of change
- Risk or compatibility notes
- Validation performed
- Follow-up work if the change is intentionally incomplete

## Testing Expectations

At minimum, contributors should run the narrowest relevant checks:

- `cargo test --workspace`
- packaging or smoke scripts if release behavior changed
- benchmark scripts only when the change affects performance-sensitive paths

Useful entry points:

- [scripts/package.ps1](C:\Users\brace\Documents\New%20project\scripts\package.ps1)
- [scripts/package.sh](C:\Users\brace\Documents\New%20project\scripts\package.sh)
- [scripts/bench-baseline.sh](C:\Users\brace\Documents\New%20project\scripts\bench-baseline.sh)
- [packaging/SERVER-VALIDATION.md](C:\Users\brace\Documents\New%20project\packaging\SERVER-VALIDATION.md)

## Dependency Policy

We intentionally keep the dependency surface narrow.
When proposing a new crate, explain:

- why in-house implementation is not preferable
- what operational risk it adds
- what update burden it creates
- whether it affects cross-platform packaging or release behavior

## Performance Claims

Do not post benchmark claims without:

- environment details
- command used
- workload shape
- whether access logging was disabled
- whether results were local, CI, or server-hosted

## Security

Do not open public issues for unpatched vulnerabilities.
Follow [SECURITY.md](C:\Users\brace\Documents\New%20project\SECURITY.md).
