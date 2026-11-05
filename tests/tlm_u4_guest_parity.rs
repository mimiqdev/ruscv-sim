//! Blocking TLM U4 (dev-plan D10) and direct-memory U3 (direct-memory D14):
//! guest-suite parity under `--transport tlm` and `--transport tlm --dmi`.
//!
//! Three committed checks live here:
//!
//! * [`the_guest_suite_runs_every_case_under_the_tlm_route`] drives the real
//!   `scripts/run_elf_tests.sh` against a fake simulator. It proves every
//!   guest gets a second pass under `--transport tlm` and a third under
//!   `--transport tlm --dmi`, each with its declared profile and cycle bound,
//!   that the parity set writes `--log-commits` on all three passes, and that
//!   the script compares the complete files as bytes. The fake simulator
//!   records each invocation, so a suite that silently drops the direct pass,
//!   the `--dmi` selection or a profile fails without needing a RISC-V
//!   toolchain.
//! * [`the_guest_suite_fails_when_the_tlm_commit_log_diverges`] and
//!   [`the_guest_suite_fails_when_the_direct_commit_log_diverges`] inject a
//!   whitespace-only divergence after an identical first line, in one pass at
//!   a time, so a first-line-only or whitespace-insensitive comparison, or a
//!   suite that compares the direct pass with the TLM pass instead of native,
//!   fails here.
//! * [`synthetic_guest_commit_logs_match_between_native_and_tlm`] runs one
//!   synthetic guest through the public CLI on both routes and compares the
//!   commit logs byte for byte. This is the target of the
//!   `tlm-u4-transport-diverges` mutation control: a TLM RAM route that
//!   changes a value the guest reads changes this log.
//!
//! The five-guest parity set named by the contract is also enforced against
//! the suite script itself: a guest renamed out of the set, or a comparison
//! weakened to a text diff, fails the invocation checks below.

#[allow(dead_code)]
mod common;

use common::public_elf as elf;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

/// The D10 parity set, in contract order.
const PARITY_GUESTS: [&str; 5] = [
    "hello",
    "s5_timer",
    "s5_external",
    "atomic_tohost_exit",
    "fd_fma",
];

/// Required `rv64i` suite members from `scripts/run_elf_tests.sh`, plus the
/// parity guest `hello`.
const RV64I_GUESTS: [&str; 38] = [
    "trap_ecall",
    "trap_illegal",
    "trap_ebreak",
    "trap_vectored",
    "trap_mret_priv",
    "s6_ebreak",
    "s6_debug_csr",
    "zca_ctrl_flow",
    "zca_link_values",
    "zca_straddle_fetch",
    "zca_ebreak_trap",
    "zca_hints",
    "direct_mem_smc_fence_i",
    "direct_mem_sv39_pt_store",
    "s4_ad_observation",
    "s4_counters",
    "s4_csr_policy",
    "s4_delegation",
    "s4_envcfg",
    "s4_fp_absent",
    "s4_mapped_rw",
    "s4_mprv",
    "s4_permission_fault",
    "s4_pmp_csr",
    "s4_privilege_transition",
    "s4_sfence_vma",
    "s4_sret_vectored",
    "s4_supervisor_interrupt",
    "s4_tvm_tsr_tw",
    "s4_walk_fault",
    "s5_counters",
    "s5_software",
    "s5_timer",
    "s5_external",
    "s5_wfi_event",
    "s5_wfi_masked",
    "s5_wfi_none",
    "hello",
];

/// Required `rv64a` suite members.
const RV64A_GUESTS: [&str; 7] = [
    "lrsc_loop",
    "amo_w",
    "amo_d",
    "aq_rl_set",
    "sc_after_store_failure",
    "atomic_near_ram_end",
    "atomic_tohost_exit",
];

/// Required `rv64ifd` suite members.
const RV64IFD_GUESTS: [&str; 9] = [
    "fd_rounding",
    "fd_flags",
    "fd_nanbox",
    "fd_fma",
    "fd_convert",
    "fd_fs_state",
    "fd_zcd_absent",
    "zcd_transfer",
    "fdc_u6_attach",
];

/// One invocation recorded by the fake simulator.
#[derive(Debug, Clone)]
struct Invocation {
    guest: String,
    isa: String,
    transport: String,
    cycles: String,
    log: String,
    direct_mem: bool,
}

impl Invocation {
    fn parse(line: &str) -> Self {
        let mut fields = line.splitn(6, '|');
        let guest = fields.next().expect("guest field").to_string();
        let isa = fields.next().expect("isa field").to_string();
        let transport = fields.next().expect("transport field").to_string();
        let cycles = fields.next().expect("cycles field").to_string();
        let log = fields.next().expect("log field").to_string();
        let direct_mem = fields.next().expect("direct-memory field") == "dmi";
        Self {
            guest,
            isa,
            transport,
            cycles,
            log,
            direct_mem,
        }
    }
}

/// The profile each guest declares in `scripts/run_elf_tests.sh`, resolved
/// with bash `case` first-match order.
fn declared_profile(guest: &str) -> &'static str {
    if guest.starts_with("s4_") {
        "rv64ima"
    } else if guest.starts_with("zca_") {
        "rv64imac"
    } else if guest.starts_with("fd_") {
        "rv64imafd"
    } else if guest.starts_with("zcd_") || guest.starts_with("fdc_") {
        "rv64imafdc"
    } else {
        ""
    }
}

/// A disposable directory directly below the repository `target/` root.
///
/// `scripts/riscv_elf_paths.sh` accepts a suite output only at that depth, so
/// the harness creates and removes its own tree there.
struct Scratch {
    path: PathBuf,
}

impl Scratch {
    fn new(label: &str) -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join(format!(
                "tlm-u4-{label}-{}-{}",
                std::process::id(),
                COUNTER.fetch_add(1, Ordering::Relaxed)
            ));
        fs::create_dir_all(&path).expect("create scratch directory");
        Self { path }
    }

    fn join(&self, name: &str) -> PathBuf {
        self.path.join(name)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn write_executable(path: &Path, contents: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("create executable parent");
    }
    fs::write(path, contents).expect("write executable");
    let mut permissions = fs::metadata(path)
        .expect("executable metadata")
        .permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions).expect("mark executable");
}

fn write_fixture(path: &Path) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("create fixture parent");
    }
    fs::write(path, b"").expect("write fixture ELF");
}

/// The fixture's fake CLI: records arguments, writes a deterministic commit
/// log, and emulates the two special suite outcomes (`s5_wfi_none`, failure
/// injection).
const FAKE_SIMULATOR: &str = r#"#!/bin/bash
set -u
name=""
transport="native"
direct=""
isa=""
cycles=""
log=""
while [ "$#" -gt 0 ]; do
    case "$1" in
        run) shift; name="$(basename "$1" .elf)"; shift ;;
        --max-cycles) cycles="$2"; shift 2 ;;
        --isa) isa="$2"; shift 2 ;;
        --transport) transport="$2"; shift 2 ;;
        --dmi) direct="dmi"; shift ;;
        --log-commits) log="$2"; shift 2 ;;
        *) shift ;;
    esac
done
printf '%s|%s|%s|%s|%s|%s\n' "$name" "$isa" "$transport" "$cycles" "$log" "$direct" >> "${RUNNER_INVOCATIONS}"
if [ -n "$log" ]; then
    printf 'commit  %s\nscope %s\n' "$name" "$name" > "$log"
    case ",${TLM_DIVERGES:-}," in
        *",${name},"*)
            if [ "$transport" = "tlm" ] && [ -z "$direct" ]; then
                # Divergence after an identical first line, whitespace only:
                # a first-line or whitespace-insensitive comparison misses it.
                printf 'commit  %s\nscope  %s\n' "$name" "$name" > "$log"
            fi
            ;;
    esac
    case ",${DIRECT_DIVERGES:-}," in
        *",${name},"*)
            if [ -n "$direct" ]; then
                printf 'commit  %s\nscope  %s\n' "$name" "$name" > "$log"
            fi
            ;;
    esac
fi
case ",${TLM_FAILS:-}," in
    *",${name},"*)
        if [ "$transport" = "tlm" ]; then
            echo "Status:     FAILED"
            echo "Exit Code:  1"
            exit 1
        fi
        ;;
esac
if [ "$name" = "s5_wfi_none" ]; then
    echo "Error:      NoProgress"
    exit 1
fi
if [ "$name" = "hello" ]; then
    echo "Hello!"
fi
echo "Status:     SUCCESS"
echo "Exit Code:  0"
exit 0
"#;

const FAKE_READELF: &str = r#"#!/bin/bash
echo "Entry point address:               0x80000000"
"#;

const FAKE_NM: &str = r#"#!/bin/bash
echo "0000000080000000 T _start"
"#;

/// Creates the suite fixtures and runs the real `scripts/run_elf_tests.sh`
/// with the fake simulator. `diverges`, `direct_diverges` and `fails` name
/// guests whose TLM pass, direct-memory pass or result must differ, to prove
/// the script reacts.
fn run_guest_suite(
    scratch: &Scratch,
    diverges: Option<&str>,
    direct_diverges: Option<&str>,
    fails: Option<&str>,
) -> Output {
    // The suite output must sit directly below `target/`; this scratch
    // directory is exactly that output directory.
    let outdir = scratch.path.clone();
    let tools = scratch.join("tools");

    for guest in RV64I_GUESTS {
        write_fixture(&outdir.join("rv64i").join(format!("{guest}.elf")));
    }
    for guest in RV64A_GUESTS {
        write_fixture(&outdir.join("rv64a").join(format!("{guest}.elf")));
    }
    for guest in RV64IFD_GUESTS {
        write_fixture(&outdir.join("rv64ifd").join(format!("{guest}.elf")));
    }
    // A non-required rv64m guest exercises the third discovery directory.
    write_fixture(&outdir.join("rv64m").join("mul.elf"));
    fs::write(
        outdir.join("manifest.txt"),
        "source_head=test\nsource_count=54\ncompiled_count=54\n",
    )
    .expect("write manifest");

    write_executable(&tools.join("fake-sim"), FAKE_SIMULATOR);
    write_executable(&tools.join("riscv-readelf"), FAKE_READELF);
    write_executable(&tools.join("riscv-nm"), FAKE_NM);

    let invocations = scratch.join("invocations.txt");
    fs::write(&invocations, b"").expect("reset invocation record");

    let script = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("scripts/run_elf_tests.sh");
    let mut command = Command::new("bash");
    command.arg(script);
    command
        .env("RISCV_TEST_SKIP_BUILD", "1")
        .env("RISCV_TEST_OUTDIR", &outdir)
        .env("RISCV_PREFIX", format!("{}/riscv-", tools.display()))
        .env("RUSCV_SIM_BIN", tools.join("fake-sim"))
        .env("RISCV_TEST_MAX_CYCLES", "123456")
        .env("RUNNER_INVOCATIONS", &invocations);
    if let Some(names) = diverges {
        command.env("TLM_DIVERGES", names);
    }
    if let Some(names) = direct_diverges {
        command.env("DIRECT_DIVERGES", names);
    }
    if let Some(names) = fails {
        command.env("TLM_FAILS", names);
    }
    command.output().expect("run scripts/run_elf_tests.sh")
}

fn recorded(scratch: &Scratch) -> Vec<Invocation> {
    let text = fs::read_to_string(scratch.join("invocations.txt")).expect("invocation record");
    text.lines().map(Invocation::parse).collect()
}

fn diagnostic(output: &Output) -> String {
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let tail = |text: &str| {
        let start = text.len().saturating_sub(40_000);
        text[start..].to_string()
    };
    format!("stdout:\n{}\nstderr:\n{}", tail(&stdout), tail(&stderr))
}

fn expected_guests() -> Vec<String> {
    let mut names: Vec<String> = RV64I_GUESTS.iter().map(|name| name.to_string()).collect();
    names.extend(RV64A_GUESTS.iter().map(|name| name.to_string()));
    names.extend(RV64IFD_GUESTS.iter().map(|name| name.to_string()));
    // A non-required rv64m guest exercises the third discovery directory.
    names.push("mul".to_string());
    names
}

#[test]
fn the_guest_suite_runs_every_case_under_the_tlm_route() {
    let scratch = Scratch::new("runner");
    let output = run_guest_suite(&scratch, None, None, None);
    let text = diagnostic(&output);
    assert!(
        output.status.success(),
        "the guest suite rejected a faithful fake simulator:\n{text}"
    );
    assert!(
        text.contains("TLM parity logs compared: 5/5"),
        "the suite did not report five byte-identical parity logs:\n{text}"
    );
    assert!(
        text.contains("Direct-memory parity logs compared: 5/5"),
        "the suite did not report five byte-identical direct-memory logs:\n{text}"
    );

    let invocations = recorded(&scratch);
    let expected = expected_guests();
    assert_eq!(
        invocations.len(),
        expected.len() * 3,
        "every guest must run exactly three times (native, TLM, direct memory):\n{invocations:#?}"
    );

    for guest in &expected {
        let runs: Vec<&Invocation> = invocations
            .iter()
            .filter(|invocation| &invocation.guest == guest)
            .collect();
        assert_eq!(
            runs.len(),
            3,
            "{guest}: expected one native, one TLM and one direct-memory pass, got {runs:#?}"
        );
        assert_eq!(
            runs[0].transport, "native",
            "{guest}: the first pass must stay on the native route"
        );
        assert!(
            !runs[0].direct_mem,
            "{guest}: the native pass must not select direct memory"
        );
        assert_eq!(
            runs[1].transport, "tlm",
            "{guest}: the second pass must run under --transport tlm"
        );
        assert!(
            !runs[1].direct_mem,
            "{guest}: the second pass must not select direct memory"
        );
        assert_eq!(
            runs[2].transport, "tlm",
            "{guest}: the third pass must run under --transport tlm"
        );
        assert!(
            runs[2].direct_mem,
            "{guest}: the third pass must select --dmi"
        );
        let profile = declared_profile(guest);
        for (index, label) in ["native", "TLM", "direct-memory"].iter().enumerate() {
            assert_eq!(
                runs[index].isa, profile,
                "{guest}: the {label} pass must use the declared profile"
            );
            assert_eq!(
                runs[index].cycles, "123456",
                "{guest}: the {label} pass must use the suite cycle bound"
            );
        }

        if PARITY_GUESTS.contains(&guest.as_str()) {
            assert!(
                runs[0].log.ends_with(&format!("{guest}.native.log"))
                    && runs[1].log.ends_with(&format!("{guest}.tlm.log"))
                    && runs[2].log.ends_with(&format!("{guest}.direct.log")),
                "{guest}: all three parity passes must write commit logs: {runs:#?}"
            );
            let native_log = fs::read(&runs[0].log).expect("native parity log");
            assert!(!native_log.is_empty(), "{guest}: empty native commit log");
            for (index, label) in [(1, "TLM"), (2, "direct-memory")] {
                let log = fs::read(&runs[index].log).expect("parity log");
                assert_eq!(
                    native_log, log,
                    "{guest}: {label} parity commit logs are not byte-identical"
                );
            }
        } else {
            assert!(
                runs[0].log.is_empty() && runs[1].log.is_empty() && runs[2].log.is_empty(),
                "{guest}: only the parity set needs commit logs: {runs:#?}"
            );
        }
    }

    // The contract names exactly these five guests; a rename or an extra
    // member means the checked comparison no longer matches D10.
    let compared: Vec<&String> = invocations
        .iter()
        .filter(|invocation| !invocation.log.is_empty())
        .map(|invocation| &invocation.guest)
        .collect();
    for guest in PARITY_GUESTS {
        assert_eq!(
            compared
                .iter()
                .filter(|name| name.as_str() == guest)
                .count(),
            3,
            "{guest}: the parity set must write one log per pass"
        );
    }
    assert_eq!(
        compared.len(),
        PARITY_GUESTS.len() * 3,
        "unexpected commit-log writers: {compared:?}"
    );
}

#[test]
fn the_guest_suite_fails_when_the_tlm_commit_log_diverges() {
    let scratch = Scratch::new("runner-diverges");
    let output = run_guest_suite(&scratch, Some("fd_fma"), None, None);
    let text = diagnostic(&output);
    assert!(
        !output.status.success(),
        "a diverging TLM commit log passed the suite:\n{text}"
    );
    assert!(
        text.contains("fd_fma: --transport tlm commit log differs from native"),
        "the suite did not name the diverging parity guest:\n{text}"
    );

    // The native and TLM passes still ran; the failure is that comparison,
    // which stops this guest before its direct-memory pass. The injected logs
    // share their first line and differ only in whitespace, so a
    // first-line-only or whitespace-insensitive comparison would wrongly pass.
    let invocations = recorded(&scratch);
    let runs: Vec<&Invocation> = invocations
        .iter()
        .filter(|invocation| invocation.guest == "fd_fma")
        .collect();
    assert_eq!(runs.len(), 2, "fd_fma passes: {runs:#?}");
    assert_eq!(runs[1].transport, "tlm");
    assert!(!runs[1].direct_mem);
    let native_log = fs::read_to_string(&runs[0].log).expect("native fd_fma log");
    let tlm_log = fs::read_to_string(&runs[1].log).expect("tlm fd_fma log");
    assert_eq!(
        native_log.lines().next(),
        tlm_log.lines().next(),
        "the injected divergence must follow an identical first line"
    );
    assert_eq!(
        native_log.split_whitespace().collect::<Vec<_>>(),
        tlm_log.split_whitespace().collect::<Vec<_>>(),
        "the injected divergence must be whitespace only"
    );
    assert_ne!(native_log, tlm_log, "the injected logs must differ");
}

#[test]
fn the_guest_suite_fails_when_the_direct_commit_log_diverges() {
    let scratch = Scratch::new("runner-direct-diverges");
    let output = run_guest_suite(&scratch, None, Some("fd_fma"), None);
    let text = diagnostic(&output);
    assert!(
        !output.status.success(),
        "a diverging direct-memory commit log passed the suite:\n{text}"
    );
    assert!(
        text.contains("fd_fma: --transport tlm --dmi commit log differs from native"),
        "the suite did not name the diverging direct-memory parity guest:\n{text}"
    );
    assert!(
        text.contains("Direct-memory parity logs compared: 4/5"),
        "the suite did not count the missing direct-memory comparison:\n{text}"
    );

    // The comparison is against native, not against the TLM pass: that pass
    // is still byte-identical here, so a suite that compared the third pass
    // with the second would accept the divergence.
    let invocations = recorded(&scratch);
    let runs: Vec<&Invocation> = invocations
        .iter()
        .filter(|invocation| invocation.guest == "fd_fma")
        .collect();
    assert_eq!(runs.len(), 3, "fd_fma passes: {runs:#?}");
    assert!(runs[2].direct_mem, "the third pass must select --dmi");
    let native_log = fs::read_to_string(&runs[0].log).expect("native fd_fma log");
    let tlm_log = fs::read_to_string(&runs[1].log).expect("tlm fd_fma log");
    let direct_log = fs::read_to_string(&runs[2].log).expect("direct fd_fma log");
    assert_eq!(
        native_log, tlm_log,
        "the injected divergence must not reach the TLM pass"
    );
    assert_eq!(
        native_log.lines().next(),
        direct_log.lines().next(),
        "the injected divergence must follow an identical first line"
    );
    assert_eq!(
        native_log.split_whitespace().collect::<Vec<_>>(),
        direct_log.split_whitespace().collect::<Vec<_>>(),
        "the injected divergence must be whitespace only"
    );
    assert_ne!(native_log, direct_log, "the injected logs must differ");
}

#[test]
fn the_guest_suite_fails_when_a_guest_fails_only_under_tlm() {
    let scratch = Scratch::new("runner-fails");
    let output = run_guest_suite(&scratch, None, None, Some("s5_timer"));
    let text = diagnostic(&output);
    assert!(
        !output.status.success(),
        "a TLM-only guest failure passed the suite:\n{text}"
    );
    assert!(
        text.contains("s5_timer: [FAIL] --transport tlm")
            || text.contains("[FAIL] --transport tlm: CLI exit status 1, expected 0"),
        "the suite did not classify the TLM pass as failed:\n{text}"
    );
}

/// A synthetic guest that stores, reloads, performs one AMO and one fused
/// multiply-add whose result is stored to RAM. The commit log therefore
/// records values read back through the selected RAM route and the FPR effect,
/// so a route that changes data or arithmetic changes the log.
fn synthetic_guest() -> Vec<u8> {
    // csrs mstatus, x14 with x14 = 0x6000 enables FS=Dirty for F/D.
    let enable_fs = (0x300 << 20) | (14 << 15) | (2 << 12) | 0x73;
    // csrw fcsr, x0 clears the FP flags before the fused operation.
    let clear_fcsr = (0x003 << 20) | (1 << 12) | 0x73;
    // fmv.w.x f2, x0 makes the fused operands deterministic zeroes.
    let fmv_w_x_f2_x0 = (0x78 << 25) | (2 << 7) | 0x53;
    // fmadd.s f1, f2, f2, f2, rne -> +0.0f.
    let fmadd_s = (2 << 27) | (2 << 20) | (2 << 15) | (1 << 7) | 0x43;
    // fsw f1, 8(x6).
    let fsw = (1 << 20) | (6 << 15) | (2 << 12) | (8 << 7) | 0x27;
    // amoadd.d x10, x9, (x6).
    let amoadd_d = (9 << 20) | (6 << 15) | (3 << 12) | (10 << 7) | 0x2f;
    let code = vec![
        elf::auipc(6, 3),       // scratch = 0x8000_3000
        elf::addi(7, 0, 0x123), // stored value
        elf::sd(7, 6, 0),       //
        elf::ld(8, 6, 0),       // x8 = 0x123 through the route
        elf::addi(9, 8, 1),     // x9 = 0x124
        amoadd_d,               // x10 = 0x123, RAM = 0x247
        elf::ld(11, 6, 0),      // x11 = 0x247
        elf::lui(14, 0x6),      // FS = Dirty
        enable_fs,              //
        clear_fcsr,             //
        fmv_w_x_f2_x0,          // f2 = +0.0f
        fmadd_s,                // f1 = +0.0f
        fsw,                    // store the raw FP bits
        elf::ld(12, 6, 8),      // x12 = 0x0000_0000
        elf::standard_exit(0),  // x5 = exit payload
        elf::lui(4, 0x40008),   // HTIF base
        elf::sd(5, 4, 0),       // exit
    ];
    elf::elf_with_code(&code, 0, true, false, 0x4000)
}

fn run_cli(path: &Path, transport: &str, log: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_ruscv-sim"))
        .arg("run")
        .arg(path)
        .args(["--max-cycles", "64"])
        .args(["--transport", transport])
        .args(["--log-commits", log.to_str().expect("log path")])
        .output()
        .expect("run ruscv-sim")
}

#[test]
fn synthetic_guest_commit_logs_match_between_native_and_tlm() {
    let scratch = Scratch::new("cli");
    let elf_path = scratch.join("parity.elf");
    fs::write(&elf_path, synthetic_guest()).expect("write synthetic guest");
    let native_log_path = scratch.join("native.log");
    let tlm_log_path = scratch.join("tlm.log");

    let native = run_cli(&elf_path, "native", &native_log_path);
    let tlm = run_cli(&elf_path, "tlm", &tlm_log_path);
    for (label, output) in [("native", &native), ("tlm", &tlm)] {
        let text = diagnostic(output);
        assert!(
            output.status.success(),
            "{label} route failed on the synthetic guest:\n{text}"
        );
        assert!(
            String::from_utf8_lossy(&output.stdout).contains("Status:     SUCCESS"),
            "{label} route did not report success:\n{text}"
        );
    }

    let native_log = fs::read_to_string(&native_log_path).expect("native commit log");
    let tlm_log = fs::read_to_string(&tlm_log_path).expect("tlm commit log");
    assert!(!native_log.is_empty(), "empty native commit log");
    for effect in [
        "x8  0x0000000000000123",
        "x10  0x0000000000000123",
        "x11  0x0000000000000247",
        " f1 ",
    ] {
        assert!(
            native_log.contains(effect),
            "the synthetic guest did not exercise {effect}:\n{native_log}"
        );
    }
    assert_eq!(
        native_log, tlm_log,
        "the TLM route's commit log diverged from native"
    );
}
