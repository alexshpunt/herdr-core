//! A pseudo console has no foreground process group. What stands in for one:
//! the newest live program a known shell root started itself. A directly
//! launched program keeps its place when it starts tools or helper processes.

use std::mem::{size_of, zeroed};
use std::os::windows::io::{FromRawHandle, OwnedHandle};
use std::ptr::null_mut;
use windows_sys::Wdk::System::Threading::{NtQueryInformationProcess, ProcessBasicInformation};
use windows_sys::Win32::Foundation::{FILETIME, HANDLE};
use windows_sys::Win32::System::JobObjects::{JobObjectBasicProcessIdList, QueryInformationJobObject};
use windows_sys::Win32::System::Threading::*;

/// A process in the terminal's job.
struct Proc {
    pid: u32,
    parent: u32,
    started: u64,
    name: String,
}

/// The pid in front of terminal `root`, whose job holds everything it started.
pub(super) fn in_front(job: HANDLE, root: u32) -> u32 {
    newest_child(root, &unsafe { job_processes(job) }).unwrap_or(root)
}

fn is_shell(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    matches!(
        name.strip_suffix(".exe").unwrap_or(&name),
        "zsh"
            | "bash"
            | "fish"
            | "sh"
            | "dash"
            | "nu"
            | "pwsh"
            | "powershell"
            | "cmd"
            | "elvish"
            | "xonsh"
            | "tcsh"
            | "csh"
            | "ksh"
            | "ion"
    )
}

/// A known shell root's newest child, console hosts aside.
fn newest_child(root: u32, procs: &[Proc]) -> Option<u32> {
    if !procs.iter().any(|p| p.pid == root && is_shell(&p.name)) {
        return None;
    }
    let host = |name: &str| ["conhost.exe", "OpenConsole.exe"].iter().any(|h| name.eq_ignore_ascii_case(h));
    procs
        .iter()
        .filter(|p| p.parent == root && p.pid != root && !host(&p.name))
        .max_by_key(|p| p.started)
        .map(|p| p.pid)
}

/// Whether an identified reporter is reached by following foreground shell jobs.
fn reporter_in_front(root: u32, reporter: u32, started: u64, procs: &[Proc]) -> bool {
    let Some(reporter_proc) = procs.iter().find(|p| p.pid == reporter && p.started == started) else { return false };
    let mut current = root;
    for _ in 0..procs.len() {
        let Some(parent) = procs.iter().find(|p| p.pid == current) else { return false };
        let Some(child) = newest_child(current, procs).and_then(|id| procs.iter().find(|p| p.pid == id)) else {
            return current == reporter_proc.pid;
        };
        if child.started < parent.started {
            return false;
        }
        current = child.pid;
    }
    false
}

/// Checks the reported process against a complete observation of the job.
pub(super) fn reporter(job: HANDLE, root: u32, pid: u32) -> Option<bool> {
    let (ids, complete) = job_pids(job);
    if !complete {
        return None;
    }
    let processes: Option<Vec<Proc>> = ids.into_iter().map(|id| unsafe { about(id) }).collect();
    let processes = processes?;
    let Some(process) = processes.iter().find(|p| p.pid == pid) else { return Some(false) };
    Some(reporter_in_front(root, pid, process.started, &processes))
}

/// Returns the available process IDs and whether the complete job fit.
fn job_pids(job: HANDLE) -> (Vec<u32>, bool) {
    #[repr(C)]
    struct Ids {
        assigned: u32,
        listed: u32,
        ids: [usize; 64],
    }
    let mut list: Ids = unsafe { zeroed() };
    let ok = unsafe {
        QueryInformationJobObject(
            job,
            JobObjectBasicProcessIdList,
            (&raw mut list).cast(),
            size_of::<Ids>() as u32,
            null_mut(),
        )
    };
    let n = (list.listed as usize).min(list.ids.len());
    let complete = ok != 0 && list.assigned == list.listed && list.listed as usize <= list.ids.len();
    (list.ids[..n].iter().map(|&id| id as u32).collect(), complete)
}

/// Existing foreground observation keeps partial results when the job is large.
unsafe fn job_processes(job: HANDLE) -> Vec<Proc> {
    job_pids(job).0.into_iter().filter_map(|pid| unsafe { about(pid) }).collect()
}

unsafe fn about(pid: u32) -> Option<Proc> {
    unsafe {
        let raw = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if raw.is_null() {
            return None;
        }
        let _process = OwnedHandle::from_raw_handle(raw);
        let h = raw as HANDLE;
        let mut basic: PROCESS_BASIC_INFORMATION = zeroed();
        let size = size_of::<PROCESS_BASIC_INFORMATION>() as u32;
        if NtQueryInformationProcess(h, ProcessBasicInformation, (&raw mut basic).cast(), size, null_mut()) != 0 {
            return None;
        }
        let [mut started, mut exited, mut kernel, mut user]: [FILETIME; 4] = zeroed();
        if GetProcessTimes(h, &mut started, &mut exited, &mut kernel, &mut user) == 0 {
            return None;
        }
        let mut path = [0u16; 1024];
        let mut len = path.len() as u32;
        if QueryFullProcessImageNameW(h, PROCESS_NAME_WIN32, path.as_mut_ptr(), &mut len) == 0 {
            return None;
        }
        let path = String::from_utf16_lossy(&path[..len as usize]);
        Some(Proc {
            pid,
            parent: basic.InheritedFromUniqueProcessId as u32,
            started: (started.dwHighDateTime as u64) << 32 | started.dwLowDateTime as u64,
            name: path.rsplit('\\').next().unwrap_or_default().to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_newest_program_the_shell_started_is_in_front() {
        let p = |pid, parent, started, name: &str| Proc { pid, parent, started, name: name.into() };
        let shell = p(10, 1, 1, "powershell.exe");
        assert_eq!(newest_child(10, &[p(10, 1, 1, "powershell.exe")]), None, "only the shell: it is in front");
        let procs = [
            shell,
            p(11, 10, 2, "node.exe"),
            p(12, 10, 3, "claude.exe"),
            p(13, 10, 4, "conhost.exe"),
            p(14, 12, 5, "node.exe"), // the agent's own helper
        ];
        assert_eq!(newest_child(10, &procs), Some(12));
    }

    #[test]
    fn a_real_process_behind_cmd_is_observed_until_it_exits() {
        use super::super::windows::{ProcessIdentity, spawn_pty, wait};
        use std::time::{Duration, Instant};
        let root = std::env::temp_dir().join(format!("hc-provenance-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let script = root.join("child.ps1");
        let pid_file = root.join("pid.txt");
        std::fs::write(
            &script,
            format!("[IO.File]::WriteAllText('{}', [string]$PID); Start-Sleep 30", pid_file.display()),
        )
        .unwrap();
        let argv = vec![
            "cmd.exe".into(),
            "/d".into(),
            "/c".into(),
            "powershell.exe".into(),
            "-NoProfile".into(),
            "-File".into(),
            script.display().to_string(),
        ];
        let env = std::env::vars().collect();
        let (pty, mut shell) = spawn_pty(&argv, &env, root.to_str().unwrap(), 80, 24).unwrap();
        let limit = Instant::now() + Duration::from_secs(10);
        let pid = loop {
            if let Ok(text) = std::fs::read_to_string(&pid_file)
                && let Ok(pid) = text.trim().parse::<i32>()
            {
                break pid;
            }
            assert!(Instant::now() < limit, "child did not start");
            std::thread::sleep(Duration::from_millis(20));
        };
        let identity = ProcessIdentity::open(pid).unwrap();
        let observed = pty.process_in_front(pid, &identity);
        let outside_pid = std::process::id() as i32;
        let outside_identity = ProcessIdentity::open(outside_pid).unwrap();
        let outside = pty.process_in_front(outside_pid, &outside_identity);
        pty.signal(shell.id(), super::super::signal::KILL);
        wait(&mut shell).unwrap();
        let limit = Instant::now() + Duration::from_secs(5);
        while identity.is_alive() && Instant::now() < limit {
            std::thread::sleep(Duration::from_millis(20));
        }
        let ended = identity.is_alive();
        drop(pty);
        std::fs::remove_dir_all(root).unwrap();
        assert_ne!(pid, shell.id() as i32);
        assert_eq!(observed, Some(true));
        assert_eq!(outside, Some(false));
        assert!(!ended);
    }
    #[test]
    fn process_identity_tracks_the_process_after_it_exits() {
        let mut child = std::process::Command::new("powershell.exe")
            .args(["-NoProfile", "-Command", "Start-Sleep 30"])
            .spawn()
            .unwrap();
        let identity = super::super::windows::ProcessIdentity::open(child.id() as i32).unwrap();
        assert!(identity.is_alive());
        child.kill().unwrap();
        child.wait().unwrap();
        assert!(!identity.is_alive());
    }
    #[test]
    fn a_reporter_in_front_can_be_behind_shell_wrappers_but_not_a_background_sibling() {
        let p = |pid, parent, started, name: &str| Proc { pid, parent, started, name: name.into() };
        let processes = [
            p(10, 1, 1, "powershell.exe"),
            p(11, 10, 2, "cmd.exe"),
            p(12, 11, 3, "bash.exe"),
            p(13, 12, 4, "node.exe"),
            p(14, 10, 1, "other.exe"),
            p(15, 13, 5, "worker.exe"),
        ];
        assert!(reporter_in_front(10, 13, 4, &processes));
        assert!(!reporter_in_front(10, 10, 1, &processes));
        assert!(!reporter_in_front(10, 11, 2, &processes));
        assert!(!reporter_in_front(10, 13, 99, &processes));
        assert!(!reporter_in_front(10, 14, 1, &processes));
        assert!(!reporter_in_front(10, 15, 5, &processes));
    }
    #[test]
    fn a_direct_program_keeps_its_place_when_it_starts_a_helper() {
        let p = |pid, parent, name: &str| Proc { pid, parent, started: pid as u64, name: name.into() };
        assert_eq!(newest_child(10, &[p(10, 1, "node.exe"), p(11, 10, "ping.exe")]), None);
        for shell in ["cmd.exe", "PwSh.EXE", "powershell.exe", "nu.exe", "bash", "sh.exe"] {
            assert_eq!(newest_child(10, &[p(10, 1, shell), p(11, 10, "node.exe")]), Some(11), "{shell}");
        }
    }
}
