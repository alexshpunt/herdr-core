# Personal herdr-core fork

This fork follows `herdrdev/herdr-core` and adds process-instance and foreground membership observations for Herdr App. Those observations let the App validate a folder report through shell wrappers without accepting a reused PID. They require a new core and holder; recreate terminals opened before the update.

A program that turns on focus reports (DEC mode 1004) gets the current focus at once, also when they are on already, not only at the next change, and hears it is unfocused when the last focused viewer leaves. The core takes a new viewer for unfocused, so a viewer whose window has focus must send FOCUS 1 after attaching: the fork's Herdr App and `hc attach` do. With upstream Herdr App, a program in a pane that opened focused is told it is unfocused until the pane loses and regains focus.

## Releases

Every push to `master` builds Windows x86_64 on GitHub. The workflow waits for successful core CI on the exact release commit before creating a GitHub Release. Windows binaries are unsigned. The archive has a SHA-256 checksum asset. GitHub and HTTPS are trusted for both the archive and checksum; the checksum is not an independent signature. The personal fork does not publish Linux or macOS builds.

Install or update the fork:

```powershell
powershell -c "irm https://github.com/alexshpunt/herdr-core/releases/latest/download/fork-install.ps1 | iex"
```

The installer pins the checksum and archive to the same release. It stages and checks the binary before updating; an unchanged binary is not replaced. An update uses the installed core's `update --from`, preserving running terminals. `HC_INSTALL_DIR` sets a custom install directory; with that override the installer does not modify the user PATH. `HC_FORK_RELEASE_BASE` selects a pinned release mirror for testing.

`hc update` uses the fork's releases, not upstream. Each build has a version such as `0.1.21-fork.18.2`; its run and attempt number distinguish builds made from the same upstream version. The latest manifest names the archive at one immutable release tag. `HERDR_RELEASES_URL` remains available for test mirrors using the upstream manifest layout.

## Upstream synchronization

`fork-sync-upstream.yml` runs daily at 04:43 UTC and can be started manually on `master`. It prepares a candidate merge on an `automation/upstream-*` branch, runs core CI on that exact SHA, and fast-forwards `master` only if the candidate and base did not change. That push starts the release workflow on the verified commit. It never force-pushes.

A conflict in `schema/hc.openrpc.json`, which both sides extend, resolves itself: `scripts/fork-resolve-conflicts` regenerates the schema from the merged code (`HC_UPDATE_SCHEMA=1` on the m0 schema test). Any other conflict stops the run with the conflicted files listed; the scheduled workflow runs no paid model. Resolve those by hand on a branch and review them through a PR.

Pushes use the `FORK_SYNC_TOKEN` repository secret: a fine-grained personal access token of the owner, limited to this repository and `alexshpunt/herdr-app` with Contents, Workflows and Actions write access and no expiration. The default `GITHUB_TOKEN` cannot push upstream commits that change `.github/workflows`. The merge job never holds the token: it hands the candidate to a fresh job as a git bundle.

When the merge, CI or a push fails, `master` stays unchanged and the run fails. The webhook receiver of agent-infrastructure on pi-vps then alerts the owner: an email, the Linear issue "Upstream sync of alexshpunt/herdr-core is failing" (opened, or commented while it is open) and a row in the GitHub Inbox of herdr-app-plugins. It matches the workflow by the word "upstream" in its name, so keep that word when renaming it, and it reads the cause from the failed steps' `::error::` annotations, which `scripts/fork-sync.py` prints. The repository needs no secret for alerts. Close the issue once a hand merge lands. Enable GitHub Actions for the fork. Disable the inherited upstream `release.yml` in this fork; it uses upstream signing and storage infrastructure, not the fork release path.

Windows CI and the fork release run on `windows-2022`: on the `windows-2025` image with Visual Studio 2026, MSVC 14.52's STL rejects the clang in zig 0.16 (STL1000) when ghostty builds simdutf.
