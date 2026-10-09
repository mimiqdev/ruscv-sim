//! Untimed exact-caller-thread Linux evidence; never physical-core affinity.
use crate::phases::digest;
use serde_json::{json, Value};
use std::{
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};

pub fn caller() -> Option<Value> {
    if !cfg!(target_os = "linux") {
        return None;
    }
    let mut files = serde_json::Map::new();
    for (name, path) in [
        ("status", "/proc/thread-self/status"),
        ("cgroup", "/proc/thread-self/cgroup"),
        ("online", "/sys/devices/system/cpu/online"),
        ("cpuset", "/sys/fs/cgroup/cpuset.cpus.effective"),
        ("cpu_max", "/sys/fs/cgroup/cpu.max"),
        ("memory_max", "/sys/fs/cgroup/memory.max"),
        ("boot_id", "/proc/sys/kernel/random/boot_id"),
    ] {
        let item = match std::fs::read_to_string(path) {
            Ok(value) => {
                json!({"path":path,"sha256":digest::sha256(value.as_bytes()),"value":value,"reason":null})
            }
            Err(error) => {
                json!({"path":path,"sha256":null,"value":null,"reason":error.to_string()})
            }
        };
        files.insert(name.into(), item);
    }
    let status = files["status"]["value"].as_str()?;
    let field = |name: &str| {
        status
            .lines()
            .find_map(|line| line.strip_prefix(name))
            .map(str::trim)
    };
    let tid: u32 = field("Pid:")?.parse().ok()?;
    let pid: u32 = field("Tgid:")?.parse().ok()?;
    let affinity = field("Cpus_allowed_list:")?.to_owned();
    let namespace = std::fs::read_link("/proc/thread-self/ns/pid")
        .ok()?
        .to_string_lossy()
        .into_owned();
    let mut namespaces = serde_json::Map::new();
    for kind in ["pid", "mnt", "user", "cgroup", "uts", "ipc", "net", "time"] {
        let path = format!("/proc/thread-self/ns/{kind}");
        let link = std::fs::read_link(&path)
            .ok()?
            .to_string_lossy()
            .into_owned();
        namespaces.insert(kind.into(), Value::String(link));
    }
    let keys = [
        "LD_PRELOAD",
        "LD_AUDIT",
        "LD_LIBRARY_PATH",
        "LD_ORIGIN_PATH",
        "GLIBC_TUNABLES",
    ];
    let mut injection: serde_json::Map<_, _> = keys
        .into_iter()
        .map(|k| {
            (
                k.into(),
                std::env::var(k)
                    .ok()
                    .map(Value::String)
                    .unwrap_or(Value::Null),
            )
        })
        .collect();
    injection.insert(
        "other".into(),
        json!(std::env::vars()
            .filter(|(k, _)| k.starts_with("LD_") && !keys.contains(&k.as_str()))
            .map(|(name, value)| json!({"name":name,"value":value}))
            .collect::<Vec<_>>()),
    );
    Some(
        json!({"method":"linux-thread-self-proc/1","pid":pid,"tid":tid,"pid_namespace":namespace,
        "utc_unix_ns":SystemTime::now().duration_since(UNIX_EPOCH).ok()?.as_nanos(),
        "affinity_vcpu_list":affinity,"namespaces":namespaces,"files":files,"injection":injection}),
    )
}

pub fn cli_proof(
    before: Option<Value>,
    after: Option<Value>,
    argv: &[String],
    executable: &Path,
) -> Value {
    let binary = std::fs::read(executable).ok();
    json!({"kind":"linux-caller-affinity-inheritance/1","caller_before":before,"caller_after":after,
        "argv":argv,"launcher":"std::process::Command::output; direct executable; no shell/pre_exec/spawn affinity attributes",
        "launcher_source_sha256":digest::sha256(include_bytes!("phases.rs")),
        "cli_sha256":binary.as_ref().map(|b|digest::sha256(b)),
        "derived_initial_vcpu_mask":before.as_ref().and_then(|v|v["affinity_vcpu_list"].as_str()),
        "direct_child_readback":{"value":null,"reason":"not attempted: caller inspections remain outside direct CLI launch-through-wait/reap interval"},
        "child_pid":{"value":null,"reason":"Command::output does not expose the child PID"},
        "guarantee":"Linux process-creation/exec CPU-mask inheritance; per-thread caller, cpuset and online CPUs restrict placement; no observed fork syscall or sustained child-mask readback claimed",
        "sources":["https://man7.org/linux/man-pages/man2/sched_setaffinity.2.html","https://man7.org/linux/man-pages/man7/cpuset.7.html","https://man7.org/linux/man-pages/man3/posix_spawn.3.html"],
        "limitations":["VM vCPU mask, not physical-core mapping","physical affinity/governor/turbo remain unobserved/uncontrolled where unavailable","no sustained equal scheduling/frequency guarantee"]})
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;
    use std::process::Command;
    unsafe extern "C" {
        fn sched_getaffinity(pid: i32, size: usize, mask: *mut u8) -> i32;
        fn sched_setaffinity(pid: i32, size: usize, mask: *const u8) -> i32;
    }
    struct Restore([u8; 128]);
    impl Drop for Restore {
        fn drop(&mut self) {
            // pid=0 is THIS test thread only; never global/physical VM policy.
            assert_eq!(
                unsafe { sched_setaffinity(0, self.0.len(), self.0.as_ptr()) },
                0
            );
        }
    }
    #[test]
    fn exact_command_output_helper_inherits_this_threads_mask_not_another_threads() {
        let mut original = [0u8; 128];
        assert_eq!(
            unsafe { sched_getaffinity(0, original.len(), original.as_mut_ptr()) },
            0
        );
        let _restore = Restore(original);
        let cpu = original
            .iter()
            .enumerate()
            .find_map(|(i, b)| {
                (0..8)
                    .find(|bit| b & (1 << bit) != 0)
                    .map(|bit| i * 8 + bit)
            })
            .expect("allowed CPU");
        let mut restricted = [0u8; 128];
        restricted[cpu / 8] = 1 << (cpu % 8);
        assert_eq!(
            unsafe { sched_setaffinity(0, restricted.len(), restricted.as_ptr()) },
            0
        );
        let before = caller().expect("own Linux caller");
        let helper = Command::new("python3")
            .args([
                "-B",
                "-c",
                "import os,json; print(json.dumps(sorted(os.sched_getaffinity(0))))",
            ])
            .output()
            .unwrap();
        let after = caller().expect("own caller after helper");
        assert!(helper.status.success());
        assert_eq!(
            serde_json::from_slice::<Vec<usize>>(&helper.stdout).unwrap(),
            vec![cpu]
        );
        assert_eq!(before["affinity_vcpu_list"], cpu.to_string());
        assert_eq!(before["tid"], after["tid"]);
        assert_eq!(before["affinity_vcpu_list"], after["affinity_vcpu_list"]);
        assert_ne!(
            before["tid"], before["pid"],
            "libtest uses a distinct calling thread; /proc/self main-thread data is not proof"
        );
        // Helper validates ONLY Linux launcher inheritance. It is not the
        // benchmark CLI and supplies no guest/result/ISA correctness evidence.
    }
}
