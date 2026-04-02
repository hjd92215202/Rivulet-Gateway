# Governance

This project is being developed with an Apache-grade quality target, even before formal foundation processes exist.

## Principles

- repository-owned automation over tribal knowledge
- explicit risk discussion over hidden assumptions
- small, testable increments over speculative rewrites
- portability and packaging as first-class engineering concerns
- truthful status reporting over aspirational claims

## Maintainer Responsibilities

Maintainers are responsible for:

- reviewing code and release changes
- keeping CI/CD and packaging healthy
- preserving a conservative dependency policy
- preventing unsupported production claims from entering docs or release notes
- triaging security and operational incidents

## Decision Style

Default decision rule:

- prefer the simpler, more inspectable path when two designs are close
- do not expand protocol surface faster than the test and validation system can support
- production-readiness claims must be backed by reproducible validation

## Required Project Areas

The project should maintain standards in:

- code review
- testing and load testing
- packaging and release automation
- documentation and upgrade guidance
- security handling
- community conduct and contributor onboarding

## Near-Term Governance Gaps

These still need formalization in future iterations:

- named maintainers
- release manager rotation
- deprecation policy
- support window policy
- public security contact
