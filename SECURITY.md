# Security Policy

`Rivulet Gateway / 溪流网关` is infrastructure software, so security handling must be conservative and explicit.

## Reporting

Please do not disclose unpatched vulnerabilities through public issues.

When reporting a security issue, include:

- affected version or commit
- vulnerability class
- reproduction steps or proof of concept
- impact scope
- whether the issue is configuration-dependent

If a private reporting channel is not yet published for the public project, maintainers should bootstrap one before broad release.
Until then, security handling should remain restricted to trusted maintainers and deployment operators.

## Response Goals

Current target service levels for maintainers:

- initial acknowledgement: within 3 business days
- reproduction decision: within 7 business days
- fix or mitigation plan: as soon as severity is understood

These are goals, not guarantees, but they set the expected operating standard.

## Severity Guidance

High-priority examples:

- request smuggling or framing bypass
- upstream response boundary confusion
- auth or policy bypass once such features exist
- unsafe packaging or release artifact tampering
- denial-of-service conditions that are easy to trigger remotely

## Disclosure

Preferred sequence:

1. private report
2. maintainer reproduction
3. patch and release preparation
4. coordinated disclosure with mitigation notes

## Current Security Notes

At the current kernel stage:

- HTTP/1.1 only
- no TLS termination yet
- no chunked transfer support
- conservative response-boundary enforcement is in place, but still early-stage
- packaging and checksum verification are present, but release signing is not yet implemented
