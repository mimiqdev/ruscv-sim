//! TLM direct memory U3 (direct-memory D11): guest-level proof that a
//! self-modifying store/fetch pair and a page-table store/walk pair really
//! left `b_transport`.
//!
//! `scripts/run_elf_tests.sh` runs every guest a third time under
//! `--transport tlm --dmi` and compares the parity set's commit logs with
//! native. That proves the direct pass is architecturally equal, but it cannot
//! show that the direct path was used: an image that never obtains a grant
//! passes as well. These tests close that gap. Each one wraps the in-tree
//! zero-delay target, records every `b_transport` the target served, and runs
//! the very image the suite runs, once with direct memory enabled and once
//! without:
//!
//! * the store into the guest's own code page and the fetch of the patched
//!   word after `FENCE.I` must produce no transaction while direct memory is
//!   enabled, and
//! * the store into the page table and the walk that follows `SFENCE.VMA`
//!   must produce no transaction while direct memory is enabled.
//!
//! The disabled run is the control: it proves the same image performs those
//! very accesses, so an enabled run cannot pass by not exercising the path.
//! The guest's own checks then prove the bytes crossed the boundary: the
//! patched word executes, and the load after the page-table rewrite reports
//! the page only the new PTE can map.
//!
//! Both images are fresh-suite members under `tests/bare-metal-riscv-test`,
//! so the toolchain that assembles them is the one CI installs.

use ruscv_sim::machine::{MachineConfig, PlatformKind};
use ruscv_sim::{
    load_and_run_file_with_config, BlockingCommand, BlockingDebug, BlockingDirectMemInvalidator,
    BlockingDirectMemQuery, BlockingDirectMemResponse, BlockingInitiator, BlockingTarget,
    BlockingTransaction, Femtoseconds, PhysicalAccessKind, Transport, ZeroDelayRam,
};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use tempfile::TempDir;

const MAX_CYCLES: u64 = 100_000;
const LINKER_SCRIPT: &str = "linker-s4.ld";

/// One transaction the counting target served.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Call {
    address: u64,
    command: BlockingCommand,
    length: usize,
    kind: PhysicalAccessKind,
    initiator: BlockingInitiator,
}

impl Call {
    /// True when this transaction carries the bytes of `address .. address+len`.
    fn covers(&self, address: u64, len: u64) -> bool {
        self.address <= address && address + len <= self.address + self.length as u64
    }

    /// True when this transaction carries any byte of `address .. address+len`.
    /// Fetch parcels are two bytes, so a fetch of one instruction overlaps it
    /// without covering it.
    fn overlaps(&self, address: u64, len: u64) -> bool {
        self.address < address + len && address < self.address + self.length as u64
    }
}

/// The 4 KiB chunk a physical address belongs to: one direct-memory grant.
fn chunk(address: u64) -> u64 {
    address & !0xfff
}

/// A zero-delay RAM target that records every transaction it serves.
///
/// It delegates the bytes, the grants and the invalidator registrations to the
/// in-tree target, so direct memory behaves exactly as production does; only
/// the observation is added.
struct CountingTarget {
    inner: ZeroDelayRam,
    calls: Mutex<Vec<Call>>,
    queries: AtomicUsize,
}

impl CountingTarget {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            inner: ZeroDelayRam::new(),
            calls: Mutex::new(Vec::new()),
            queries: AtomicUsize::new(0),
        })
    }

    fn calls(&self) -> Vec<Call> {
        self.calls.lock().unwrap().clone()
    }

    fn queries(&self) -> usize {
        self.queries.load(Ordering::SeqCst)
    }

    fn bytes_at(&self, address: u64, len: usize) -> Vec<u8> {
        self.inner.bytes_at(address, len)
    }
}

impl BlockingTarget for CountingTarget {
    fn declared_max_delay(&self) -> Femtoseconds {
        self.inner.declared_max_delay()
    }

    fn b_transport(&self, transaction: &mut BlockingTransaction<'_>) {
        let (kind, initiator) = {
            let extension = transaction.extension_mut();
            (extension.access_kind(), extension.initiator())
        };
        let call = Call {
            address: transaction.address(),
            command: transaction.command(),
            length: transaction.data_length(),
            kind,
            initiator,
        };
        self.calls.lock().unwrap().push(call);
        self.inner.b_transport(transaction);
    }

    fn transport_dbg(&self, transaction: &mut BlockingDebug<'_>) -> usize {
        self.inner.transport_dbg(transaction)
    }

    fn transport_reset(&self) -> bool {
        self.inner.transport_reset()
    }

    fn get_direct_mem_ptr(&self, query: BlockingDirectMemQuery) -> BlockingDirectMemResponse {
        self.queries.fetch_add(1, Ordering::SeqCst);
        self.inner.get_direct_mem_ptr(query)
    }

    fn register_direct_mem_invalidator(&self, invalidator: BlockingDirectMemInvalidator) {
        self.inner.register_direct_mem_invalidator(invalidator)
    }
}

fn command_available(command: &str) -> bool {
    Command::new(command)
        .arg("--version")
        .output()
        .is_ok_and(|output| output.status.success())
}

fn required_toolchain() -> Option<(String, String, String)> {
    let prefix =
        std::env::var("RISCV_PREFIX").unwrap_or_else(|_| "riscv64-unknown-elf-".to_string());
    let assembler = format!("{prefix}as");
    let linker = format!("{prefix}ld");
    let namer = format!("{prefix}nm");
    if command_available(&assembler) && command_available(&linker) && command_available(&namer) {
        Some((assembler, linker, namer))
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
        .args(["-march=rv64ima_zicsr_zifencei", "-mabi=lp64"])
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
    elf
}

/// One symbol's virtual address from the linked image.
fn symbol(namer: &str, elf: &Path, name: &str) -> u64 {
    let output = Command::new(namer)
        .arg(elf)
        .output()
        .unwrap_or_else(|error| panic!("failed to invoke {namer}: {error}"));
    assert!(output.status.success(), "{} failed", namer);
    let names = String::from_utf8_lossy(&output.stdout);
    names
        .lines()
        .find_map(|line| {
            let fields: Vec<&str> = line.split_whitespace().collect();
            if fields.len() >= 3 && fields[2] == name {
                u64::from_str_radix(fields[0], 16).ok()
            } else {
                None
            }
        })
        .unwrap_or_else(|| panic!("{name} is not in {}", elf.display()))
}

/// Runs one guest image through the configured entry point with `dmi` on or
/// off, and returns its result together with the recording target.
fn run_guest(elf: &Path, direct_mem: bool) -> (ruscv_sim::ExecutionResult, Arc<CountingTarget>) {
    let target = CountingTarget::new();
    let mut config = MachineConfig::new(PlatformKind::Native);
    config.transport = Transport::tlm(target.clone());
    config.dmi = direct_mem;
    let result = load_and_run_file_with_config(
        elf.to_str().expect("UTF-8 guest path"),
        config,
        Some(MAX_CYCLES),
        None,
        None,
        false,
        None,
    )
    .unwrap_or_else(|error| panic!("{} failed: {error}", elf.display()));
    (result, target)
}

fn assert_success(label: &str, result: &ruscv_sim::ExecutionResult) {
    assert_eq!(result.exit_code, 0, "{label}: guest failed");
    assert!(!result.timed_out, "{label}: guest timed out");
    assert!(result.error.is_none(), "{label}: {:?}", result.error);
}

fn build(toolchain: &(String, String, String), name: &str, directory: &Path) -> PathBuf {
    let repository = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let source = repository
        .join("tests/bare-metal-riscv-test/rv64i")
        .join(format!("{name}.S"));
    let linker_script = repository
        .join("tests/bare-metal-riscv-test")
        .join(LINKER_SCRIPT);
    assemble_and_link(
        &toolchain.0,
        &toolchain.1,
        &source,
        &linker_script,
        directory,
        name,
    )
}

fn skip_or_fail(message: &str) {
    if std::env::var_os("RISCV_REQUIRE_RISCV_TOOLCHAIN").is_some() {
        panic!("{message}; CI requested a real guest run");
    }
    eprintln!("[SKIP] {message}");
}

#[test]
fn self_modifying_code_keeps_its_store_and_its_fetch_on_the_direct_path() {
    let Some(toolchain) = required_toolchain() else {
        skip_or_fail("riscv64-unknown-elf-as/ld/nm unavailable");
        return;
    };
    let temp = TempDir::new().expect("temporary direct-memory guest directory");
    let elf = build(&toolchain, "direct_mem_smc_fence_i", temp.path());
    let patch = symbol(&toolchain.2, &elf, "smc_patch");
    const PLANTED: u32 = 0x02a0_0513; // addi a0, zero, 42

    // Direct memory enabled: the store into the code page and the fetch of
    // the patched word must both leave the blocking route.
    let (direct, target) = run_guest(&elf, true);
    assert_success("direct-memory smc run", &direct);
    let calls = target.calls();
    assert!(
        target.queries() > 0,
        "the enabled run never queried for a grant"
    );
    assert!(
        !calls
            .iter()
            .any(|call| call.kind == PhysicalAccessKind::DataWrite
                && call.covers(patch, 4)
                && call.initiator == BlockingInitiator::Hart),
        "the store into the code page took b_transport: {calls:#?}"
    );
    assert!(
        !calls
            .iter()
            .any(|call| call.kind == PhysicalAccessKind::Fetch && call.overlaps(patch, 4)),
        "the fetch of the patched word took b_transport: {calls:#?}"
    );
    // Only the chunk's first (cold) access may be a transaction: that is the
    // access whose hint installs the grant the rest of the page uses.
    let code_page: Vec<&Call> = calls
        .iter()
        .filter(|call| chunk(call.address) == chunk(patch))
        .collect();
    assert_eq!(
        code_page.len(),
        1,
        "the code page took more than its cold access: {code_page:#?}"
    );
    assert_eq!(
        code_page[0].kind,
        PhysicalAccessKind::Fetch,
        "the cold access to the code page was not the entry fetch"
    );
    // The store really crossed the boundary, and the target's bytes show it.
    assert_eq!(
        target.bytes_at(patch, 4),
        PLANTED.to_le_bytes().to_vec(),
        "the planted instruction is not in the target's memory"
    );

    // The same image without direct memory: the same guest passes, and the
    // very accesses above are now ordinary transactions. Without this control
    // the proof could pass by not exercising the path at all.
    let (blocking, blocking_target) = run_guest(&elf, false);
    assert_success("blocking smc run", &blocking);
    let blocking_calls = blocking_target.calls();
    assert_eq!(
        blocking_target.queries(),
        0,
        "direct memory was disabled but the adapter still queried"
    );
    assert!(
        blocking_calls
            .iter()
            .any(|call| call.kind == PhysicalAccessKind::DataWrite
                && call.covers(patch, 4)
                && call.initiator == BlockingInitiator::Hart),
        "the control run did not store to the code page: {blocking_calls:#?}"
    );
    assert!(
        blocking_calls
            .iter()
            .any(|call| call.kind == PhysicalAccessKind::Fetch && call.overlaps(patch, 4)),
        "the control run did not fetch the patched word: {blocking_calls:#?}"
    );
    assert_eq!(
        blocking_target.bytes_at(patch, 4),
        PLANTED.to_le_bytes().to_vec(),
        "the control run's store did not reach the target"
    );
}

#[test]
fn a_page_table_store_and_the_next_walk_stay_on_the_direct_path() {
    let Some(toolchain) = required_toolchain() else {
        skip_or_fail("riscv64-unknown-elf-as/ld/nm unavailable");
        return;
    };
    let temp = TempDir::new().expect("temporary direct-memory guest directory");
    let elf = build(&toolchain, "direct_mem_sv39_pt_store", temp.path());
    let leaf = symbol(&toolchain.2, &elf, "leaf_table");
    let page_b = symbol(&toolchain.2, &elf, "page_b");
    // leaf[3] maps VA 0x3000: the guest stores the new leaf PTE there.
    let leaf_pte = leaf + 24;
    let new_pte = ((page_b >> 12) << 10) | 0xc7;

    // Direct memory enabled: the PTE store and the walk that follows
    // SFENCE.VMA must both leave the blocking route.
    let (direct, target) = run_guest(&elf, true);
    assert_success("direct-memory sv39 run", &direct);
    let calls = target.calls();
    assert_eq!(
        target.bytes_at(leaf_pte, 8),
        new_pte.to_le_bytes().to_vec(),
        "the target's page table does not hold the new PTE"
    );
    // The guest writes leaf[3] twice: once in M-mode to establish the mapping
    // and once in S-mode after the first load. The page-table chunk may only
    // be entered once, by that setup store, which is the access whose hint
    // installs the grant; the rewrite under test and every later walk read,
    // including the one after SFENCE.VMA, must be direct.
    let table_page: Vec<&Call> = calls
        .iter()
        .filter(|call| chunk(call.address) == chunk(leaf_pte))
        .collect();
    assert_eq!(
        table_page.len(),
        1,
        "the page-table page took more than its cold access: {table_page:#?}"
    );
    assert_eq!(
        table_page[0].kind,
        PhysicalAccessKind::DataWrite,
        "the cold access to the page-table page was not the setup store"
    );
    assert!(
        table_page[0].covers(leaf_pte, 8),
        "the cold access to the page-table page did not write leaf[3]"
    );

    // Without direct memory the same guest passes, and the PTE store and the
    // walks are ordinary transactions: the control proves the image exercises
    // exactly the accesses the enabled run kept off the blocking route.
    let (blocking, blocking_target) = run_guest(&elf, false);
    assert_success("blocking sv39 run", &blocking);
    let blocking_calls = blocking_target.calls();
    let blocking_writes = blocking_calls
        .iter()
        .filter(|call| call.kind == PhysicalAccessKind::DataWrite && call.covers(leaf_pte, 8))
        .count();
    assert_eq!(
        blocking_writes, 2,
        "the control run did not write leaf[3] twice (setup and rewrite): {blocking_calls:#?}"
    );
    let walks = blocking_calls
        .iter()
        .filter(|call| call.kind == PhysicalAccessKind::DataRead && call.covers(leaf_pte, 8))
        .count();
    assert!(
        walks >= 2,
        "the control run read the leaf PTE {walks} time(s); the walk after \
         SFENCE.VMA is missing: {blocking_calls:#?}"
    );
    assert_eq!(
        blocking_target.bytes_at(leaf_pte, 8),
        new_pte.to_le_bytes().to_vec(),
        "the control run's PTE store did not reach the target"
    );
}
