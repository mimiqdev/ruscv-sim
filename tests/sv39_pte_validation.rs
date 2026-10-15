//! Component validation; these tests do not establish public-path translation.
use ruscv_sim::core::PrivilegeMode;
use ruscv_sim::mmu::physical::PhysicalMemory;
use ruscv_sim::mmu::pte::{flags, PagePermissions, PageTableEntry};
use ruscv_sim::mmu::sv39::{PageTableWalker, VirtualAddress, WalkResult};
use ruscv_sim::mmu::AccessType;

fn check(level: usize, raw: u64, valid: bool) {
    let mut memory = PhysicalMemory::new(0x1000, 0x3000);
    for current in (level + 1..3).rev() {
        let table = 3 - current;
        memory
            .write_dword(
                table as u64 * 0x1000,
                PageTableEntry::new_pointer(table as u64 + 1).bits(),
            )
            .unwrap();
    }
    let address = (3 - level) as u64 * 0x1000;
    memory.write_dword(address, raw).unwrap();
    let result = PageTableWalker::new(&mut memory, 1).walk_check_permissions_and_update_ad(
        VirtualAddress::new(0).unwrap(),
        AccessType::Read,
        PrivilegeMode::Supervisor,
        0,
    );
    if valid {
        assert!(
            matches!(
                result,
                WalkResult::Success {
                    paddr: 0x4000_0000,
                    ..
                }
            ),
            "{result:?}"
        );
        assert_eq!(memory.read_dword(address).unwrap(), raw | flags::A);
    } else {
        assert!(
            matches!(result, WalkResult::PageFault { level: actual } if actual == level),
            "{result:?}"
        );
        assert_eq!(
            memory.read_dword(address).unwrap(),
            raw,
            "rejected PTE must not gain A/D"
        );
    }
}

#[test]
fn reserved_high_bits_fault_at_every_level() {
    for level in 0..3 {
        let leaf = PageTableEntry::new_leaf(0x40000, PagePermissions::rw(), false).bits();
        for bit in 54..64 {
            check(level, leaf | (1 << bit), false);
        }
        check(level, leaf | flags::RSW_MASK | flags::G, true);
    }
}

#[test]
fn nonleaf_reserved_flags_and_terminal_pointer_fault() {
    for level in 0..3 {
        for flag in [flags::D, flags::A, flags::U] {
            check(level, PageTableEntry::new_pointer(3).bits() | flag, false);
        }
    }
    check(0, PageTableEntry::new_pointer(3).bits(), false);
}

#[test]
fn misaligned_superpage_and_reserved_write_permission_fault() {
    for level in 0..3 {
        check(level, flags::V | flags::W, false);
        check(level, 0, false);
        if level != 0 {
            for bit in 0..9 * level {
                let leaf =
                    PageTableEntry::new_leaf(0x40000 | (1 << bit), PagePermissions::rw(), false);
                check(level, leaf.bits(), false);
            }
        }
    }
}
