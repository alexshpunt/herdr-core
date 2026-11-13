//! Fork releases on GitHub: latest or an exact build's immutable manifest,
//! with checked archives. HERDR_RELEASES_URL retains the test mirror layout.
//! curl, sha256sum/shasum (Windows: its own crypto) and tar do the work.

use crate::rpc::RpcError;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Deserialize)]
pub struct Manifest {
    pub version: String,
    pub generation: u32,
    pub targets: BTreeMap<String, Target>,
}

#[derive(Deserialize)]
pub struct Target {
    pub url: String,
    pub sha256: String,
}

const FORK_RELEASES: &str = "https://github.com/alexshpunt/herdr-core/releases";

#[derive(PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Version {
    base: [u64; 3],
    fork: Option<(u64, u64)>,
}

/// A plain upstream version sorts before a fork build of the same version.
pub(crate) fn version_parts(version: &str) -> Option<Version> {
    let (base, fork) = match version.split_once('-') {
        Some((base, suffix)) => {
            let mut parts = suffix.strip_prefix("fork.")?.split('.');
            let run = parts.next()?.parse::<u64>().ok()?;
            let attempt = parts.next()?.parse::<u64>().ok()?;
            if parts.next().is_some() || run == 0 || attempt == 0 {
                return None;
            }
            (base, Some((run, attempt)))
        }
        None => (version, None),
    };
    let mut parts = base.split('.');
    let base = [parts.next()?.parse().ok()?, parts.next()?.parse().ok()?, parts.next()?.parse().ok()?];
    if parts.next().is_some() {
        return None;
    }
    Some(Version { base, fork })
}

fn fork_tag(version: &str) -> Option<String> {
    let (run, attempt) = version_parts(version)?.fork?;
    Some(format!("fork-{run}-{attempt}"))
}

fn unavailable(why: String) -> RpcError {
    RpcError::new("unavailable", why, "check the fork's GitHub releases and your connection")
}

fn manifest_url(version: Option<&str>, mirror: Option<&str>) -> Result<String, RpcError> {
    if let Some(base) = mirror {
        let base = base.trim_end_matches('/');
        return Ok(match version {
            Some(v) if version_parts(v).is_some() => format!("{base}/core/{v}/manifest.json"),
            Some(_) => return Err(unavailable("invalid release version".into())),
            None => format!("{base}/core/latest.json"),
        });
    }
    match version {
        None => Ok(format!("{FORK_RELEASES}/latest/download/manifest.json")),
        Some(v) => fork_tag(v)
            .map(|tag| format!("{FORK_RELEASES}/download/{tag}/manifest.json"))
            .ok_or_else(|| unavailable(format!("{v} does not identify a fork build"))),
    }
}

fn validate_manifest(m: &Manifest, version: Option<&str>, fork: bool) -> Result<(), RpcError> {
    if version.is_some_and(|v| v != m.version) {
        return Err(unavailable("release list does not match the requested version".into()));
    }
    if fork {
        let tag = fork_tag(&m.version).ok_or_else(|| unavailable("release list has no fork build identity".into()))?;
        for (target, artifact) in &m.targets {
            let ext = match target.as_str() {
                "windows-x86_64" => "zip",
                "linux-x86_64" | "linux-aarch64" | "macos-x86_64" | "macos-aarch64" => "tar.gz",
                _ => return Err(unavailable("release list has an unknown target".into())),
            };
            let expected = format!("{FORK_RELEASES}/download/{tag}/herdr-core-{target}.{ext}");
            if artifact.url != expected {
                return Err(unavailable("release list points outside its fork build".into()));
            }
        }
    }
    Ok(())
}

fn curl(url: &str, to: &Path) -> Result<(), RpcError> {
    // Quiet: our error says what failed, and piped stderr is one JSON error.
    let ok = Command::new("curl")
        .args(["-fsL", "--retry", "2", "--connect-timeout", "10", "-o"])
        .arg(to)
        .arg(url)
        .stderr(std::process::Stdio::null())
        .status();
    match ok {
        Ok(s) if s.success() => Ok(()),
        _ => Err(unavailable(format!("cannot download {url}"))),
    }
}

/// The latest release's list, or one version's.
pub fn manifest(version: Option<&str>, scratch: &Path) -> Result<Manifest, RpcError> {
    let mirror = std::env::var("HERDR_RELEASES_URL").ok();
    let url = manifest_url(version, mirror.as_deref())?;
    let file = scratch.join("manifest.json");
    curl(&url, &file)?;
    let bytes = std::fs::read(&file).map_err(|e| unavailable(e.to_string()))?;
    let manifest =
        serde_json::from_slice(&bytes).map_err(|e| unavailable(format!("{url} is not a release list: {e}")))?;
    validate_manifest(&manifest, version, mirror.is_none())?;
    Ok(manifest)
}

/// Windows' own tar opens zips; Git Bash's GNU tar, often first on PATH there, can't.
fn tar_program() -> PathBuf {
    match std::env::var_os("SystemRoot") {
        Some(root) if cfg!(windows) => Path::new(&root).join(r"System32\tar.exe"),
        _ => "tar".into(),
    }
}

#[cfg(windows)]
fn sha256(file: &Path) -> Option<String> {
    crate::platform::sha256(&std::fs::read(file).ok()?)
}

#[cfg(unix)]
fn sha256(file: &Path) -> Option<String> {
    let run =
        |cmd: &str, args: &[&str]| Command::new(cmd).args(args).arg(file).output().ok().filter(|o| o.status.success());
    let out = run("sha256sum", &[]).or_else(|| run("shasum", &["-a", "256"]))?;
    String::from_utf8_lossy(&out.stdout).split_whitespace().next().map(String::from)
}

/// Downloads `m`'s build for `target` (`linux-x86_64`) into `scratch`,
/// checks it against the list, unpacks it: the path of its herdr-core.
/// Windows builds are a zip (the exe and its ConPTY); Windows' own tar
/// unpacks it.
pub fn download(m: &Manifest, target: &str, scratch: &Path, bundle: bool) -> Result<PathBuf, RpcError> {
    let tar = fetch(m, target, scratch)?;
    let exe = if target.starts_with("windows") { "herdr-core.exe" } else { "herdr-core" };
    let ok =
        Command::new(tar_program()).arg("-xf").arg(&tar).arg("-C").arg(scratch).status().is_ok_and(|s| s.success());
    // Root executable remains in macOS archives for old updaters. New ones
    // carry the entire signed bundle through installation instead.
    let bundled = scratch.join("Herdr Core.app/Contents/MacOS/herdr-core");
    let bin = if bundle && target.starts_with("macos-") && bundled.is_file() { bundled } else { scratch.join(exe) };
    if !ok || !bin.is_file() {
        return Err(RpcError::new(
            "refused",
            format!("hc {}'s download for {target} holds no herdr-core", m.version),
            "",
        ));
    }
    crate::platform::verify_release_bundle(&bin)
        .map_err(|e| RpcError::new("refused", format!("invalid release signature: {e}"), ""))?;
    Ok(bin)
}

/// Downloads `m`'s archive for `target` into `scratch` and checks it against
/// the list: its path, still packed.
pub fn fetch(m: &Manifest, target: &str, scratch: &Path) -> Result<PathBuf, RpcError> {
    let t = m.targets.get(target).ok_or_else(|| {
        RpcError::new("refused", format!("hc {} has no build for {target}", m.version), "install hc there yourself")
    })?;
    let archive = if target.starts_with("windows") { "zip" } else { "tar.gz" };
    let file = scratch.join(format!("herdr-core-{target}.{archive}"));
    curl(&t.url, &file)?;
    if sha256(&file).as_deref() != Some(t.sha256.as_str()) {
        return Err(RpcError::new(
            "refused",
            format!("the download of hc {} for {target} is damaged (checksum mismatch)", m.version),
            "try again",
        ));
    }
    Ok(file)
}

/// A private folder for one download, removed when dropped.
pub struct Scratch(pub PathBuf);

impl Scratch {
    pub fn new() -> Result<Scratch, RpcError> {
        let dir =
            std::env::temp_dir().join(format!("hc-download-{}-{}", std::process::id(), crate::platform::random_hex()));
        std::fs::create_dir_all(&dir).map_err(|e| RpcError::internal(e.to_string()))?;
        Ok(Scratch(dir))
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// This machine as release targets name it: `linux-x86_64`, `macos-aarch64`.
pub fn this_target() -> String {
    format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH)
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fork_manifests_bind_every_download_to_the_identified_release() {
        let valid: Manifest = serde_json::from_value(serde_json::json!({
            "version": "0.1.21-fork.18.2", "generation": 1,
            "targets": {"linux-x86_64": {"url": "https://github.com/alexshpunt/herdr-core/releases/download/fork-18-2/herdr-core-linux-x86_64.tar.gz", "sha256": "00"}}
        })).unwrap();
        assert!(validate_manifest(&valid, None, true).is_ok());
        assert!(validate_manifest(&valid, Some("0.1.21-fork.18.1"), true).is_err());
        let mut wrong = valid;
        wrong.targets.get_mut("linux-x86_64").unwrap().url = "https://releases.herdr.dev/core/archive.tar.gz".into();
        assert!(validate_manifest(&wrong, None, true).is_err());
        assert!(validate_manifest(&wrong, None, false).is_ok());
    }

    #[test]
    fn github_urls_use_latest_or_the_immutable_exact_build() {
        let base = "https://github.com/alexshpunt/herdr-core/releases";
        assert_eq!(manifest_url(None, None).unwrap(), format!("{base}/latest/download/manifest.json"));
        assert_eq!(
            manifest_url(Some("0.1.21-fork.18.2"), None).unwrap(),
            format!("{base}/download/fork-18-2/manifest.json")
        );
        assert!(manifest_url(Some("0.1.21"), None).is_err());
        assert!(manifest_url(Some("0.1.21-fork.18.2/../x"), None).is_err());
        assert_eq!(
            manifest_url(Some("0.1.21"), Some("http://localhost:5000/")).unwrap(),
            "http://localhost:5000/core/0.1.21/manifest.json"
        );
    }
}
