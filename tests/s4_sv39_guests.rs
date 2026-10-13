//! Stage 4 translated guest-ELF integration (dev-plan §4, D2/D3/D4/D8/D9).
//!
//! This test deliberately assembles and links the Sv39 guests into a
//! temporary directory for every invocation, exactly like the A6 trap
//! guests, and runs each through the three public routes: the `ruscv-sim
//! run` CLI child, the native library entry (`load_and_run`), and the flat
//! facade (`RiscVSimulator`).  A pre-existing ELF is never accepted.
//!
//! `s4_mapped_rw` proves the mapped read/write family: an M-mode boot
//! builds Sv39 tables, mret enters S, translated fetch runs from a 4 KiB
//! page, loads and stores go through a 4 KiB mapping and a 1 GiB superpage,
//! the untouched neighbor stays untouched, the A/D bits the hardware set
//! are checked in the leaf PTEs, and all four SFENCE.VMA forms retire with
//! translation still working (M4).
//!
//! `s4_permission_fault` proves the permission-fault family: a store to a
//! read-only PTE, a fetch from a non-executable PTE, an S-mode load of a
//! U=1 page with SUM=0, and a load of an execute-only page with MXR=0 each
//! raise the page fault of the original kind with the original virtual
//! address in mtval, checked exactly by the M-mode handler, and the same
//! execute-only load with MXR=1 retires with the right value.
//!
//! `s4_mprv` proves the MPRV family (D9): with MPRV=1 and MPP=S in M mode,
//! a load of a U=1 page succeeds when SUM=1 (SUM applies under MPRV), the
//! same load takes the load page fault with the virtual address in mtval
//! when SUM=0, a store lands at the translated address with the neighbor
//! untouched, and instruction fetch never uses the translation (the whole
//! program runs at its bare physical address).
//!
//! `s4_ad_observation` proves the two A/D cases a guest can raise (D4): a
//! fresh page sets A after a read and A and D after a write, and a leaf
//! whose PPN sits below RAM commits that A/D write and then takes the load
//! access fault. The rejected write-back is a driver case, because no
//! public target serves a PTE and then refuses the conditional store.
//!
//! `s4_sret_vectored` proves two interactions. A load page fault taken
//! with vectored mtvec enters at BASE, and the slots at BASE + 4*cause are
//! failure paths. SRET from S returns to U with MPRV clear, SPIE set, and
//! SEPC bits 1:0 clear, and the same instruction in U is illegal.
//!
//! `s4_sfence_vma` proves the SFENCE.VMA family (D8, M4): S mode rewrites a
//! leaf PTE before each of the four rs1/rs2 forms and then loads (or
//! stores) through the newly written mapping, a previously invalid leaf
//! becomes usable after a per-VA fence, and a non-canonical rs1 raises no
//! exception. The rewritten leaf is also visible before the fence; that is
//! recorded as a property of this cache-less model, not as an architectural
//! requirement. SFENCE.VMA in U is an illegal-instruction trap with mtval
//! equal to the instruction bits, as the specification requires. HFENCE.VVMA
//! in S also traps with mtval equal to its bits; with H absent that encoding
//! is reserved, so the trap is asserted as consistency with the Spike and
//! Sail reference models, not as a specification requirement.

use ruscv_sim::executor::{load_and_run, RiscVSimulator};
use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::TempDir;

const GUESTS: [&str; 7] = [
    "s4_mapped_rw",
    "s4_permission_fault",
    "s4_mprv",
    "s4_walk_fault",
    "s4_ad_observation",
    "s4_sret_vectored",
    "s4_sfence_vma",
];
const LINKER_SCRIPT: &str = "linker-s4.ld";
const MAX_CYCLES: u64 = 100_000;

fn command_available(command: &str) -> bool {
    Command::new(command)
        .arg("--version")
        .output()
        .is_ok_and(|output| output.status.success())
}

fn required_toolchain() -> Option<(String, String)> {
    let prefix = env::var("RISCV_PREFIX").unwrap_or_else(|_| "riscv64-unknown-elf-".to_string());
    let assembler = format!("{prefix}as");
    let linker = format!("{prefix}ld");
    if command_available(&assembler) && command_available(&linker) {
        Some((assembler, linker))
    } else {
        None
    }
}

fn assemble_and_link(
    assembler: &str,
    linker: &str,
    source: &Path,
    linker_script: &Path,
    output_dir: &Path,
    name: &str,
) -> PathBuf {
    let object = output_dir.join(format!("{name}.o"));
    let elf = output_dir.join(format!("{name}.elf"));

    let assembled = Command::new(assembler)
        .args(["-march=rv64ima_zicsr", "-mabi=lp64"])
        .arg(source)
        .arg("-o")
        .arg(&object)
        .output()
        .unwrap_or_else(|error| panic!("failed to invoke {assembler}: {error}"));
    assert!(
        assembled.status.success(),
        "assembling {} failed:\n{}",
        source.display(),
        String::from_utf8_lossy(&assembled.stderr)
    );

    let linked = Command::new(linker)
        .arg(format!("-T{}", linker_script.display()))
        .arg(&object)
        .arg("-o")
        .arg(&elf)
        .output()
        .unwrap_or_else(|error| panic!("failed to invoke {linker}: {error}"));
    assert!(
        linked.status.success(),
        "linking {} failed:\n{}",
        source.display(),
        String::from_utf8_lossy(&linked.stderr)
    );
    assert!(elf.is_file(), "linker did not produce {}", elf.display());
    elf
}

fn cli_binary() -> PathBuf {
    // Cargo supplies this compile-time path for integration tests.  Do not
    // fall back to target/debug: isolated CARGO_TARGET_DIR builds must never
    // accidentally execute a stale default-target binary.
    PathBuf::from(env!("CARGO_BIN_EXE_ruscv-sim"))
}

#[test]
fn s4_sv39_guests_pass_through_cli_library_and_flat_facade() {
    let Some((assembler, linker)) = required_toolchain() else {
        let message = "riscv64-unknown-elf-as/ld unavailable; S4 Sv39 guest integration skipped";
        if env::var_os("RISCV_REQUIRE_RISCV_TOOLCHAIN").is_some() {
            panic!("{message}; CI requested a real guest run");
        }
        eprintln!("[SKIP] {message}");
        return;
    };

    let repository = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let source_dir = repository.join("tests/bare-metal-riscv-test/rv64i");
    let linker_script = repository
        .join("tests/bare-metal-riscv-test")
        .join(LINKER_SCRIPT);
    let temp = TempDir::new().expect("temporary S4 guest build directory");
    let cli = cli_binary();

    for name in GUESTS {
        let source = source_dir.join(format!("{name}.S"));
        let elf = assemble_and_link(
            &assembler,
            &linker,
            &source,
            &linker_script,
            temp.path(),
            name,
        );
        let elf_data = std::fs::read(&elf).expect("fresh guest ELF");

        let cli_result = Command::new(&cli)
            .args(["run", elf.to_str().expect("UTF-8 guest path")])
            .args(["--max-cycles", &MAX_CYCLES.to_string()])
            .output()
            .unwrap_or_else(|error| panic!("failed to run CLI for {name}: {error}"));
        assert!(
            cli_result.status.success(),
            "CLI guest {name} failed with {}:\nstdout:\n{}\nstderr:\n{}",
            cli_result.status,
            String::from_utf8_lossy(&cli_result.stdout),
            String::from_utf8_lossy(&cli_result.stderr)
        );

        let library_result = load_and_run(&elf_data, Some(MAX_CYCLES), None, None, false)
            .unwrap_or_else(|error| panic!("load_and_run failed for {name}: {error}"));
        assert_eq!(library_result.exit_code, 0, "library route: {name}");
        assert!(!library_result.timed_out, "library route timed out: {name}");
        assert!(
            library_result.error.is_none(),
            "library route error for {name}: {:?}",
            library_result.error
        );

        let mut flat = RiscVSimulator::new(0x20_000);
        flat.load_elf(&elf_data)
            .unwrap_or_else(|error| panic!("flat facade load failed for {name}: {error}"));
        let flat_result = flat
            .run(Some(MAX_CYCLES))
            .unwrap_or_else(|error| panic!("flat facade run failed for {name}: {error}"));
        assert_eq!(flat_result.exit_code, 0, "flat facade: {name}");
        assert!(!flat_result.timed_out, "flat facade timed out: {name}");
        assert!(
            flat_result.error.is_none(),
            "flat facade error for {name}: {:?}",
            flat_result.error
        );

        eprintln!("[PASS] {name}: CLI process, load_and_run, and RiscVSimulator exit 0");
    }
}
