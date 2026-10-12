//! Failed Sv39 walk reads are classified by the `MemoryError` variant.
//!
//! Target rejections stay `AccessFault`. Host, protocol, and unknown
//! completions use their own variants. The diagnostic string is ignored.
//! The original virtual address is kept. A failed read is issued once,
//! writes nothing, and inserts no TLB entry.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

use ruscv_sim::core::PrivilegeMode;
use ruscv_sim::memory::MemoryError;
use ruscv_sim::mmu::physical::PhysicalMemoryInterface;
use ruscv_sim::mmu::pte::{flags, PagePermissions, PageTableEntry};
use ruscv_sim::mmu::sv39::{PageTableWalker, VirtualAddress, WalkResult};
use ruscv_sim::mmu::{
    AccessType, AddressTranslator, Mmu, MmuConfig, MmuError, Tlb, TranslationRequest,
};

const ROOT_PPN: u64 = 0x80000;
const LEVEL1_PPN: u64 = 0x80001;
const LEVEL0_PPN: u64 = 0x80002;
const DATA_PPN: u64 = 0x4_0000_0000;
const SATP: u64 = (8 << 60) | ROOT_PPN;

#[derive(Clone, Copy, Debug)]
enum Injected {
    InvalidAddress,
    OutOfBounds,
    Misaligned,
    Backend,
    Protocol,
    Unknown,
}

impl Injected {
    const ALL: [Self; 6] = [
        Self::InvalidAddress,
        Self::OutOfBounds,
        Self::Misaligned,
        Self::Backend,
        Self::Protocol,
        Self::Unknown,
    ];

    fn at(self, paddr: u64) -> MemoryError {
        match self {
            Self::InvalidAddress => MemoryError::InvalidAddress(paddr),
            Self::OutOfBounds => MemoryError::OutOfBounds,
            Self::Misaligned => MemoryError::Misaligned(paddr, 8),
            Self::Backend => MemoryError::Backend("says protocol".into()),
            Self::Protocol => MemoryError::Protocol("says unknown".into()),
            Self::Unknown => MemoryError::Unknown("says access fault".into()),
        }
    }
}

struct Script {
    entries: [(u64, u64); 3],
    installed: usize,
    fault_level: Option<usize>,
    injected: Injected,
    reads: Mutex<Vec<u64>>,
    writes: AtomicUsize,
}

impl Script {
    fn new(
        entries: [(u64, u64); 3],
        installed: usize,
        fault_level: Option<usize>,
        injected: Injected,
    ) -> Self {
        Self {
            entries,
            installed,
            fault_level,
            injected,
            reads: Mutex::new(Vec::new()),
            writes: AtomicUsize::new(0),
        }
    }

    fn tables(level: usize, leaf: Option<u64>) -> ([(u64, u64); 3], usize) {
        let vpn = [3u64, 2, 1];
        let mut entries = [(0, 0); 3];
        let mut installed = 0;
        let mut add = |paddr, value| {
            entries[installed] = (paddr, value);
            installed += 1;
        };
        if level < 2 {
            add(
                (ROOT_PPN << 12) + vpn[2] * 8,
                PageTableEntry::new_pointer(LEVEL1_PPN).bits(),
            );
        }
        if level < 1 {
            add(
                (LEVEL1_PPN << 12) + vpn[1] * 8,
                PageTableEntry::new_pointer(LEVEL0_PPN).bits(),
            );
        }
        if let Some(bits) = leaf {
            let table = [LEVEL0_PPN, LEVEL1_PPN, ROOT_PPN][level];
            add((table << 12) + vpn[level] * 8, bits);
        }
        (entries, installed)
    }

    fn fault(level: usize, injected: Injected) -> Self {
        let (entries, installed) = Self::tables(level, None);
        Self::new(entries, installed, Some(level), injected)
    }

    fn preset(level: usize) -> Self {
        let bits = PageTableEntry::new_leaf(DATA_PPN, PagePermissions::rwx(), false).bits()
            | flags::A
            | flags::D;
        let (entries, installed) = Self::tables(level, Some(bits));
        Self::new(entries, installed, None, Injected::Backend)
    }

    fn invalid_at(level: usize) -> Self {
        let (entries, installed) = Self::tables(level, Some(0));
        Self::new(entries, installed, None, Injected::Backend)
    }

    fn level_of(paddr: u64) -> usize {
        match paddr >> 12 {
            ROOT_PPN => 2,
            LEVEL1_PPN => 1,
            LEVEL0_PPN => 0,
            other => panic!("walk touched {other:#x}, outside the scripted tables"),
        }
    }

    fn assert_reads(&self, level: usize, attempts: usize) {
        let addresses = [0x8000_0008, 0x8000_1010, 0x8000_2018];
        assert_eq!(
            *self.reads.lock().unwrap(),
            addresses[..3 - level].repeat(attempts)
        );
    }

    fn writes(&self) -> usize {
        self.writes.load(Ordering::Relaxed)
    }
}

impl PhysicalMemoryInterface for Script {
    fn read_byte(&self, paddr: u64) -> Result<u8, MemoryError> {
        panic!("unexpected byte read at {paddr:#x}")
    }

    fn write_byte(&mut self, paddr: u64, _value: u8) -> Result<(), MemoryError> {
        panic!("unexpected byte write at {paddr:#x}")
    }

    fn read_dword(&self, paddr: u64) -> Result<u64, MemoryError> {
        self.reads.lock().unwrap().push(paddr);
        let level = Self::level_of(paddr);
        if self.fault_level == Some(level) {
            return Err(self.injected.at(paddr));
        }
        self.entries[..self.installed]
            .iter()
            .find(|(addr, _)| *addr == paddr)
            .map(|(_, value)| *value)
            .ok_or(MemoryError::OutOfBounds)
    }

    fn write_dword(&mut self, _paddr: u64, _value: u64) -> Result<(), MemoryError> {
        self.writes.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }
}

fn vaddr() -> u64 {
    (1 << 30) | (2 << 21) | (3 << 12) | 0xABC
}

fn expected_paddr(level: usize) -> u64 {
    [0x4000_0000_0ABC, 0x4000_0000_3ABC, 0x4000_0040_3ABC][level]
}

fn virtual_address() -> VirtualAddress {
    VirtualAddress::new(vaddr()).unwrap()
}

fn expect_walk(result: WalkResult, injected: Injected, level: usize) {
    let actual = match (injected, result) {
        (
            Injected::InvalidAddress | Injected::OutOfBounds | Injected::Misaligned,
            WalkResult::AccessFault { level },
        ) => level,
        (Injected::Backend, WalkResult::HostFault { level }) => level,
        (Injected::Protocol, WalkResult::ProtocolFault { level }) => level,
        (Injected::Unknown, WalkResult::UnknownFault { level }) => level,
        (_, other) => panic!("{injected:?} classified as {other:?}"),
    };
    assert_eq!(actual, level);
}

fn expect_mmu(result: Result<u64, MmuError>, injected: Injected) {
    let address = vaddr();
    match (injected, result) {
        (
            Injected::InvalidAddress | Injected::OutOfBounds | Injected::Misaligned,
            Err(MmuError::AccessFault(got)),
        )
        | (Injected::Backend, Err(MmuError::WalkHostFault(got)))
        | (Injected::Protocol, Err(MmuError::WalkProtocolFault(got)))
        | (Injected::Unknown, Err(MmuError::WalkUnknownFault(got))) => assert_eq!(got, address),
        (_, other) => panic!("{injected:?} classified as {other:?}"),
    }
}

fn request(access: AccessType) -> TranslationRequest {
    TranslationRequest {
        vaddr: vaddr(),
        access_type: access,
        privilege: PrivilegeMode::Supervisor,
        satp: SATP,
        mstatus: 0,
    }
}

#[test]
fn walk_classifies_every_read_error_at_every_level() {
    for level in [2usize, 1, 0] {
        for injected in Injected::ALL {
            let mut memory = Script::fault(level, injected);
            let walker = PageTableWalker::new(&mut memory, ROOT_PPN);
            expect_walk(walker.walk(virtual_address()), injected, level);
            memory.assert_reads(level, 1);
            assert_eq!(memory.writes(), 0);
        }
    }
}

#[test]
fn permission_and_ad_wrappers_propagate_the_read_class() {
    for level in [2usize, 1, 0] {
        for injected in Injected::ALL {
            let mut memory = Script::fault(level, injected);
            let mut walker = PageTableWalker::new(&mut memory, ROOT_PPN);
            expect_walk(
                walker.walk_check_permissions(virtual_address(), AccessType::Read, false),
                injected,
                level,
            );
            expect_walk(
                walker.walk_check_permissions_and_update_ad(
                    virtual_address(),
                    AccessType::Write,
                    false,
                ),
                injected,
                level,
            );
            memory.assert_reads(level, 2);
            assert_eq!(memory.writes(), 0);
        }
    }
}

#[test]
fn both_translator_routes_and_mmu_keep_the_class_and_vaddr() {
    let translator = AddressTranslator::new(MmuConfig::default());
    let accesses = [
        AccessType::InstructionFetch,
        AccessType::Read,
        AccessType::Write,
    ];
    for level in [2usize, 1, 0] {
        for injected in Injected::ALL {
            for access in accesses {
                let mut memory = Script::fault(level, injected);
                let mut tlb = Tlb::new(8, 1);
                let through_tlb =
                    translator.translate_with_tlb(request(access), &mut tlb, &mut memory);
                expect_mmu(through_tlb, injected);
                assert_eq!(tlb.stats().hits, 0);
                assert_eq!(tlb.stats().misses, 1);
                memory.assert_reads(level, 1);
                assert_eq!(memory.writes(), 0);
                assert!(tlb.lookup(vaddr() >> 12, 0).is_none());
                expect_mmu(
                    translator.translate_with_tlb(request(access), &mut tlb, &mut memory),
                    injected,
                );
                memory.assert_reads(level, 2);
                assert_eq!(memory.writes(), 0);

                memory = Script::fault(level, injected);
                let direct = translator.translate_sv39_walk(
                    vaddr(),
                    SATP,
                    access,
                    PrivilegeMode::Supervisor,
                    &mut memory,
                );
                expect_mmu(direct, injected);
                memory.assert_reads(level, 1);
                assert_eq!(memory.writes(), 0);

                memory = Script::fault(level, injected);
                let mut mmu = Mmu::new(MmuConfig::default());
                expect_mmu(mmu.translate(request(access), &mut memory), injected);
                memory.assert_reads(level, 1);
                assert_eq!(memory.writes(), 0);
                expect_mmu(mmu.translate(request(access), &mut memory), injected);
                memory.assert_reads(level, 2);
                assert_eq!(memory.writes(), 0);
                let (itlb, dtlb) = mmu.tlb_stats();
                assert_eq!(itlb.hits + dtlb.hits, 0);
                assert_eq!(itlb.misses + dtlb.misses, 2);
            }
        }
    }
}

#[test]
fn success_with_preset_ad_and_invalid_pte_are_unchanged() {
    let translator = AddressTranslator::new(MmuConfig::default());
    for level in [2usize, 1, 0] {
        let mut memory = Script::preset(level);
        let mut walker = PageTableWalker::new(&mut memory, ROOT_PPN);
        let walked = walker.walk_check_permissions_and_update_ad(
            virtual_address(),
            AccessType::Write,
            false,
        );
        let WalkResult::Success {
            paddr, level: got, ..
        } = walked
        else {
            panic!("preset A/D leaf at level {level} should succeed, got {walked:?}");
        };
        assert_eq!(got, level);
        assert_eq!(paddr, expected_paddr(level));
        assert_eq!(memory.writes(), 0, "preset A and D need no write-back");

        let mut tlb = Tlb::new(8, 1);
        memory = Script::preset(level);
        let translated = translator
            .translate_with_tlb(request(AccessType::Write), &mut tlb, &mut memory)
            .unwrap();
        assert_eq!(translated, expected_paddr(level));
        assert_eq!(tlb.stats().misses, 1);
        assert_eq!(memory.writes(), 0);
        memory = Script::preset(level);
        let again = translator
            .translate_with_tlb(request(AccessType::Write), &mut tlb, &mut memory)
            .unwrap();
        assert_eq!(again, translated);
        assert_eq!(tlb.stats().hits, 1);
        assert!(
            memory.reads.lock().unwrap().is_empty(),
            "a TLB hit walked again"
        );

        memory = Script::invalid_at(level);
        let walker = PageTableWalker::new(&mut memory, ROOT_PPN);
        assert!(matches!(
            walker.walk(virtual_address()),
            WalkResult::PageFault { level: fault } if fault == level
        ));
        assert_eq!(memory.writes(), 0);
    }
}

#[test]
fn variants_keep_copy_clone_and_partial_eq() {
    fn copy_and_clone<T: Copy + Clone>() {}
    copy_and_clone::<WalkResult>();
    assert_ne!(
        MmuError::WalkProtocolFault(1),
        MmuError::WalkUnknownFault(1)
    );
    assert_ne!(MmuError::AccessFault(1), MmuError::WalkHostFault(1));
}
