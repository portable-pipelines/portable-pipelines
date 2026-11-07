# Releasing Portable Pipelines

This page is for John and for an assistant helping him. It describes how a
release is prepared in chat, how GitHub Actions publishes it after an
explicit go, and how to check the result. It does not cut a release by
itself.

**Merging this setup does not publish anything.** Ordinary pull requests
do not publish anything. The only merge that publishes is a later,
dedicated release-intent pull request, after John has reviewed the
version and the notes and has said to go.

There is no standing permission to ship arbitrary versions. Each release
needs its own intent file, its own notes, and a fresh explicit go.

## What a reader will download

A published GitHub Release for tag `vX.Y.Z` is complete only when it has
all four platform archives, each under a versioned name and again under a
stable name with no version, plus a `.sha256` sidecar for every file.
That is sixteen files:

- `pipe-windows-x64-vX.Y.Z.zip` and `pipe-windows-x64.zip`
- `pipe-linux-x86_64-vX.Y.Z.tar.gz` and `pipe-linux-x86_64.tar.gz`
- `pipe-macos-arm64-vX.Y.Z.tar.gz` and `pipe-macos-arm64.tar.gz`
- `pipe-macos-x86_64-vX.Y.Z.tar.gz` and `pipe-macos-x86_64.tar.gz`
- a `.sha256` file beside each of those eight archives

The checksum text names the file it belongs to. The Windows `pipe.exe` is
Authenticode-signed. GitHub records a build provenance attestation for
each archive. Linux and macOS tarballs do not bundle Regina or Python.

Stable names are how
`https://github.com/portable-pipelines/portable-pipelines/releases/latest/download/`
keeps working after later releases. Packaging details live in
[packaging.md](packaging.md).

## Chat-driven release, in order

1. John asks to do a release, for example "let's do a release". He may
   name a version. If he does not, the assistant proposes the next
   version from the current workspace version in `Cargo.toml` and waits.
   The next version is John's decision. This repository was at `0.1.6`
   when the automation was added. Nothing here chooses `0.1.7` for him.
2. The assistant collects the real commits since the previous published
   tag and shows John a preview: the proposed version, the changelog
   section, and drafted notes. The helper lists commits. It does not
   invent the public story. The assistant (or John) writes readable
   notes in complete sentences from those commits and from the work that
   actually landed, including the marina examples, docs-in-packages, and
   `json objects` changes that were still unreleased after `v0.1.6`.
3. John reviews that preview. The assistant does not open a
   release-intent pull request until John explicitly says to go.
4. After that go, the assistant bumps the workspace version, moves
   `[Unreleased]` in `CHANGELOG.md`, writes the reviewed notes to
   `docs/release-notes-vX.Y.Z.md`, and commits a live intent file at
   `.github/release-intent.json`. Those changes land in one dedicated
   pull request. That pull request is the release request. Its title is
   not what Actions trusts.
5. John reviews the pull request. The assistant merges it only after a
   second explicit go. Merging that pull request is the publish trigger.
   The built-in GitHub connection can merge a pull request. It cannot
   create a GitHub Release or dispatch a workflow, so merge is the
   deliberate trigger on purpose. No extra personal access token is
   required for this path.
6. GitHub Actions then tags the exact merge commit, opens a **draft**
   GitHub Release whose body is the committed notes file, builds and
   signs all four platforms from that commit, uploads the sixteen files,
   and only then publishes the draft. John does not need to click through
   the GitHub Release UI for that path.

If John says "prepare a draft" or "show me notes" without saying to
publish, the assistant stops after the preview.

## Helper commands

From a clone, with Python 3:

```bash
python3 scripts/release/prepare_release.py collect
python3 scripts/release/prepare_release.py prepare --version 0.1.7 --dry-run
python3 scripts/release/prepare_release.py check
```

`collect` prints commits since the latest `vX.Y.Z` tag. `prepare` (without
`--dry-run`) writes `Cargo.toml`, `Cargo.lock`, `CHANGELOG.md`, the notes
skeleton, and `.github/release-intent.json`. The skeleton contains the
marker `DRAFT-RELEASE-NOTES`. Actions will refuse to publish while that
marker remains. Replace the skeleton with the reviewed notes before you
ask John to merge.

`prepare` and `check` never create a tag, never publish, and never upload
assets. GitHub Actions is what publishes, and only after the intent
pull request is merged to `master`.

An example intent with `"publish": false` lives at
`scripts/release/example-intent.json`. Copying that example onto
`.github/release-intent.json` without changing `"publish"` to JSON
`true` will not publish.

## The intent file

Actions reads `.github/release-intent.json` from the merge commit. It
does not trust pull request titles, pull request bodies, or chat text.
The file must be UTF-8 JSON with exactly these keys:

| Key | Meaning |
|---|---|
| `schema_version` | The integer `1`, not a string |
| `publish` | JSON `true` or `false`, not the string `"true"` |
| `version` | Workspace version, three numeric parts, for example `0.1.7` |
| `tag` | `v` plus `version`, for example `v0.1.7` |
| `notes_file` | Exactly `docs/release-notes-v{version}.md` |

Unknown keys, duplicate keys, path escape, and comments are rejected.
The same commit must have a matching `Cargo.toml` workspace version,
matching workspace versions in `Cargo.lock`, a dated `CHANGELOG.md`
section for that version, and notes that are non-empty UTF-8 without the
draft marker. The JSON Schema is
[`.github/release-intent.schema.json`](../.github/release-intent.schema.json).

`"publish": false` is a valid file that Actions will only validate.

## Why packaging still runs (GITHUB_TOKEN)

GitHub will not start a new workflow from a tag or a GitHub Release that
a workflow created with `GITHUB_TOKEN`. The documented exceptions are
`workflow_dispatch` and `repository_dispatch`. `release: published` is
not an exception. If this automation created a Release and then waited
for the existing `release: published` workflow, the build, sign, attest,
and upload jobs would never run.

This repository therefore does two things in **one** Actions run:

1. `release-from-intent.yml` runs when `.github/release-intent.json`
   changes on `master`. It validates the committed files, creates the
   tag at `github.sha`, and opens a draft Release whose body is the
   committed notes.
2. That same workflow **calls** `.github/workflows/release.yml` as a
   reusable workflow (`workflow_call`) with the tag and the commit SHA.
   Packaging, Authenticode signing (environment `signing`), attestations,
   and upload happen there, on that SHA, for all four platforms.

After every expected asset is present on the draft, the intent workflow
publishes the draft. Publishing with `GITHUB_TOKEN` does not start a
second `release: published` run, so there is no double build and no
double upload.

A personal access token is not required, and it would be a worse design
here: a PAT-created Release **would** fire `release: published` and could
race the reusable call.

## What does not publish

- Merging a pull request that does not change `.github/release-intent.json`
- Merging this automation setup, which does not include a live intent file
- Opening or updating a release-intent pull request (Actions only
  validates on `pull_request`)
- `"publish": false` in the intent file
- `workflow_dispatch` on `release.yml` (signing dry run, no GitHub Release)
- The packaging dry run that `release.yml` already runs on pull requests
  (Windows zip, no Azure login, no upload)

## Failure, retry, and "latest"

The GitHub Release stays a **draft** until upload of all sixteen files
succeeds. Drafts are not GitHub's latest release, so a failed Windows or
Unix job does not replace the previous latest download links.

If a job fails, do not delete the tag and do not force-push. Re-run the
failed jobs on the same workflow run. The planner treats a draft at the
same commit as a resume: it re-uploads onto that draft (`--clobber` is
only for retrying the same draft) and publishes when the set is complete.

If the tag already exists on a **different** commit, Actions refuses.
If the Release is already **published**, Actions will not upload, retag,
or overwrite it. A complete published Release at the same commit is a
successful no-op so a rerun cannot ship twice. A published Release that
is missing files is also a refusal; fixing that needs a human decision
and is not done automatically.

Concurrency groups prevent two publishes from cancelling each other
mid-sign (`cancel-in-progress` is false).

Later ordinary commits do not re-release. The intent workflow's path
filter is the intent file, and a leftover intent for an already-published
tag is a no-op.

## Manual GitHub Release path

You can still publish a GitHub Release by hand. That human `published`
event is not from `GITHUB_TOKEN`, so `release.yml` still runs, still
signs, still attests, and still uploads the same names. That path
publishes **before** the archives exist, so GitHub's latest can briefly
point at a Release with missing files. The intent path is the one that
keeps the Release a draft until the archives are in place.

Do not mix the two for the same version. If the intent path already
created the tag, do not also create that Release in the UI.

## After the first authorized release: checks

These checks are what "it worked" means. They are not claimed for this
setup pull request.

1. `pipe --version` from each archive prints `pipe X.Y.Z`.
2. Each `.sha256` sidecar matches the archive bytes and names that file.
3. Versioned and stable pairs are the same bytes (same digest).
4. All four platforms are present, built from the same tag and commit.
5. Latest download URLs resolve to the new stable names:
   `https://github.com/portable-pipelines/portable-pipelines/releases/latest/download/pipe-windows-x64.zip`
   and the three Unix stable tarballs.
6. `gh attestation verify <archive> --repo portable-pipelines/portable-pipelines`
   succeeds for the archives.
7. On Windows, `Get-AuthenticodeSignature` for `pipe.exe` is `Valid`.
8. The GitHub Release body is the committed `docs/release-notes-vX.Y.Z.md`
   text, not a regenerated summary.

PowerShell checksum example:

```powershell
Get-FileHash .\pipe-windows-x64-vX.Y.Z.zip -Algorithm SHA256
Get-Content .\pipe-windows-x64-vX.Y.Z.zip.sha256
```

## Prerequisites that this setup cannot prove

The following only show up on a real, authorized release:

- Microsoft Artifact Signing through GitHub OIDC and environment
  `signing`, now reached from `release.yml` via `workflow_call` as well
  as from `release: published`. The Entra federated credential is
  documented in the workflow as scoped to this repository and the
  `signing` environment name, not to a single event. If a first intent
  run fails at Azure login, the federated credential may need to allow
  this job.
- GitHub artifact attestations on that same `workflow_call` path.
- Draft-then-publish becoming GitHub's latest only after all sixteen
  files exist.
- End-to-end download of the four platforms from `releases/latest`.

The `signing` environment currently has no required reviewers, so the
intent path should not stop for an extra environment approval. If that
environment later requires reviewers, someone would have to approve the
Windows job in the GitHub UI.

This page does not change the project site and does not send
announcements.

## Tests you can run without publishing

```bash
python3 -m unittest discover -s scripts/release -p 'test_*.py' -v
python3 scripts/release/prepare_release.py check
python3 scripts/release/prepare_release.py prepare --version 0.1.7 --dry-run
```

CI runs those unit tests and `actionlint` on every pull request. That is
not a production release. Do not treat a green setup pull request as
proof that signing, attestations, or latest-download links work until
John authorizes a real release.
