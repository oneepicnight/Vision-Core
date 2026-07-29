# 0003: Immutable Release Identity

- Status: Accepted
- Scope: Repository governance and releases

## Context

Branches are mutable and local names can be misleading. Users and maintainers need a stable way to identify the exact source and history represented by a release. Moving a public tag destroys that audit trail.

## Decision

Identify each public Vision-Core release with an annotated, immutable tag pointing to the exact approved commit. Align package/version metadata and release notes with that identity. Never move or recreate a published release tag.

Preserve historical tags and any governance-required archival branches. Branch retirement is independently authorized and is not implied by release publication.

## Consequences

- Corrections receive a new version and tag.
- Release approval names a full commit and exact tag.
- Remote peeled-tag verification is part of publication.
- Repository topology and clean-clone behavior are release evidence.

## Evidence

`vision-core-consensus-v1.0.4` identifies `b874d73cbdf60657334b62c867ed7f18b80a186b`; earlier public tags remain historical identities. See [RELEASE_PROCESS.md](../RELEASE_PROCESS.md).
