# Personal herdr-core fork

This fork follows `herdrdev/herdr-core` and adds process-instance and foreground membership observations for Herdr App. Those observations let the App validate a folder report through shell wrappers without accepting a reused PID. They require a new core and holder; recreate terminals opened before the update.

## Releases

Every push to `master` builds five targets on GitHub: Windows x86_64, Linux x86_64 and aarch64, and macOS x86_64 and aarch64. The workflow waits for successful core CI on the exact release commit before creating a GitHub Release. Windows binaries are unsigned; macOS bundles are ad-hoc signed and not notarized. Every archive has a SHA-256 checksum asset. GitHub and HTTPS are trusted for both the archive and checksum; the checksum is not an independent signature.

Install or update the fork:

```powershell
powershell -c "irm https://github.com/alexshpunt/herdr-core/releases/latest/download/fork-install.ps1 | iex"
```

```sh
curl -fsSL https://github.com/alexshpunt/herdr-core/releases/latest/download/fork-install.sh | sh
```

Both installers pin the checksum and archive to the same release. They stage and check the binary before updating; an unchanged binary is not replaced on Windows or Linux. An update uses the installed core's `update --from`, preserving running terminals. On macOS the app bundle follows the upstream local bundle installation path. The Unix installer passes only login and core-connection variables to a core it might start. `HC_INSTALL_DIR` sets a custom install directory; with that override the Windows installer does not modify the user PATH. `HC_FORK_RELEASE_BASE` selects a pinned release mirror for testing.

`hc update` uses the fork's releases, not upstream. Each build has a version such as `0.1.21-fork.18.2`; its run and attempt number distinguish builds made from the same upstream version. The latest manifest names archives at one immutable release tag. Installation on a machine of another architecture requests the exact build, not the latest one. `HERDR_RELEASES_URL` remains available for test mirrors using the upstream manifest layout.

The macOS bundles are ad-hoc signed and not notarized. Their signature verifies the bundle's integrity but not the publisher; the download relies on GitHub, HTTPS and the manifest checksum.

## Upstream synchronization

`fork-sync-upstream.yml` runs daily at 04:43 UTC and can be started manually on `master`. It prepares a candidate merge on an `automation/upstream-*` branch, runs the existing core CI on that exact SHA, and fast-forwards `master` only if the candidate and base did not change. The release workflow then builds the verified commit. It never force-pushes.

If a merge conflicts or verification fails, the workflow leaves `master` unchanged and fails the Actions run. Resolve conflicts manually on a branch and review them through a PR; the scheduled workflow does not run a paid model or create public issues. Enable GitHub Actions for the fork. Disable the inherited upstream `release.yml` in this fork; it uses upstream signing and storage infrastructure, not the fork release path.
