# Release Process

## Purpose

This document defines how Vision-Core turns a validated revision into an identifiable, auditable public release. A release is a repository-governance event as well as a software build.

## Release Invariants

- The release commit is immutable.
- Public release tags are never moved or recreated.
- The source tree, version metadata, documentation, and release notes agree on identity.
- Consensus and protocol compatibility claims are explicit.
- Validation evidence names the exact commit.
- Promotion uses ordinary Git history. History is not rewritten to manufacture a release.
- Archival branches and historical tags are preserved.

## Roles

The repository owner authorizes release creation. Engineering produces the candidate and validation evidence. The release administrator verifies repository topology, creates the approved annotated tag, pushes it, and publishes the approved notes.

One person may hold multiple roles, but the evidence and authorization remain separate artifacts.

## Release Lifecycle

```text
scoped development
  -> reviewed integration branch
  -> frozen release candidate
  -> validation and repository audit
  -> owner approval
  -> authoritative-branch promotion
  -> annotated tag
  -> remote publication
  -> post-release validation
  -> branch retirement under separate authorization
```

No stage implicitly authorizes the next one.

## Development and Integration

1. Start from the verified authoritative base.
2. Implement one engineering concern per commit.
3. Keep consensus, protocol, persistence, formatting, dependencies, and
   documentation isolated.
4. Validate each tranche according to [TESTING_POLICY.md](TESTING_POLICY.md).
5. Assemble approved commits on a named integration branch without rewriting
   published history.
6. Review the complete diff and ancestry from the prior authoritative state.

An integration branch is a candidate assembly area, not a release identity.

## Candidate Preparation

1. Freeze the candidate scope.
2. Confirm all intended changes are committed.
3. Confirm no unauthorized source, dependency, CI, or generated-file changes are present.
4. Confirm version and release identity.
5. Classify consensus, protocol, persistence, API, and operator impact.
6. Prepare release notes from the actual tagged diff.

Do not add opportunistic cleanup after validation begins. A candidate change invalidates evidence collected against the prior commit.

## Validation

Run the release-candidate row and every applicable higher-risk row in
[TESTING_POLICY.md](TESTING_POLICY.md) against the exact candidate revision.
The Testing Policy is the single authority for test commands and minimum
validation gates. Release governance additionally requires a clean worktree and
the repository audit below.

Record exact test counts. Do not infer a new release’s counts from an earlier run.

## Repository Audit

Before promotion, verify:

- candidate commit and tree identity;
- expected ancestry from the current authoritative branch;
- intended diff from the prior release tag;
- all historical release tags still resolve to their original objects;
- remote branch and tag state;
- absence of unexpected local-only commits or files;
- clean-clone checkout and build behavior where required.

If `main` must be advanced, use the approved normal merge or fast-forward strategy. Preserve the previous public state with an explicitly authorized archival ref when governance requires it.

## Candidate Freeze

Once validation begins:

- identify the candidate by full commit hash;
- record its tree hash;
- stop accepting unrelated commits;
- freeze release notes to the candidate diff;
- rerun invalidated validation after every candidate change.

Even a documentation correction after freeze creates a new candidate commit
and requires applicable evidence to be refreshed.

## Release Approval Package

The approval package should state:

- release name and semantic version;
- exact candidate commit;
- prior release tag;
- source delta;
- consensus and protocol impact;
- validation commands and results;
- repository-integrity findings;
- known limitations;
- exact actions authorized.

Approval to prepare a candidate is not approval to tag or publish it. Tag creation and remote publication must be explicitly authorized.

## Authoritative-Branch Promotion

Immediately before promotion:

1. fetch and inspect current remote refs;
2. verify that the approved candidate has not changed;
3. verify expected ancestry and the approved promotion method;
4. preserve the prior authoritative state when an archival ref is required;
5. advance the authoritative branch using the approved normal fast-forward or
   merge;
6. verify the local and remote branch commit.

Force push, history rewrite, branch deletion, and archival-ref deletion require
separate explicit owner authorization.

## Tag Creation

Vision-Core releases use annotated tags. The authorized tag name and commit must be checked immediately before creation.

Conceptually:

```powershell
git tag -a <approved-tag> <approved-commit> -m "<approved release annotation>"
git show --no-patch --decorate <approved-tag>
```

Never tag an implicit `HEAD` when the approval names a full commit. Verify the peeled tag target and annotation before pushing.

## Publication

Push only the approved tag:

```powershell
git push origin refs/tags/<approved-tag>
```

Then verify the remote tag and peeled commit. Publish release notes using the approved documentation package. Do not silently edit compatibility claims during publication.

## Post-Publication Verification

After publication:

- verify the remote annotated tag object and peeled commit;
- verify historical tags remain unchanged;
- verify the authoritative branch still points to the expected commit;
- verify the archival branch remains present when required;
- verify release notes name the correct tag and commit;
- record the final repository topology.

Branch retirement is a separate governance action. A release authorization does not authorize branch deletion.

## Post-Release Validation

1. Resolve the remote annotated tag and peeled commit.
2. Compare them with the approved candidate.
3. Verify the authoritative remote branch.
4. Verify historical tags and required archival refs.
5. Check out or clone the tag into a clean workspace.
6. Run the approved smoke, build, or release validation.
7. Verify version output and published release notes.
8. Record final topology, results, and external publication identifiers.

If clean-clone behavior differs from the candidate workspace, stop and classify
the release as suspect.

## Release Identity

The canonical release identity is carried by:

- the immutable annotated tag;
- the tagged commit;
- package/version metadata;
- release notes;
- repository history.

A branch name is not a release identity. A mutable hosting-platform release page is not sufficient by itself.

## Hotfixes and Corrections

Never move a published tag. If a published release is incorrect:

1. document the defect;
2. prepare a new candidate;
3. choose a new version and tag;
4. repeat validation and approval;
5. publish a correction that references the superseded release.

## Failed Candidate and Rollback

### Before authoritative promotion

1. Preserve the failing commit and evidence.
2. Record the failed command and cause.
3. Do not tag the candidate.
4. Correct the issue in a new isolated commit or abandon the candidate.
5. Create a new candidate identity.
6. Rerun all invalidated validation.

Do not rewrite the failed candidate merely to make the evidence disappear.

### After branch promotion but before tag publication

Stop immediately and do not create the tag. Preserve both promoted and prior
commits. The repository owner must choose an auditable forward correction or an
explicitly authorized ref restoration based on whether the branch has been
consumed externally.

The exact recovery action is **Owner Decision Required**.

### After tag publication

Never move or delete the tag to conceal the release.

1. Publish a clear advisory when users may be affected.
2. Preserve the release and validation evidence.
3. Assess consensus, protocol, persistence, and security impact.
4. Prepare a new corrective version and tag.
5. Repeat the complete release lifecycle.
6. Document operator stop, upgrade, restore, or migration requirements.

Emergency network or database rollback semantics are **Owner Decision
Required** unless an accepted incident plan governs the event.

## Release Checklist

### Candidate

- [ ] Scope frozen and fully committed
- [ ] Full candidate commit and tree recorded
- [ ] Prior release and authoritative branch recorded
- [ ] Version identity aligned
- [ ] Release notes derived from the actual diff
- [ ] Consensus, protocol, persistence, API, and operator impact classified

### Validation

- [ ] Release-candidate row and all applicable higher-risk rows in
      `TESTING_POLICY.md` passed with exact evidence
- [ ] Repository and clean-clone audit passed

### Authorization and publication

- [ ] Owner approval names the exact commit and tag
- [ ] Authoritative branch promotion explicitly authorized
- [ ] Annotated tag creation explicitly authorized
- [ ] Remote tag push explicitly authorized
- [ ] Release notes approved

### Verification

- [ ] Remote branch verified
- [ ] Remote annotated tag and peeled commit verified
- [ ] Historical tags unchanged
- [ ] Required archival refs preserved
- [ ] Clean tagged checkout validated
- [ ] Final topology and evidence recorded

## Historical v1.0.4 Record

`vision-core-consensus-v1.0.4` identifies commit `b874d73cbdf60657334b62c867ed7f18b80a186b`. Its release scope was a deterministic P2P watchdog recovery test correction; it did not introduce a runtime consensus or protocol change relative to v1.0.3. This statement is historical and does not substitute for validating a future candidate.

See [0003: Release Identity](DECISIONS/0003_release_identity.md).
