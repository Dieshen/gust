# VS Code Marketplace Publish Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Ship `editors/vscode/` to the VS Code Marketplace as `gust-lang.gust-lang`, with repeatable manual-publish + GitHub-Actions-automated publish paths.

**Architecture:** Keep the extension source under `editors/vscode/` as today. Package via `@vscode/vsce`. Authenticate via an Azure DevOps Personal Access Token stored as the repo secret `VSCE_PAT`. Publish manually for the first release (to register the publisher and eyeball the listing) and via a tag-triggered GitHub Actions workflow for all subsequent releases.

**Tech Stack:** Node 20, TypeScript 5.x, `@vscode/vsce` 2.27+, GitHub Actions, Azure DevOps PAT.

---

### Task 1: Complete `package.json` for marketplace listing

**Files:**
- Modify: `editors/vscode/package.json`

**Step 1:** Add a `repository` field pointing at the GitHub source of truth:
```json
"repository": { "type": "git", "url": "https://github.com/Dieshen/gust.git" }
```
**Step 2:** Add a `license` field (pick to match the workspace's `LICENSE` — currently MIT):
```json
"license": "MIT"
```
**Step 3:** Add a `bugs` field:
```json
"bugs": { "url": "https://github.com/Dieshen/gust/issues" }
```
**Step 4:** Add an `icon` field referencing a 128×128 PNG (see Task 2):
```json
"icon": "images/marketplace-icon.png"
```
**Step 5:** Add `keywords` and a `galleryBanner` to help discovery and polish:
```json
"keywords": ["gust", "state-machine", "language-server", "codegen", "rust", "go"],
"galleryBanner": { "color": "#1f2937", "theme": "dark" }
```
**Step 6:** Run `npx @vscode/vsce ls` inside `editors/vscode/` to confirm the packaged file list is correct (no stray build artefacts, no `node_modules`). Add a `.vscodeignore` if needed.

### Task 2: Add marketplace icon

**Files:**
- Create: `editors/vscode/images/marketplace-icon.png`

**Step 1:** Produce a 128×128 PNG marketplace icon. The existing `images/fileicon.svg` (pixelated wind dots) is the design source; rasterise it at 128×128 and 256×256 for HiDPI.
**Step 2:** Commit only the 128×128 PNG referenced by `package.json` (marketplace requires a single raster).
**Step 3:** Open `editors/vscode/README.md` in VS Code's markdown preview to sanity-check that it will render on the marketplace listing; fix any broken image links.

### Task 3: First-time publisher and PAT setup (manual, one-time)

**Files:**
- None in this repo. Document outcome in `editors/vscode/README.md`.

**Step 1:** Create an Azure DevOps organisation at https://dev.azure.com (any name — it's just the PAT container).
**Step 2:** Generate a Personal Access Token: User Settings → Personal access tokens → New Token.
- Organization: **All accessible organizations** (the default single-org scope will not work for marketplace auth).
- Scope: **Custom defined** → **Marketplace** → **Manage**.
- Expiration: ≤ 1 year. Record the rotation date.
**Step 3:** At https://marketplace.visualstudio.com/manage create a publisher whose ID exactly matches the `"publisher"` field in `package.json` (currently `gust-lang`). If that ID is taken, pick a new one and update `package.json` to match.
**Step 4:** Save the PAT in a secure store (password manager). Do not commit it, paste it into chat, or store it in `.env`.

### Task 4: First manual publish

**Files:**
- None modified. Produces a release on the marketplace.

**Step 1:** From `editors/vscode/`, run `npm ci && npm run compile` to produce a clean build.
**Step 2:** Authenticate once with `npx @vscode/vsce login gust-lang` and paste the PAT when prompted.
**Step 3:** Publish the current `package.json` version:
```
npx @vscode/vsce publish
```
(Or `npx @vscode/vsce publish patch` / `minor` / `major` to bump the version in the same command.)
**Step 4:** Open `https://marketplace.visualstudio.com/items?itemName=gust-lang.gust-lang`; confirm the listing renders, icon appears, README is legible, and the install button works from a fresh VS Code instance (`code --install-extension gust-lang.gust-lang`).
**Step 5:** Tag the git commit that was published, e.g. `git tag vscode-v0.2.0 && git push origin vscode-v0.2.0`, so every marketplace release is reproducible from source.

### Task 5: Automate via GitHub Actions

**Files:**
- Create: `.github/workflows/publish-vsce.yml`
- Modify: `.github/SECURITY.md` (optional — document the secret's rotation cadence)

**Step 1:** Add `VSCE_PAT` as a repository secret under the Dieshen/gust repo Settings → Secrets → Actions.
**Step 2:** Create the workflow, triggered only by tags matching `vscode-v*`:
```yaml
name: Publish VS Code extension
on:
  push:
    tags: ["vscode-v*"]
permissions:
  contents: read
jobs:
  publish:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v6
      - uses: actions/setup-node@v4
        with: { node-version: "20" }
      - name: Install deps
        working-directory: editors/vscode
        run: npm ci
      - name: Compile
        working-directory: editors/vscode
        run: npm run compile
      - name: Verify package contents
        working-directory: editors/vscode
        run: npx @vscode/vsce ls
      - name: Publish
        working-directory: editors/vscode
        run: npx @vscode/vsce publish -p ${{ secrets.VSCE_PAT }}
```
**Step 3:** Test the workflow on a throwaway pre-release tag (e.g. `vscode-v0.2.1-rc1`) before the first real tag. Verify the marketplace listing updates.
**Step 4:** Document the release ritual in `editors/vscode/README.md`:
1. Bump `version` in `editors/vscode/package.json`.
2. Commit, `git tag vscode-vX.Y.Z`, push both.
3. Watch the workflow succeed.

### Task 6: Version-sync discipline (optional but recommended)

**Files:**
- Modify: `editors/vscode/package.json` (on each release)
- Optional: `xtask` or Makefile target to bump extension + workspace together

**Step 1:** Decide whether the extension's `version` tracks the workspace `Cargo.toml` versions (coupled) or floats independently (decoupled). Default recommendation: **decoupled** — the extension can change without a compiler release, and vice versa.
**Step 2:** Add a short note in `editors/vscode/README.md` stating the extension's version scheme and its current compatibility window with the `gust` CLI and `gust-lsp`.
**Step 3:** If coupling is preferred later, script it in `xtask` or a Make target rather than relying on manual sync.

### Task 7: Rotation + incident response documentation

**Files:**
- Modify: `SECURITY.md` or `editors/vscode/README.md`

**Step 1:** Document PAT rotation: every ≤ 12 months, or immediately if the token is exposed. Rotation procedure:
1. Generate a new PAT in Azure DevOps with the same scope.
2. Update the `VSCE_PAT` repository secret.
3. Revoke the old PAT.
4. Trigger a no-op publish (bump patch, tag, push) to confirm the new token works end-to-end.
**Step 2:** Document unpublish procedure (`npx @vscode/vsce unpublish gust-lang.gust-lang`) and note that unpublishing is **permanent** for that version string — subsequent publishes must bump the version.

### Task 8: Pre-publish verification checklist

**Files:**
- Workspace

**Step 1:** `cd editors/vscode && npm ci && npm run compile`
**Step 2:** `npx @vscode/vsce ls` — confirm no `node_modules`, no `.map` files, no test fixtures leak into the VSIX.
**Step 3:** `npx @vscode/vsce package` — produces `gust-lang-X.Y.Z.vsix`.
**Step 4:** `code --install-extension gust-lang-X.Y.Z.vsix --force` in a clean VS Code profile (`code --user-data-dir /tmp/fresh-profile`) and smoke-test:
- Open a `.gu` file → syntax highlighting lights up.
- Hover / go-to-definition work (requires `gust-lsp` on PATH).
- `gust.showDiagram` command runs without errors.
**Step 5:** If all green, proceed to publish (Task 4 for manual, Task 5 for tag-triggered).

---

## Rollback plan

- **Bad release shipped:** bump the version (e.g. `0.2.1` → `0.2.2`), republish. Marketplace does not allow re-uploading the same version. If the release is actively harmful, also unpublish the bad version: `npx @vscode/vsce unpublish gust-lang.gust-lang@0.2.1`.
- **Compromised PAT:** revoke immediately in Azure DevOps → User Settings → Personal access tokens. Generate a new one, update `VSCE_PAT` secret, publish a no-op version to confirm.
- **Wrong publisher ID committed:** if `package.json` publishes to the wrong publisher, the marketplace listing will appear under that publisher; you cannot transfer it. Unpublish and re-publish under the correct publisher ID.
