#!/bin/sh
# Installs or updates the personal herdr-core fork from GitHub Releases.
# Running terminals are kept. HC_INSTALL_DIR selects a destination;
# HC_FORK_RELEASE_BASE selects a pinned release mirror.
set -eu

# Pin both files to one GitHub release. HC_FORK_RELEASE_BASE is a pinned mirror URL.
releases=${HC_FORK_RELEASE_BASE:-}
bin=${HC_INSTALL_DIR:-$HOME/.local/bin}

say() { printf '%s\n' "$*"; }
fail() { say "hc: $*" >&2; exit 1; }
need() { command -v "$1" >/dev/null 2>&1 || fail "needs $1"; }
need curl
need tar

# An update can start a missing core. Give it login variables, not an agent's environment.
safe_hc() {
  env -i ${PATH:+"PATH=$PATH"} ${HOME:+"HOME=$HOME"} ${USER:+"USER=$USER"} \
    ${LOGNAME:+"LOGNAME=$LOGNAME"} ${SHELL:+"SHELL=$SHELL"} ${TMPDIR:+"TMPDIR=$TMPDIR"} \
    ${LANG:+"LANG=$LANG"} ${LC_ALL:+"LC_ALL=$LC_ALL"} ${LC_CTYPE:+"LC_CTYPE=$LC_CTYPE"} \
    ${SSH_AUTH_SOCK:+"SSH_AUTH_SOCK=$SSH_AUTH_SOCK"} ${SSH_CONNECTION:+"SSH_CONNECTION=$SSH_CONNECTION"} \
    ${SSH_CLIENT:+"SSH_CLIENT=$SSH_CLIENT"} ${SSH_TTY:+"SSH_TTY=$SSH_TTY"} \
    ${DISPLAY:+"DISPLAY=$DISPLAY"} ${WAYLAND_DISPLAY:+"WAYLAND_DISPLAY=$WAYLAND_DISPLAY"} \
    ${XAUTHORITY:+"XAUTHORITY=$XAUTHORITY"} ${DBUS_SESSION_BUS_ADDRESS:+"DBUS_SESSION_BUS_ADDRESS=$DBUS_SESSION_BUS_ADDRESS"} \
    ${XDG_CONFIG_HOME:+"XDG_CONFIG_HOME=$XDG_CONFIG_HOME"} ${XDG_DATA_HOME:+"XDG_DATA_HOME=$XDG_DATA_HOME"} \
    ${XDG_STATE_HOME:+"XDG_STATE_HOME=$XDG_STATE_HOME"} ${XDG_RUNTIME_DIR:+"XDG_RUNTIME_DIR=$XDG_RUNTIME_DIR"} \
    ${HC_SESSION:+"HC_SESSION=$HC_SESSION"} ${HC_SOCKET:+"HC_SOCKET=$HC_SOCKET"} "$bin/herdr-core" "$@"
}

case "$(uname -s)" in
  Linux) os=linux ;;
  Darwin) os=macos ;;
  *) fail "hc runs on Linux and macOS for now" ;;
esac
case "$(uname -m)" in
  x86_64 | amd64) arch=x86_64 ;;
  arm64 | aarch64) arch=aarch64 ;;
  *) fail "no build for $(uname -m)" ;;
esac
target="$os-$arch"

sha256() {
  if command -v sha256sum >/dev/null 2>&1; then sha256sum "$1" | cut -d' ' -f1
  else shasum -a 256 "$1" | cut -d' ' -f1; fi
}

tmp=$(mktemp -d)
stage=
trap 'rm -rf "$tmp"; [ -z "$stage" ] || rm -f "$stage"' EXIT

if [ -z "$releases" ]; then
  latest=$(curl -fsSL -o /dev/null -w '%{url_effective}' https://github.com/alexshpunt/herdr-core/releases/latest) || fail 'cannot reach the latest release'
  tag=${latest##*/}
  case "$tag" in fork-*-*) ;; *) fail 'latest release is not a fork build' ;; esac
  case "$tag" in *[!A-Za-z0-9._-]*) fail 'invalid release tag' ;; esac
  releases="https://github.com/alexshpunt/herdr-core/releases/download/$tag"
fi
name="herdr-core-$target.tar.gz"
curl -fsSL "$releases/$name.sha256" -o "$tmp/sha256" || fail 'cannot download checksum'
want=$(awk 'NR == 1 { print $1; exit }' "$tmp/sha256")
case "$want" in *[!0-9a-f]*|'') fail 'invalid checksum' ;; esac
[ "${#want}" = 64 ] || fail 'invalid checksum'
curl -fsSL "$releases/$name" -o "$tmp/hc.tar.gz" || fail 'cannot download core'
[ "$(sha256 "$tmp/hc.tar.gz")" = "$want" ] || fail 'checksum mismatch'
tar -xzf "$tmp/hc.tar.gz" -C "$tmp"
core=$tmp/herdr-core
if ! "$core" --version >/dev/null 2>&1; then
  # A noexec mount under the temp dir fails the check with EACCES, which is
  # not "cannot run here": stage on the destination filesystem and re-check.
  mkdir -p "$bin"
  stage=$(mktemp "$bin/.herdr-core-staged.XXXXXX")
  core=$stage
  if ! cp "$tmp/herdr-core" "$core" || ! chmod 755 "$core"; then fail "cannot stage hc in $bin"; fi
  "$core" --version >/dev/null 2>&1 || fail "the downloaded hc does not run on this machine"
fi
version=$("$core" --version)
version=${version#hc }
# App completion must converge even before the first bundled release exists.
if [ "${HC_BUNDLE_ONLY:-0}" = 1 ] && [ ! -x "$tmp/Herdr Core.app/Contents/MacOS/herdr-core" ]; then
  exit 0
fi
mkdir -p "$bin"

bundle=0
if [ "$os" = macos ] && [ "$(id -u)" != 0 ]; then
  case "$bin" in "$HOME"/*) bundle=1 ;; esac
fi
if [ "$bundle" = 1 ] && [ -x "$tmp/Herdr Core.app/Contents/MacOS/herdr-core" ]; then
  action=installed
  [ ! -x "$bin/herdr-core" ] || action=updated
  bundled="$tmp/Herdr Core.app/Contents/MacOS/herdr-core"
  if [ "$action" = updated ]; then
    installed=$("$bin/herdr-core" --version)
    candidate=$("$bundled" --version)
    awk -v old="$installed" -v new="$candidate" 'BEGIN {
      sub(/^hc /, "", old); sub(/^hc /, "", new)
      split(old, a, /[.-]/); split(new, b, /[.-]/)
      for (i=1; i<=3; i++) { if (a[i]+0 > b[i]+0) exit 1; if (a[i]+0 < b[i]+0) exit 0 }
    }' || fail "$candidate is older than $installed; use hc update --from with --force to downgrade"
  fi
  case "$releases" in
    https://releases.herdr.dev | https://releases.herdr.dev/)
      /usr/bin/codesign --verify --strict -R '=identifier "dev.herdr.core" and anchor apple generic and certificate leaf[subject.OU] = "2W9652Q7QR"' "$tmp/Herdr Core.app" || fail "not a signed Herdr release" ;;
  esac
  "$bundled" --hc-install-bundle "$bundled" "$bin/herdr-core" || fail "installing Herdr Core.app failed"
  # A fresh install must not start a core (especially not a system/root core).
  if [ "$action" = updated ]; then
    safe_hc server restart >/dev/null || fail "restarting the core failed"
  fi
  say "hc $version $action as Herdr Core.app. Recreate pre-update terminals if macOS privacy access is broken; running terminals were kept."
elif [ -x "$bin/herdr-core" ]; then
  if [ "$(sha256 "$bin/herdr-core")" = "$(sha256 "$core")" ]; then
    say "hc $version is already installed."
  else
    # Swapped the planned way, so its terminals keep running.
    safe_hc update --from "$core" >/dev/null || fail "updating $bin/herdr-core failed (see above)"
    say "hc updated to $version."
  fi
else
  cp "$core" "$bin/herdr-core.new"
  chmod 755 "$bin/herdr-core.new"
  mv -f "$bin/herdr-core.new" "$bin/herdr-core"
  say "hc $version installed into $bin/herdr-core."
fi
if ! command -v hc >/dev/null 2>&1 && [ ! -e "$bin/hc" ]; then
  ln -s herdr-core "$bin/hc"
fi

case ":$PATH:" in
  *":$bin:"*) ;;
  *) say "Add $bin to your PATH to use hc." ;;
esac
say "Agents: hc --skill. People: hc --help."
