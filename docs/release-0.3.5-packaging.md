# Viden 0.3.5 Packaging And Bundle Matrix

Chinese version: [release-0.3.5-packaging.zh-CN.md](release-0.3.5-packaging.zh-CN.md)

Date: 2026-09-13. Written by batch P1 of
[release-0.3.5-plan.md](release-0.3.5-plan.md), which owns "reproducible
bundles for macOS (arm64, x86_64), Windows, and Linux" and Exit Criterion 7.

This document records what was actually built, what was not, and why. It is
evidence, not a claim of release. Nothing here is tagged, signed with a real
identity, notarized, uploaded, published, or pushed.

## Provenance

| Field | Value |
| --- | --- |
| Branch | `claude/release-packaging-p1` |
| Worktree | `.worktrees/release-packaging-p1` |
| Base | `claude/int-0.3.5` at `f2298f4f` |
| Workspace / CLI version | `0.1.30` (`Cargo.toml` `[workspace.package]`) |
| TUI version | `0.3.5` |
| GUI version | `0.1.0-rc.5` (R1 bumps to `rc.6` and rebuilds) |
| Core checkpoint | `0.3.7`, code checkpoint `39ed155d` |

The versions above are the ones the bundles carry. R1 raises them; the
artifacts recorded here are therefore candidate-shaped evidence for the
packaging path, not the release unit's final assets.

## Authorization Boundary

Local builds and offline checks need no authorization and were run. Every
externally visible step needs an explicit per-step "go"; none was given for
this batch, so each is recorded below as **not run — authorization missing**
rather than skipped silently.

| External step | State |
| --- | --- |
| Dispatch `.github/workflows/release.yml` | not run — authorization missing |
| Create a Git tag | not run — authorization missing |
| Create or edit a GitHub Release | not run — authorization missing |
| Upload release assets | not run — authorization missing |
| Code signing with a Developer ID identity | not run — authorization missing, and no signing identity exists on the host (`security find-identity -v -p codesigning` → `0 valid identities found`) |
| Notarization (`notarytool`) | not run — authorization missing, and no Apple ID / Team ID credentials in the environment |
| Homebrew tap (`wikieden/homebrew-tap`) | not run — authorization missing |
| `git push` of any branch | not run — the batch does not merge or push |

## Toolchain Of Record

| Component | Value |
| --- | --- |
| Host | macOS 26.6.2 (build 25G83), arm64 |
| Xcode | 26.6 (17F113), `ld-1267` |
| `rustc` / `cargo` | 1.95.0 (Homebrew), host `aarch64-apple-darwin` |
| `rustup` | absent — the Rust toolchain is a Homebrew formula, so no cross target can be added |
| Installed std targets | `aarch64-apple-darwin` only |
| Node / npm | v24.16.0 / 11.13.0 |
| `@tauri-apps/cli` | 2.11.4 (`tauri` crate 2.11.5) |
| `actionlint` | absent — workflow validated by YAML parse instead |
| `gh` | 2.97.0, present but **not authenticated** (`The token in keyring is invalid`) |

The missing `rustup` is the single reason the macOS x86_64 row below is not
built. It is a host property, not a repository defect.

## Bundle Matrix

`built?` means an artifact exists on this host from this base. `signed?` and
`notarized?` describe the artifact as produced, not as it would ship.

| Platform | Arch | Format | Built? | Signed? | Notarized? | SHA-256 |
| --- | --- | --- | --- | --- | --- | --- |
| macOS | arm64 | GUI `.app` | yes | ad-hoc only — linker-signed Mach-O, bundle not sealed | no | binary `01373033…d6d44eca` |
| macOS | arm64 | GUI `.app` zip | yes | inherits the `.app` | no | `0366e3d2…b5ed63a9` |
| macOS | arm64 | GUI `.dmg` | yes | container unsigned | no | `8bba128a…df487da9` |
| macOS | arm64 | CLI/TUI `.tar.gz` | yes | ad-hoc only — linker-signed Mach-O | no | `18426464…f9eda50f` |
| macOS | x86_64 | GUI `.app` / `.dmg` | not built — `x86_64-apple-darwin` std is not installed and `rustup` is absent | — | — | — |
| macOS | x86_64 | CLI/TUI `.tar.gz` | not built — same toolchain gap | — | — | — |
| Windows | x86_64 (`x86_64-pc-windows-msvc`) | CLI/TUI `.tar.gz` | not built — no Windows host; the CI path exists and is prepared but its dispatch is not authorized | — | — | — |
| Windows | x86_64 | GUI `.msi` / NSIS `.exe` | not built — **no workflow job produces a GUI bundle on any platform** | — | — | — |
| Linux | x86_64 (`x86_64-unknown-linux-gnu`) | CLI/TUI `.tar.gz` | not built — no Linux host; CI path prepared, dispatch not authorized | — | — | — |
| Linux | x86_64 | GUI `.deb` / `.rpm` / `.AppImage` | not built — no workflow job produces a GUI bundle | — | — | — |

Full checksums are in the next section. The abbreviated column exists so the
table stays readable; it is not a second source of truth.

### The three-platform gate is not met

`docs/parallel-development-plan.md` requires macOS, Linux, and Windows build
and launch, and treats the GitHub Release and the Homebrew tap as one release
unit. Against that rule this matrix is one platform of three for the CLI and
one of three for the GUI, and the GUI has no cross-platform path at all. R1
cannot report the gate as met from this evidence.

## Artifacts And Checksums

Artifacts are build outputs and stay out of tracked source (`/target` and
`/dist` are ignored). They were produced in the worktree and copied to the
batch evidence directory; the checksums are the durable record.

`SHA256SUMS`, verbatim:

```
18426464fd317cffaa31b02c58810db75ff9af6e5531a703361f813cf9eda50f  viden-v0.1.30-aarch64-apple-darwin.tar.gz
0366e3d2ac8aac162806a31f4873ebc6bb5f389756973df95fb32ab7b5ed63a9  Viden-0.1.0-rc.5-aarch64-apple-darwin.app.zip
8bba128ad0eb6855c79f2802c6603e09fdbd698845952f5aa47ef784df487da9  Viden_0.1.0-rc.5_aarch64.dmg
```

| Artifact | Path as built | Bytes |
| --- | --- | --- |
| GUI app bundle | `target/release/bundle/macos/Viden.app` | 43,367,611 across 3 files |
| GUI app zip | `ditto -c -k --sequesterRsrc --keepParent` of the above | 14,340,514 |
| GUI disk image | `target/release/bundle/dmg/Viden_0.1.0-rc.5_aarch64.dmg` | 15,000,970 |
| CLI/TUI archive | `dist/viden-v0.1.30-aarch64-apple-darwin.tar.gz` | 10,196,763 |
| CLI/TUI binary inside it | `viden-v0.1.30-aarch64-apple-darwin/viden` | 28,282,432 |

The `.app` contains exactly three files — `Contents/Info.plist`,
`Contents/MacOS/viden-gui` (42,666,768 bytes), and
`Contents/Resources/icon.icns`. The frontend is embedded in the binary, which
is why the bundle has no web asset tree.

Bundle metadata, from `Contents/Info.plist`:

| Key | Value |
| --- | --- |
| `CFBundleShortVersionString` | `0.1.0-rc.5` |
| `CFBundleVersion` | `0.1.0-rc.5` |
| `CFBundleIdentifier` | `dev.viden.gui` |
| `CFBundleName` | `Viden` |

The packaged CLI reports `viden 0.1.30`.

## Signing State

`codesign -dv --verbose=4` on the `.app` reports:

```
Identifier=viden_gui-8e75e0f205f8032b
Format=app bundle with Mach-O thin (arm64)
CodeDirectory v=20400 size=330872 flags=0x20002(adhoc,linker-signed) ...
Signature=adhoc
Info.plist=not bound
TeamIdentifier=not set
Sealed Resources=none
```

Read this precisely: the signature is the **linker's** ad-hoc signature on the
Mach-O, not a bundle signature. `tauri.conf.json` declares no
`bundle.macOS.signingIdentity`, no entitlements, and no notarization fields,
so the bundler signed nothing. The consequences are:

- the code-signing identifier is the linker's hash-suffixed name, not the
  bundle identifier `dev.viden.gui`;
- `Info.plist` is not bound to the signature and `Sealed Resources=none`, so
  `codesign --verify --deep --strict` fails with
  `code has no resources but signature indicates they must be present`;
- Gatekeeper will quarantine the `.dmg` on any machine that did not build it.

That is the honest state for an unsigned local build. Real signing needs a
Developer ID identity, `signingIdentity` and entitlements in
`tauri.conf.json`, and a per-step authorization. None exists today.

Notarization: **not run — no credentials/authorization**.

## Reproducibility

The bundle was built twice from the same source, with `viden-gui` forced to
recompile and relink between them (`cargo clean -p viden-gui`), and the trees
compared.

| Item | Build 1 vs build 2 |
| --- | --- |
| `.app` file list | identical |
| `Contents/MacOS/viden-gui` | **identical** — `013730333814628a50ff28b2d71c44f993a93807e8be0d5e9eff6e6ad6d44eca` both times |
| `Contents/Info.plist` | **identical** — `d04627c8…b40fb069` |
| `Contents/Resources/icon.icns` | **identical** — `e7cfb70c…58b93efc` |
| `.app` zip | **differs** — `0366e3d2…` vs `1976acbd…` |
| `.dmg` | **differs** — `8bba128a…` vs `98e9aab8…`, and the size differs by 7 bytes (15,000,970 vs 15,000,963) |

Why each difference exists:

- **The `.app` content is reproducible.** Rust's release compile and link
  produced a byte-identical binary across a full recompile, so the bundle
  itself is deterministic on a fixed toolchain. This is the result that
  matters for "same source, same app".
- **The zip is not**, because `ditto -c -k` stores each entry's modification
  time and the bundler rewrites the bundle on every build (build 1 stamped
  `15:58:52`, build 2 `16:00:51`). The payload is the same; the container
  timestamps are not. `SOURCE_DATE_EPOCH` was exported for both builds and the
  Tauri bundler does not honour it.
- **The `.dmg` is not**, and differs far more than the zip: 14,906,628 of
  15,000,970 bytes differ. A UDZO disk image embeds the HFS volume creation
  date and UUID, and those feed into every compressed block, so one changed
  timestamp cascades through the whole image. Byte-stable `.dmg` output would
  need a post-processing step this repository does not have.

The practical rule for R1: publish and verify the checksum of the `.app` and
its binary, and treat `.dmg` checksums as per-build identifiers, not as
reproducibility evidence.

## What `release.yml` Actually Builds

Validated by YAML parse (`actionlint` is not installed on this host):

```
YAML PARSE: ok
triggers: workflow_dispatch
  input tag: required=true type=string default="v0.1.25"
  input upload_to_release: required=true type=boolean default=true
  matrix[0]: os=macos-latest target=aarch64-apple-darwin
  matrix[1]: os=macos-latest target=x86_64-apple-darwin
  matrix[2]: os=ubuntu-latest target=x86_64-unknown-linux-gnu
  matrix[3]: os=windows-latest target=x86_64-pc-windows-msvc
upload-release if: ${{ github.event.inputs.upload_to_release == 'true' }}
```

Per matrix entry the workflow runs `scripts/package-release.sh "$version"
"$target"`, which builds **`viden-cli` only** and emits
`dist/viden-v<version>-<target>.tar.gz` plus a `.sha256` sibling. The
`upload-release` job is gated on `upload_to_release == 'true'` and needs
`contents: write`.

Three findings follow, and each is an R1 input:

1. **No job builds a GUI bundle.** There is no `npm ci`, no
   `tauri build`, and no `.app`/`.dmg`/`.msi`/`.deb` anywhere in the workflow.
   Windows and Linux GUI bundles cannot be produced by dispatching it, today,
   at any authorization level. Closing the GUI half of the three-platform gate
   requires a new workflow job, not a dispatch.
2. **The tag input is a build label, not a checkout ref.** The workflow checks
   out the ref it is dispatched on and derives the version by stripping the
   leading `v` from `tag`. Passing `tag=v0.1.30` while dispatching on `main`
   builds `main`, labelled `0.1.30`.
3. **The 0.3.5 candidate is not on GitHub.** `f2298f4f` is on no remote
   branch; `origin/main` is `75e2a84a`. A dispatch today would therefore build
   `main`, not the integration branch — so the dispatch is worth running only
   after the candidate is pushed, which is itself an authorized step.

## Prepared Dispatch Command — Not Dispatched

Prepared, reviewed, and deliberately **not run**. `upload_to_release=false`
keeps it a build-only validation with no release mutation.

```bash
gh workflow run release.yml \
  --repo wikieden/viden \
  --ref <the pushed candidate ref> \
  -f tag=v0.1.30 \
  -f upload_to_release=false
```

Preconditions, all currently unmet:

1. an explicit "go" for the dispatch step;
2. `gh` authenticated — the keyring token is currently invalid, so the command
   fails on auth before it reaches the API;
3. the candidate pushed, so `--ref` names something GitHub can check out; with
   `--ref` omitted the workflow runs on the default branch, which is the
   behaviour `scripts/release-smoke.sh --github-actions` has today.

Follow-up under a "go": poll with `gh run watch` / `gh run list --workflow
release.yml`, then `gh run download <id> --dir <evidence dir>`, and record each
artifact's path, size, and checksum in this document.

## Offline Legs

`scripts/release-smoke.sh --skip-package --version 0.1.30 --target
aarch64-apple-darwin` was run with the live-provider, GitHub Actions, GitHub
Release, and Homebrew flags off. Sixteen legs passed and six were skipped by
flag:

```
- PASS `cargo-fmt`
- PASS `tdd-testing-contract-smoke`
- PASS `tui-turn-controller-smoke`
- PASS `rc-tui-stability-contract-smoke`
- PASS `final-zero-bug-contract-smoke`
- PASS `cargo-clippy`
- PASS `viden-terminal-entry-tests`
- PASS `workspace-tests`
- PASS `tui-regression`
- PASS `fallback-cli-smoke`
- PASS `plan-mode-smoke`
- PASS `daily-loop-smoke`
- PASS `codex-app-server-protocol-fixture`
- PASS `codex-app-server-write-guard`
- PASS `lane-operator-loop-smoke`
- PASS `rc-tui-stability-smoke`
- SKIP `package-smoke`
- SKIP `deepseek-dev-scenario-smoke` (use `--deepseek`)
- SKIP `provider-live-smoke` (use `--provider-smoke <id>`)
- SKIP `github-actions-release-validation` (use `--github-actions`)
- SKIP `github-release-assets-validation` (use `--github-release-assets`)
- SKIP `homebrew-validation` (use `--homebrew`)
```

`cargo-clippy` is `cargo clippy --workspace --all-targets -- -D warnings` and
passed with no warning, and `workspace-tests` is the full suite. The
`package-smoke` leg was skipped by `--skip-package` and covered instead by the
direct `scripts/package-release.sh 0.1.30 aarch64-apple-darwin` run above,
whose archive is in the checksum table and whose binary reports `viden 0.1.30`.
The last four SKIP rows are the external legs this batch is not authorized to
run.

## Gaps Carried To R1

1. macOS x86_64 needs either a `rustup` toolchain on this host or the CI
   matrix entry, which is already present and only needs an authorized
   dispatch on a pushed ref.
2. Windows and Linux CLI archives need the authorized dispatch.
3. Windows and Linux GUI bundles need a new workflow job; no dispatch can
   produce them.
4. Signing needs a Developer ID identity plus `signingIdentity` and
   entitlements in `apps/gui/src-tauri/tauri.conf.json`; notarization needs
   credentials and a `notarytool` step.
5. `.dmg` output is not byte-reproducible; decide whether R1 requires it.
6. The GitHub Release and Homebrew tap remain unvalidated as one release unit.
