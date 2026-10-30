//! U1: the DM system bus access register block.
//!
//! `docs/dev-plan.md` §4 U1, with §2 D2/D5/D6 and §3 ownership. Each test
//! names one contract sentence. These tests stay at the DMI/DM boundary: they
//! drive `sbcs`, `sbaddress` and `sbdata` through [`DebugModule::dmi_read`] and
//! [`DebugModule::dmi_write`], hand each started request to a minimal RAM
//! target, and report the result with [`DebugModule::complete_sba_access`].
//! The Platform inbound path (U2) and Machine admission (U3) are not part of
//! this unit.

use ruscv_sim::core::RiscvCore;
use ruscv_sim::csr::{machine, mstatus, supervisor};
use ruscv_sim::memory::{MemoryInterface, SimpleMemory};
use ruscv_sim::physical::{
    AtomicAccess, AtomicAccessResult, AtomicRequest, PhysicalAccess, PhysicalAccessResult,
    PhysicalRequest,
};
use ruscv_sim::{DebugModule, SbaAccessSize, SbaFault, SbaRequest};
use std::sync::{Arc, Mutex};

/// Debug Spec 1.0 §3.14 register addresses.
const DMCONTROL: u8 = 0x10;
const SBCS: u8 = 0x38;
const SBADDRESS0: u8 = 0x39;
const SBADDRESS1: u8 = 0x3a;
const SBADDRESS2: u8 = 0x3b;
const SBADDRESS3: u8 = 0x37;
const SBDATA0: u8 = 0x3c;
const SBDATA1: u8 = 0x3d;
const SBDATA2: u8 = 0x3e;
const SBDATA3: u8 = 0x3f;

const DMACTIVE: u32 = 1;

/// `sbcs` field positions (§3.14 `sbcs`).
const SBBUSYERROR: u32 = 1 << 22;
const SBBUSY: u32 = 1 << 21;
const SBREADONADDR: u32 = 1 << 20;
const SBACCESS_SHIFT: u32 = 17;
const SBACCESS_MASK: u32 = 0b111 << SBACCESS_SHIFT;
const SBAUTOINCREMENT: u32 = 1 << 16;
const SBREADONDATA: u32 = 1 << 15;
const SBERROR_SHIFT: u32 = 12;
const SBERROR_MASK: u32 = 0b111 << SBERROR_SHIFT;
const SBASIZE_SHIFT: u32 = 5;
const SBACCESS128: u32 = 1 << 4;
const SBACCESS_WIDTHS: u32 = 0b1111;

/// Physical base of the minimal SBA target.
const TARGET_BASE: u64 = 0x8000_0000;
const TARGET_SIZE: usize = 0x1_0000;

/// `sbaccess` and `sberror` field encoders.
fn sbaccess(width: u32) -> u32 {
    width << SBACCESS_SHIFT
}

fn sberror(value: u32) -> u32 {
    value << SBERROR_SHIFT
}

/// Minimal physical target for the DM's inbound request: raw little-endian
/// bytes, the Platform's raw-byte rule, over one address window.
struct SbaRam {
    base: u64,
    bytes: Mutex<Vec<u8>>,
}

impl SbaRam {
    fn new(base: u64) -> Self {
        Self {
            base,
            bytes: Mutex::new(vec![0; TARGET_SIZE]),
        }
    }

    /// The bytes at a window offset, for checking what a write committed.
    fn peek(&self, offset: usize, len: usize) -> Vec<u8> {
        self.bytes.lock().unwrap()[offset..offset + len].to_vec()
    }

    fn access(&self, request: SbaRequest) -> Result<u64, SbaFault> {
        let len = request.size.bytes() as usize;
        let Some(offset) = request.address.checked_sub(self.base) else {
            return Err(SbaFault::BadAddress);
        };
        if offset + len as u64 > TARGET_SIZE as u64 {
            return Err(SbaFault::BadAddress);
        }
        let offset = offset as usize;
        let mut bytes = self.bytes.lock().unwrap();
        match request.write {
            Some(value) => {
                bytes[offset..offset + len].copy_from_slice(&value.to_le_bytes()[..len]);
                Ok(0)
            }
            None => {
                let mut raw = [0u8; 8];
                raw[..len].copy_from_slice(&bytes[offset..offset + len]);
                Ok(u64::from_le_bytes(raw))
            }
        }
    }
}

/// A Hart port that fails the test the moment it is used. The DM reaches its
/// own target, never the Hart's physical path (§3 ownership).
struct ForbiddenPort;

impl PhysicalAccess for ForbiddenPort {
    fn access(&mut self, _request: PhysicalRequest<'_>) -> PhysicalAccessResult {
        panic!("a system bus access reached the Hart's physical port");
    }
}

impl AtomicAccess for ForbiddenPort {
    fn access_atomic(&mut self, _request: AtomicRequest<'_>) -> AtomicAccessResult {
        panic!("a system bus access reached the Hart's atomic port");
    }
}

struct Harness {
    dm: DebugModule,
    hart: RiscvCore,
    target: SbaRam,
}

fn typed(mem: Arc<Mutex<SimpleMemory>>) -> Arc<Mutex<dyn MemoryInterface + Send + Sync>> {
    mem
}

impl Harness {
    /// An active DM over a Hart whose fetch and data ports panic if touched.
    fn activated() -> Self {
        Self::with_target(TARGET_BASE)
    }

    fn with_target(base: u64) -> Self {
        let mem = Arc::new(Mutex::new(SimpleMemory::new(TARGET_SIZE)));
        let hart = RiscvCore::new_with_physical_ports(
            typed(mem.clone()),
            typed(mem),
            Arc::new(Mutex::new(ForbiddenPort)),
            Arc::new(Mutex::new(ForbiddenPort)),
        );
        let mut harness = Self {
            dm: DebugModule::new(),
            hart,
            target: SbaRam::new(base),
        };
        harness.write(DMCONTROL, DMACTIVE);
        harness
    }

    fn read(&mut self, addr: u8) -> u32 {
        self.dm.dmi_read(&mut self.hart, addr)
    }

    fn write(&mut self, addr: u8, value: u32) {
        self.dm.dmi_write(&mut self.hart, addr, value)
    }

    fn cs(&mut self) -> u32 {
        self.read(SBCS)
    }

    fn busy(&mut self) -> bool {
        self.cs() & SBBUSY != 0
    }

    fn busy_error(&mut self) -> bool {
        self.cs() & SBBUSYERROR != 0
    }

    fn access_width(&mut self) -> u32 {
        (self.cs() & SBACCESS_MASK) >> SBACCESS_SHIFT
    }

    fn error(&mut self) -> u32 {
        (self.cs() & SBERROR_MASK) >> SBERROR_SHIFT
    }

    /// The 64-bit `sbaddress`.
    fn address(&mut self) -> u64 {
        u64::from(self.read(SBADDRESS0)) | (u64::from(self.read(SBADDRESS1)) << 32)
    }

    /// The 64-bit `sbdata`.
    fn data(&mut self) -> u64 {
        u64::from(self.read(SBDATA0)) | (u64::from(self.read(SBDATA1)) << 32)
    }

    /// Write `sbaddress` high half first, as a debugger does.
    fn set_address(&mut self, address: u64) {
        self.write(SBADDRESS1, (address >> 32) as u32);
        self.write(SBADDRESS0, address as u32);
    }

    /// Hand every started request to the target and record its result.
    /// Returns how many requests this drained, which also proves that one
    /// request is handed out once.
    fn run(&mut self) -> u32 {
        let mut drained = 0;
        while let Some(request) = self.dm.next_sba_request() {
            let result = self.target.access(request);
            self.dm.complete_sba_access(result);
            drained += 1;
        }
        drained
    }

    /// Write `value` at `address` with `width`, then complete the access.
    fn write_at(&mut self, width: u32, address: u64, value: u64) {
        self.write(SBCS, sbaccess(width));
        self.set_address(address);
        self.write(SBDATA1, (value >> 32) as u32);
        self.write(SBDATA0, value as u32);
        assert_eq!(self.run(), 1, "a sbdata0 write starts one bus write");
    }

    /// Read at `address` with `width` through `sbreadonaddr`, then complete
    /// the access.
    fn read_at(&mut self, width: u32, address: u64) -> u64 {
        self.write(SBCS, SBREADONADDR | sbaccess(width));
        self.set_address(address);
        self.run();
        self.data()
    }
}

/// Contract §2 D2: `sbcs` version 1, `sbasize` 64, widths 8-64 advertised and
/// `sbaccess128` reading 0.
#[test]
fn sbcs_advertises_version_size_and_width_capabilities() {
    let mut harness = Harness::activated();
    let cs = harness.cs();
    assert_eq!((cs >> 29) & 0b111, 1, "sbversion is 1");
    assert_eq!((cs >> 23) & 0b111_111, 0, "bits 28:23 are reserved 0");
    assert_eq!(
        (cs >> SBASIZE_SHIFT) & 0x7f,
        64,
        "sbasize is a 64-bit address"
    );
    assert_eq!(cs & SBACCESS_WIDTHS, 0b1111, "widths 8, 16, 32 and 64");
    assert_eq!(cs & SBACCESS128, 0, "sbaccess128 reads 0");
    assert_eq!(harness.access_width(), 2, "sbaccess resets to 32-bit");
    assert_eq!(cs & SBREADONADDR, 0);
    assert_eq!(cs & SBREADONDATA, 0);
    assert_eq!(cs & SBAUTOINCREMENT, 0);
    assert!(!harness.busy(), "sbbusy resets to 0");
    assert!(!harness.busy_error(), "sbbusyerror resets to 0");
    assert_eq!(harness.error(), 0, "sberror resets to 0");
    assert_eq!(harness.address(), 0, "sbaddress resets to 0");
    assert_eq!(harness.data(), 0, "sbdata resets to 0");
}

/// Contract §2 D2: no `sbaddress2`/`sbaddress3` and no `sbdata2`/`sbdata3`.
#[test]
fn absent_sbaddress_and_sbdata_registers_read_zero_and_ignore_writes() {
    let mut harness = Harness::activated();
    harness.write(SBCS, sbaccess(3));
    for addr in [SBADDRESS2, SBADDRESS3, SBDATA2, SBDATA3] {
        harness.write(addr, 0xffff_ffff);
        assert_eq!(
            harness.read(addr),
            0,
            "register {addr:#04x} is absent in this profile"
        );
    }
    harness.set_address(0x1122_3344_5566_7788);
    assert_eq!(
        harness.address(),
        0x1122_3344_5566_7788,
        "an absent address register writes nothing"
    );
    assert_eq!(harness.data(), 0);
}

/// Contract §4 U1: every `sbaccess` width, 8, 16, 32 and 64.
#[test]
fn every_sbaccess_width_round_trips_through_the_target() {
    let mut harness = Harness::activated();
    // Four adjacent, distinctly aligned slots: byte, half, word, double.
    let slots = [
        (0u32, 0x0u64, 0xab_u64),
        (1, 0x2, 0xbeef),
        (2, 0x4, 0xdead_beef),
        (3, 0x8, 0x89ab_cdef_0123_4567),
    ];
    for (width, offset, value) in slots {
        harness.write_at(width, TARGET_BASE + offset, value);
        assert_eq!(harness.error(), 0, "width {width} write succeeds");
        let len = 1usize << width;
        assert_eq!(
            harness.target.peek(offset as usize, len),
            value.to_le_bytes()[..len].to_vec(),
            "width {width} wrote exactly its bytes"
        );
    }
    for (width, offset, value) in slots {
        assert_eq!(
            harness.read_at(width, TARGET_BASE + offset),
            value,
            "width {width} reads back"
        );
        assert_eq!(harness.error(), 0, "width {width} read succeeds");
    }
}

/// Contract §2 D6: `sbreadonaddr` triggers a read at the new address.
#[test]
fn sbreadonaddr_triggers_a_read_of_the_new_address() {
    let mut harness = Harness::activated();
    harness.write(SBCS, sbaccess(2));
    harness.set_address(TARGET_BASE);
    harness.write(SBDATA0, 0x2020_0304);
    harness.run();

    harness.write(SBCS, SBREADONADDR | sbaccess(2));
    harness.set_address(TARGET_BASE);
    assert!(harness.busy(), "writing sbaddress0 starts the read");
    assert_eq!(harness.run(), 1);
    assert_eq!(harness.data(), 0x2020_0304);

    // A write to `sbaddress1` is not a trigger (§3.14 names `sbaddress0`).
    harness.write(SBADDRESS1, 0);
    assert!(!harness.busy(), "only a sbaddress0 write starts the read");
    assert_eq!(harness.run(), 0);

    // With the bit clear, writing the address starts nothing.
    harness.write(SBCS, sbaccess(2));
    harness.set_address(TARGET_BASE);
    assert!(!harness.busy());
}

/// Contract §2 D6: `sbreadondata` reads after returning the sampled value.
#[test]
fn sbreadondata_returns_the_previous_value_and_reads_the_next() {
    let mut harness = Harness::activated();
    harness.write(SBCS, sbaccess(2));
    harness.set_address(TARGET_BASE);
    harness.write(SBDATA0, 0x1111_2222);
    harness.run();
    harness.set_address(TARGET_BASE + 4);
    harness.write(SBDATA0, 0x3333_4444);
    harness.run();

    harness.write(SBCS, SBREADONDATA | sbaccess(2));
    harness.set_address(TARGET_BASE);
    assert_eq!(
        harness.read(SBDATA0),
        0x3333_4444,
        "the read returns the sbdata it sampled before the trigger"
    );
    assert!(harness.busy(), "reading sbdata0 starts the bus read");
    harness.run();
    assert_eq!(
        harness.read(SBDATA0),
        0x1111_2222,
        "the completed read updated sbdata"
    );
    // That read started the next bus read; drain it before the side-effect
    // check below.
    harness.run();

    // `sbdata1` has no side effect (§3.14 sbdata1).
    let cs_before = harness.cs();
    harness.read(SBDATA1);
    assert_eq!(harness.cs(), cs_before);
    assert!(!harness.busy());
    assert_eq!(harness.run(), 0);
}

/// Contract §2 D6: `sbautoincrement` advances by the access size after a
/// successful access only.
#[test]
fn sbautoincrement_advances_by_the_access_size_after_a_successful_access() {
    let mut harness = Harness::activated();
    for (width, bytes) in [(0u32, 1u64), (1, 2), (2, 4), (3, 8)] {
        harness.write(SBCS, SBREADONADDR | SBAUTOINCREMENT | sbaccess(width));
        harness.set_address(TARGET_BASE + 0x100);
        harness.run();
        assert_eq!(
            harness.address(),
            TARGET_BASE + 0x100 + bytes,
            "a width-{width} read advances by its own size"
        );

        harness.write(SBCS, SBAUTOINCREMENT | sbaccess(width));
        harness.set_address(TARGET_BASE + 0x200);
        harness.write(SBDATA0, 5);
        harness.run();
        assert_eq!(
            harness.address(),
            TARGET_BASE + 0x200 + bytes,
            "a width-{width} write advances by its own size"
        );
    }

    // A failed access leaves the address alone.
    harness.write(SBCS, SBAUTOINCREMENT | sbaccess(3));
    harness.set_address(TARGET_BASE - 8);
    harness.write(SBDATA0, 1);
    harness.run();
    assert_eq!(harness.error(), 2, "the target refused the address");
    assert_eq!(
        harness.address(),
        TARGET_BASE - 8,
        "a failed access does not advance sbaddress"
    );
}

/// Contract §4 U1: the `sbaddress1` carry on a 64-bit address overflow.
#[test]
fn sbautoincrement_carries_into_sbaddress1() {
    // A window whose low half ends at the 32-bit boundary, so an
    // auto-incremented address crosses it.
    let mut harness = Harness::with_target(0xffff_ff00);
    harness.write(SBCS, SBAUTOINCREMENT | sbaccess(3));
    harness.set_address(0x0000_0000_ffff_fff8);
    harness.write(SBDATA0, 1);
    harness.run();
    assert_eq!(harness.error(), 0, "the access succeeded");
    assert_eq!(harness.read(SBADDRESS0), 0, "the low half carried out");
    assert_eq!(harness.read(SBADDRESS1), 1, "the carry reached sbaddress1");
    assert_eq!(harness.address(), 0x1_0000_0000);
}

/// Contract §4 U1: the same increment wraps at the top of the 64-bit
/// address space.
#[test]
fn sbautoincrement_wraps_at_the_top_of_the_address_space() {
    let mut harness = Harness::with_target(0xffff_ffff_ffff_ff00);
    harness.write(SBCS, SBAUTOINCREMENT | sbaccess(3));
    harness.set_address(0xffff_ffff_ffff_fff8);
    harness.write(SBDATA0, 1);
    harness.run();
    assert_eq!(harness.error(), 0, "the access succeeded");
    assert_eq!(harness.address(), 0, "the 64-bit address wrapped to 0");
}

/// Contract §2 D5: `sbbusy` is set while an access is in progress.
#[test]
fn sbbusy_is_set_while_an_access_is_in_progress() {
    let mut harness = Harness::activated();
    harness.write(SBCS, sbaccess(2));
    assert!(!harness.busy());
    harness.set_address(TARGET_BASE);
    harness.write(SBDATA0, 0x1234);
    assert!(harness.busy(), "a started write keeps sbbusy set");
    assert_eq!(harness.access_width(), 2, "the access keeps its width");
    harness.run();
    assert!(!harness.busy(), "completing the access clears sbbusy");
}

/// Contract §2 D5: an access while busy sets `sbbusyerror` and changes
/// nothing else.
#[test]
fn access_while_busy_sets_sbbusyerror_and_changes_nothing() {
    let mut harness = Harness::activated();
    harness.write(SBCS, sbaccess(2));
    harness.set_address(TARGET_BASE);
    harness.write(SBDATA0, 0x5151_6262);
    assert!(harness.busy());
    assert!(!harness.busy_error());

    harness.write(SBADDRESS0, 0x40);
    assert!(harness.busy_error(), "a busy sbaddress0 write sets it");
    assert_eq!(
        harness.read(SBADDRESS0),
        TARGET_BASE as u32,
        "the ignored write did not change sbaddress"
    );

    harness.write(SBDATA0, 0x7777_8888);
    assert_eq!(
        harness.read(SBDATA0),
        0x5151_6262,
        "the ignored write did not change sbdata"
    );

    harness.read(SBDATA1);
    assert_eq!(harness.run(), 1, "only the original access was started");
    assert!(!harness.busy());
    assert!(harness.busy_error(), "sbbusyerror is sticky");
}

/// Contract §2 D5: a sticky `sbbusyerror` blocks new accesses until W1C.
#[test]
fn sbbusyerror_blocks_new_accesses_until_w1c() {
    let mut harness = Harness::activated();
    harness.write(SBCS, sbaccess(2));
    harness.set_address(TARGET_BASE);
    harness.write(SBDATA0, 0x1111_1111);
    harness.write(SBADDRESS0, 0);
    assert!(harness.busy_error());
    harness.run();

    // Error set: a would-be write starts nothing, though it still stages.
    harness.write(SBDATA0, 0x2222_2222);
    assert_eq!(harness.run(), 0, "no bus access starts while blocked");
    assert_eq!(harness.read(SBDATA0), 0x2222_2222);
    assert_eq!(harness.target.peek(0, 4), 0x1111_1111u32.to_le_bytes());

    harness.write(SBCS, SBBUSYERROR | sbaccess(2));
    assert!(!harness.busy_error(), "W1C clears sbbusyerror");
    harness.write(SBDATA0, 0x3333_3333);
    assert_eq!(
        harness.run(),
        1,
        "the cleared error admits the access again"
    );
    assert_eq!(harness.target.peek(0, 4), 0x3333_3333u32.to_le_bytes());
}

/// Contract §2 D5: a read while busy sets `sbbusyerror`, and so does a
/// `sbdata1` access, which has no trigger of its own.
#[test]
fn a_read_while_busy_sets_sbbusyerror() {
    let mut harness = Harness::activated();
    harness.write(SBCS, sbaccess(2));
    harness.set_address(TARGET_BASE);
    harness.write(SBDATA0, 0x1111_1111);
    assert!(harness.busy());
    assert!(!harness.busy_error());
    harness.read(SBDATA0);
    assert!(
        harness.busy_error(),
        "the read sampled sbdata while a read was in progress"
    );
    harness.run();

    harness.write(SBCS, SBBUSYERROR | sbaccess(2));
    harness.write(SBDATA0, 0x2222_2222);
    assert!(harness.busy());
    assert!(!harness.busy_error());
    harness.read(SBDATA1);
    assert!(
        harness.busy_error(),
        "an sbdata1 read while busy also sets it"
    );
}

/// §3.14 leaves a `sbcs` write while `sbbusy` high undefined. This module
/// drops it so a running access keeps its width and a sticky error cannot be
/// cleared underneath it.
#[test]
fn sbcs_write_while_busy_is_dropped() {
    let mut harness = Harness::activated();
    harness.write(SBCS, sbaccess(2));
    harness.set_address(TARGET_BASE);
    harness.write(SBDATA0, 0x1111_2222);
    assert!(harness.busy());

    harness.write(SBCS, SBREADONDATA | SBAUTOINCREMENT | sbaccess(3));
    assert_eq!(harness.access_width(), 2, "sbaccess is unchanged");
    let cs = harness.cs();
    assert_eq!(cs & (SBREADONDATA | SBAUTOINCREMENT), 0);

    assert_eq!(harness.run(), 1);
    assert_eq!(
        harness.target.peek(0, 4),
        0x1111_2222u32.to_le_bytes(),
        "the running access kept its width"
    );
}

/// The request a target receives carries only the bytes its `sbaccess` width
/// covers, and a read result is masked the same way.
#[test]
fn the_request_and_result_carry_only_the_access_width() {
    let mut harness = Harness::activated();
    // A byte write with a full high half staged in `sbdata1`.
    harness.write(SBCS, sbaccess(0));
    harness.set_address(TARGET_BASE);
    harness.write(SBDATA1, 0xffff_ffff);
    harness.write(SBDATA0, 0xffff_ffab);
    assert_eq!(
        harness.dm.next_sba_request(),
        Some(SbaRequest {
            address: TARGET_BASE,
            size: SbaAccessSize::Byte,
            write: Some(0xab),
        }),
        "a byte write carries one byte"
    );
    harness.dm.complete_sba_access(Ok(0));

    // A half-word read whose target answers with more than two bytes.
    harness.write(SBCS, SBREADONADDR | sbaccess(1));
    harness.set_address(TARGET_BASE + 2);
    let request = harness.dm.next_sba_request().expect("one read request");
    assert_eq!(request.size, SbaAccessSize::Half);
    harness.dm.complete_sba_access(Ok(0xffff_ffff_ffff_beef));
    assert_eq!(
        harness.data(),
        0xbeef,
        "only the half word the access covers reaches sbdata"
    );
}

/// A completion needs a request a target was actually given. One that was
/// never handed out cannot clear `sbbusy` or fabricate a result.
#[test]
fn a_completion_needs_a_request_that_reached_a_target() {
    let mut harness = Harness::activated();
    harness.write(SBCS, SBAUTOINCREMENT | sbaccess(2));
    harness.set_address(TARGET_BASE);
    harness.write(SBDATA0, 0x1234_5678);
    assert!(harness.busy());

    harness.dm.complete_sba_access(Ok(0xdead_beef));
    assert!(harness.busy(), "sbbusy survives a spurious completion");
    assert_eq!(harness.data(), 0x1234_5678, "sbdata is unchanged");
    assert_eq!(
        harness.address(),
        TARGET_BASE,
        "sbaddress did not autoincrement"
    );

    assert_eq!(harness.run(), 1, "the real request still completes once");
    assert!(!harness.busy());
    assert_eq!(harness.address(), TARGET_BASE + 4);
}

/// Contract §2 D5 and §4 U1: `sberror` 2 is sticky until W1C and blocks new
/// accesses until cleared.
#[test]
fn sberror_reports_a_bad_address_from_the_target() {
    let mut harness = Harness::activated();
    harness.write(SBCS, sbaccess(2));
    harness.set_address(TARGET_BASE - 4);
    harness.write(SBDATA0, 0xdead_beef);
    assert_eq!(harness.run(), 1);
    assert_eq!(harness.error(), 2, "sberror 2 is a bad address");
    assert!(!harness.busy(), "the failed access is no longer busy");

    // Sticky: reading `sbcs` does not clear it, and a `sbcs` write that does
    // not assert the W1C bits leaves it set while other fields still change.
    assert_eq!(harness.error(), 2);
    harness.write(SBCS, sbaccess(3));
    assert_eq!(
        harness.error(),
        2,
        "a plain sbcs write does not clear sberror"
    );
    assert_eq!(harness.access_width(), 3, "a plain sbcs write still lands");
    harness.write(SBCS, sbaccess(2));

    // Blocked: the trigger is suppressed but the register update is not
    // (§3.14 gates only the start on `sberror`, and so does the pinned
    // Spike).
    harness.set_address(TARGET_BASE);
    assert_eq!(
        harness.read(SBADDRESS0),
        TARGET_BASE as u32,
        "the address register still takes the value"
    );
    harness.write(SBDATA0, 0x0101_0101);
    assert_eq!(harness.run(), 0, "no bus access starts with sberror set");
    assert_eq!(
        harness.data(),
        0x0101_0101,
        "sbdata stages but starts nothing"
    );
    assert_eq!(harness.error(), 2, "sberror stays set");

    // W1C recovery.
    harness.write(SBCS, sberror(2) | sbaccess(2));
    assert_eq!(harness.error(), 0, "W1C clears sberror");
    harness.set_address(TARGET_BASE);
    harness.write(SBDATA0, 0x0a0b_0c0d);
    assert_eq!(harness.run(), 1);
    assert_eq!(harness.error(), 0, "the access succeeds after recovery");
    assert_eq!(harness.target.peek(0, 4), 0x0a0b_0c0du32.to_le_bytes());
}

/// Contract §2 D5: `sberror` 3 is an alignment error, decided before any
/// target access.
#[test]
fn sberror_reports_alignment_before_any_target_access() {
    let mut harness = Harness::activated();
    for (width, offset) in [(1u32, 1u64), (2, 2), (2, 6), (3, 4)] {
        harness.write(SBCS, sbaccess(width));
        harness.set_address(TARGET_BASE + offset);
        harness.write(SBDATA0, 1);
        assert_eq!(
            harness.run(),
            0,
            "a width-{width} access at offset {offset} never reaches the target"
        );
        assert_eq!(harness.error(), 3, "sberror 3 is an alignment error");
        harness.write(SBCS, sberror(3));
        assert_eq!(harness.error(), 0);
    }
    assert_eq!(
        harness.target.peek(0, 16),
        vec![0; 16],
        "nothing was written"
    );
}

/// Contract §2 D5: `sberror` 4 is an unsupported access size. The profile
/// advertises no 128-bit width, so `sbaccess` 4 and above fail.
#[test]
fn sberror_reports_an_unsupported_size() {
    let mut harness = Harness::activated();
    for width in 4..=7 {
        harness.write(SBCS, sbaccess(width));
        assert_eq!(
            harness.access_width(),
            width,
            "sbaccess keeps the value the debugger wrote"
        );
        harness.set_address(TARGET_BASE);
        harness.write(SBDATA0, 1);
        assert_eq!(harness.run(), 0, "no bus access starts");
        assert_eq!(harness.error(), 4, "sberror 4 is an unsupported size");
        harness.write(SBCS, sberror(4) | sbaccess(2));
        assert_eq!(harness.error(), 0);
    }

    // A read trigger refuses the same way.
    harness.write(SBCS, SBREADONADDR | sbaccess(4));
    harness.set_address(TARGET_BASE);
    assert_eq!(harness.run(), 0);
    assert_eq!(harness.error(), 4);
    assert_eq!(harness.target.peek(0, 8), vec![0; 8]);
}

/// Contract §3 ownership and §4 U1: an SBA access never enters the Hart's
/// fetch, translation, PMP or trap path, and changes no Hart state.
///
/// The Hart's fetch and data ports panic if reached, `mstatus.MPRV` with
/// `MPP=S` selects a supervisor data path whose Sv39 root maps nothing, and
/// `pc` is outside the Hart's memory. A Hart-mediated access to the SBA
/// address would panic, trap, or fail; the DM returns the target's data and
/// leaves every listed field alone.
#[test]
fn sba_accesses_never_enter_the_hart_paths() {
    let mut harness = Harness::activated();
    {
        let csr = &mut harness.hart.state_mut().csr;
        csr.write(supervisor::SATP, (8 << 60) | 1).unwrap();
        csr.write(machine::MSTATUS, mstatus::MPRV | (1 << mstatus::MPP_SHIFT))
            .unwrap();
        csr.write(machine::MEPC, 0x1234).unwrap();
        csr.write(machine::MCAUSE, 0xf).unwrap();
        csr.write(machine::MTVAL, 0x5678).unwrap();
        csr.write(machine::MEDELEG, 0xffff).unwrap();
    }
    harness.hart.state_mut().pc = TARGET_BASE;
    let before = snapshot(&harness.hart);

    harness.write_at(3, TARGET_BASE + 0x40, 0xfeed_face_dead_beef);
    assert_eq!(harness.error(), 0, "the Hart's checks refused nothing");
    assert_eq!(
        harness.read_at(3, TARGET_BASE + 0x40),
        0xfeed_face_dead_beef
    );

    assert_eq!(
        snapshot(&harness.hart),
        before,
        "no Hart fetch, translation, PMP or trap state changed"
    );
    assert!(
        !harness.hart.in_debug_mode(),
        "the Hart never entered Debug Mode"
    );
}

/// The Hart fields an SBA access must leave alone.
#[derive(Debug, PartialEq, Eq)]
struct HartState {
    pc: u64,
    mstatus: u64,
    satp: u64,
    mepc: u64,
    mcause: u64,
    mtval: u64,
    medeleg: u64,
    dcsr: u64,
    dpc: u64,
    minstret: u64,
    mcycle: u64,
    halted: bool,
}

fn snapshot(hart: &RiscvCore) -> HartState {
    let csr = &hart.state().csr;
    HartState {
        pc: hart.state().pc,
        mstatus: csr.read(machine::MSTATUS).unwrap(),
        satp: csr.read(supervisor::SATP).unwrap(),
        mepc: csr.read(machine::MEPC).unwrap(),
        mcause: csr.read(machine::MCAUSE).unwrap(),
        mtval: csr.read(machine::MTVAL).unwrap(),
        medeleg: csr.read(machine::MEDELEG).unwrap(),
        dcsr: hart.dcsr(),
        dpc: hart.dpc(),
        minstret: csr.read(machine::MINSTRET).unwrap(),
        mcycle: csr.read(machine::MCYCLE).unwrap(),
        halted: hart.in_debug_mode(),
    }
}

/// Contract §2 D9: DM reset clears the SBA command and busy state.
#[test]
fn dm_reset_clears_the_sba_state() {
    let mut harness = Harness::activated();
    harness.write(SBCS, SBREADONADDR | SBAUTOINCREMENT | sbaccess(3));
    harness.set_address(TARGET_BASE + 0x20);
    harness.write(SBDATA1, 0xaaaa_bbbb);
    harness.write(SBDATA0, 0xcccc_dddd);
    assert!(harness.busy());
    assert_eq!(harness.target.peek(0x20, 8), vec![0; 8], "not reached yet");

    // `dmactive` 1-to-0 is a DM reset (§3.14 dmcontrol.dmactive).
    harness.write(DMCONTROL, 0);
    harness.write(DMCONTROL, DMACTIVE);

    assert!(!harness.busy(), "sbbusy is cleared");
    assert!(!harness.busy_error());
    assert_eq!(harness.error(), 0);
    assert_eq!(harness.access_width(), 2, "sbaccess resets");
    let cs = harness.cs();
    assert_eq!(cs & (SBREADONADDR | SBREADONDATA | SBAUTOINCREMENT), 0);
    assert_eq!(harness.address(), 0);
    assert_eq!(harness.data(), 0);
    assert_eq!(harness.dm.next_sba_request(), None, "no request survives");
}

/// SBA registers are part of the DM: they read 0 and start nothing while
/// `dmactive` is 0.
#[test]
fn sba_registers_are_absent_while_the_dm_is_inactive() {
    let mut harness = Harness::activated();
    harness.write(DMCONTROL, 0);

    assert_eq!(harness.read(SBCS), 0);
    assert_eq!(harness.read(SBADDRESS0), 0);
    harness.write(SBCS, sbaccess(3));
    harness.set_address(TARGET_BASE);
    harness.write(SBDATA0, 1);
    assert_eq!(harness.run(), 0);

    harness.write(DMCONTROL, DMACTIVE);
    assert_eq!(harness.access_width(), 2, "the DM reset its sbcs");
    assert_eq!(harness.dm.next_sba_request(), None);
}
