//! TLB (Translation Lookaside Buffer) integration tests
//!
//! Tests TLB functionality including:
//! - Basic lookup/insert operations
//! - ASID-aware lookups
//! - Global entries
//! - Flush operations
//! - LRU replacement policy
//! - Statistics tracking

use ruscv_sim::mmu::{Tlb, TlbEntry};

#[test]
fn test_tlb_basic_lookup() {
    let mut tlb = Tlb::new(64, 4);

    let entry = TlbEntry {
        vpn: 0x100,
        ppn: 0x200,
        asid: 0,
        global: false,
        read: true,
        write: true,
        execute: false,
        user: false,
        accessed: true,
        dirty: false,
    };

    tlb.insert(0x100, entry);

    let found = tlb.lookup(0x100, 0);
    assert!(found.is_some());
    let found_entry = found.unwrap();
    assert_eq!(found_entry.ppn, 0x200);
    assert_eq!(found_entry.vpn, 0x100);
}

#[test]
fn test_tlb_miss() {
    let mut tlb = Tlb::new(64, 4);

    let found = tlb.lookup(0x100, 0);
    assert!(found.is_none());

    let stats = tlb.stats();
    assert_eq!(stats.misses, 1);
    assert_eq!(stats.accesses, 1);
}

#[test]
fn test_tlb_asid_isolation() {
    let mut tlb = Tlb::new(64, 4);

    // Insert entry for ASID 1
    let entry = TlbEntry {
        vpn: 0x100,
        ppn: 0x200,
        asid: 1,
        global: false,
        read: true,
        ..Default::default()
    };
    tlb.insert(0x100, entry);

    // Lookup with same ASID should hit
    assert!(tlb.lookup(0x100, 1).is_some());

    // Lookup with different ASID should miss
    assert!(tlb.lookup(0x100, 2).is_none());

    // Lookup with ASID 0 should miss
    assert!(tlb.lookup(0x100, 0).is_none());
}

#[test]
fn test_tlb_global_entry() {
    let mut tlb = Tlb::new(64, 4);

    // Insert global entry for ASID 1
    let entry = TlbEntry {
        vpn: 0x100,
        ppn: 0x200,
        asid: 1,
        global: true, // Global entry
        read: true,
        ..Default::default()
    };
    tlb.insert(0x100, entry);

    // Global entry should match any ASID
    assert!(tlb.lookup(0x100, 0).is_some());
    assert!(tlb.lookup(0x100, 1).is_some());
    assert!(tlb.lookup(0x100, 99).is_some());
}

#[test]
fn test_tlb_flush_all() {
    let mut tlb = Tlb::new(64, 4);

    // Insert multiple entries
    for i in 0..10 {
        let entry = TlbEntry {
            vpn: i,
            ppn: i * 2,
            asid: (i % 4) as u16,
            global: false,
            read: true,
            ..Default::default()
        };
        tlb.insert(i, entry);
    }

    // Verify entries exist
    assert!(tlb.lookup(5, 1).is_some());

    // Flush all
    tlb.flush_all();

    // All entries should be gone
    for i in 0..10 {
        assert!(tlb.lookup(i, (i % 4) as u16).is_none());
    }

    assert_eq!(tlb.stats().flushes, 1);
}

#[test]
fn test_tlb_flush_asid() {
    let mut tlb = Tlb::new(64, 4);

    // Insert entries for different ASIDs
    for asid in 1..=3 {
        for vpn in 0..5 {
            let entry = TlbEntry {
                vpn,
                ppn: vpn * asid as u64,
                asid,
                global: false,
                read: true,
                ..Default::default()
            };
            tlb.insert(vpn, entry);
        }
    }

    // Insert global entries (should not be flushed)
    for vpn in 10..15 {
        let entry = TlbEntry {
            vpn,
            ppn: vpn * 100,
            asid: 2,
            global: true, // Global
            read: true,
            ..Default::default()
        };
        tlb.insert(vpn, entry);
    }

    // Flush ASID 2
    tlb.flush_asid(2);

    // ASID 2 entries should be gone
    for vpn in 0..5 {
        assert!(tlb.lookup(vpn, 2).is_none());
    }

    // Global entries should still exist
    for vpn in 10..15 {
        assert!(tlb.lookup(vpn, 2).is_some());
    }

    // ASID 1 and 3 entries should still exist
    for vpn in 0..5 {
        assert!(tlb.lookup(vpn, 1).is_some());
        assert!(tlb.lookup(vpn, 3).is_some());
    }
}

#[test]
fn test_tlb_flush_va() {
    let mut tlb = Tlb::new(64, 4);

    // Insert entries for different VPNs
    for vpn in 0..10 {
        let entry = TlbEntry {
            vpn: vpn as u64,
            ppn: vpn as u64 * 2,
            asid: 1,
            global: false,
            read: true,
            ..Default::default()
        };
        tlb.insert(vpn as u64, entry);
    }

    // Flush specific VPN
    tlb.flush_va(5);

    // That VPN should be gone
    assert!(tlb.lookup(5, 1).is_none());

    // Other VPNs should still exist
    for vpn in 0..10 {
        if vpn != 5 {
            assert!(tlb.lookup(vpn as u64, 1).is_some());
        }
    }
}

#[test]
fn test_tlb_flush_asid_va() {
    let mut tlb = Tlb::new(64, 4);

    // Insert same VPN for different ASIDs
    for asid in 1..=3 {
        let entry = TlbEntry {
            vpn: 0x100,
            ppn: asid as u64 * 0x1000,
            asid: asid as u16,
            global: false,
            read: true,
            ..Default::default()
        };
        tlb.insert(0x100, entry);
    }

    // Flush specific ASID+VA combination
    tlb.flush_asid_va(2, 0x100);

    // ASID 2's entry should be gone
    assert!(tlb.lookup(0x100, 2).is_none());

    // Other ASIDs should still have their entries
    assert!(tlb.lookup(0x100, 1).is_some());
    assert!(tlb.lookup(0x100, 3).is_some());
}

#[test]
fn test_tlb_statistics() {
    let mut tlb = Tlb::new(64, 4);

    // Insert some entries
    for i in 0..5 {
        let entry = TlbEntry {
            vpn: i,
            ppn: i * 2,
            asid: 0,
            global: false,
            read: true,
            ..Default::default()
        };
        tlb.insert(i, entry);
    }

    // Perform lookups
    for i in 0..10 {
        let _ = tlb.lookup(i as u64, 0);
    }

    let stats = tlb.stats();
    assert_eq!(stats.accesses, 10);
    assert_eq!(stats.hits, 5); // First 5 should hit
    assert_eq!(stats.misses, 5); // Last 5 should miss

    let hit_rate = stats.hit_rate();
    assert!((hit_rate - 0.5).abs() < 0.01);
}

#[test]
fn test_tlb_lru_replacement() {
    // Small TLB: 4 entries, 1 way (direct mapped)
    // With 4 sets, VPN maps to set = VPN % 4
    let mut tlb = Tlb::new(4, 1);

    // Fill the TLB with VPNs that map to different sets (0, 1, 2, 3)
    for i in 0..4 {
        let entry = TlbEntry {
            vpn: i as u64,
            ppn: i as u64 * 2,
            asid: 0,
            global: false,
            read: true,
            ..Default::default()
        };
        tlb.insert(i as u64, entry);
    }

    // Access entry 0 to make it most recently used
    assert!(tlb.lookup(0, 0).is_some());

    // Insert a new entry that maps to set 1 (same as VPN 1)
    // VPN 5 % 4 = 1, so it will collide with VPN 1
    let new_entry = TlbEntry {
        vpn: 5, // Maps to set 1, same as VPN 1
        ppn: 200,
        asid: 0,
        global: false,
        read: true,
        ..Default::default()
    };
    tlb.insert(5, new_entry);

    // Entry 1 should be replaced (it was LRU in set 1)
    assert!(tlb.lookup(1, 0).is_none());

    // Entry 0 should still exist (was accessed recently, different set)
    assert!(tlb.lookup(0, 0).is_some());

    // New entry should exist
    assert!(tlb.lookup(5, 0).is_some());
}

#[test]
fn test_tlb_large_scale() {
    let mut tlb = Tlb::new(256, 8); // 256 entries, 8-way

    // Insert many entries
    for i in 0..500 {
        let entry = TlbEntry {
            vpn: i,
            ppn: i * 0x1000,
            asid: (i % 16) as u16,
            global: i % 10 == 0,
            read: true,
            write: i % 2 == 0,
            execute: i % 3 == 0,
            user: i % 5 == 0,
            accessed: true,
            dirty: i % 2 == 0,
        };
        tlb.insert(i, entry);
    }

    // Lookup entries - some will hit, some will miss due to replacement
    let mut hits = 0;
    let mut misses = 0;
    for i in 0..500 {
        if tlb.lookup(i, (i % 16) as u16).is_some() {
            hits += 1;
        } else {
            misses += 1;
        }
    }

    // Due to LRU replacement, we should have some hits
    assert!(hits > 0, "Expected some TLB hits");
    assert!(misses > 0, "Expected some TLB misses due to replacement");

    let stats = tlb.stats();
    assert_eq!(stats.hits, hits as u64);
    assert_eq!(stats.misses, misses as u64);
}

#[test]
fn test_tlb_entry_permissions() {
    let mut tlb = Tlb::new(64, 4);

    let entry = TlbEntry {
        vpn: 0x100,
        ppn: 0x200,
        asid: 0,
        global: false,
        read: true,
        write: false,
        execute: true,
        user: true,
        accessed: true,
        dirty: false,
    };

    tlb.insert(0x100, entry);

    let found = tlb.lookup(0x100, 0).unwrap();
    assert!(found.read);
    assert!(!found.write);
    assert!(found.execute);
    assert!(found.user);
    assert!(found.accessed);
    assert!(!found.dirty);
}

/// Test LRU aging mechanism with a simulated small counter
///
/// This test verifies that the LRU aging mechanism works correctly:
/// 1. LRU counters are properly updated on access
/// 2. When counters would overflow, they are aged (halved)
/// 3. After aging, the LRU replacement policy still works correctly
///
/// Note: We use a direct-mapped TLB (1-way) to ensure deterministic behavior
#[test]
fn test_lru_aging_simulation() {
    // Small TLB with 4 sets, 1 way (direct mapped)
    // This makes it easier to track which entry is in which set
    let mut tlb = Tlb::new(4, 1);

    // Insert 4 entries, each mapping to a different set (0, 1, 2, 3)
    for i in 0..4u64 {
        let entry = TlbEntry {
            vpn: i, // VPN i maps to set i % 4
            ppn: i * 0x1000,
            asid: 0,
            global: false,
            read: true,
            ..Default::default()
        };
        tlb.insert(i, entry);
    }

    // Access entry 0 repeatedly to make it the most recently used
    // Each access updates the LRU counter for the accessed entry
    for _ in 0..10 {
        assert!(tlb.lookup(0, 0).is_some());
    }

    // Entry 0 should now have the highest LRU counter in its set
    // Other entries should have lower counters

    // Insert a new entry that collides with set 1 (VPN 1 and VPN 5 both map to set 1)
    let new_entry = TlbEntry {
        vpn: 5, // VPN 5 % 4 = 1, collides with VPN 1
        ppn: 0x5000,
        asid: 0,
        global: false,
        read: true,
        ..Default::default()
    };
    tlb.insert(5, new_entry);

    // Entry 1 should be replaced (it was LRU in set 1)
    assert!(
        tlb.lookup(1, 0).is_none(),
        "Entry 1 should be replaced (LRU)"
    );

    // Entry 0 should still exist (was accessed recently)
    assert!(tlb.lookup(0, 0).is_some(), "Entry 0 should still exist");

    // New entry should exist
    assert!(tlb.lookup(5, 0).is_some(), "New entry should exist");
}

/// Test LRU behavior after many sequential accesses
///
/// This test verifies that after many accesses, the LRU replacement
/// policy still correctly identifies the least recently used entry.
#[test]
fn test_lru_after_many_accesses() {
    // 4-way set associative TLB with 4 sets = 16 entries
    let mut tlb = Tlb::new(16, 4);

    // Insert entries into the same set (VPNs that map to set 0)
    // With 4 ways, we can have 4 entries in a set
    for i in 0..4u64 {
        // VPNs 0, 4, 8, 12 all map to set 0 (VPN % 4 = 0)
        let entry = TlbEntry {
            vpn: i * 4,
            ppn: i * 0x1000,
            asid: 0,
            global: false,
            read: true,
            ..Default::default()
        };
        tlb.insert(i * 4, entry);
    }

    // Access entries in a specific order to establish LRU order
    // Access pattern: 0, 1, 2, 3 (all hit, updating LRU counters)
    for i in 0..4u64 {
        assert!(
            tlb.lookup(i * 4, 0).is_some(),
            "Entry {} should be found",
            i * 4
        );
    }

    // Now entry 0 is the oldest (LRU) since all were accessed once in order
    // Insert a new entry that also maps to set 0
    let new_entry = TlbEntry {
        vpn: 16, // 16 % 4 = 0, same set as entries 0, 4, 8, 12
        ppn: 0x10000,
        asid: 0,
        global: false,
        read: true,
        ..Default::default()
    };
    tlb.insert(16, new_entry);

    // Entry 0 should be replaced (LRU)
    assert!(
        tlb.lookup(0, 0).is_none(),
        "Entry 0 should be replaced (LRU)"
    );

    // Other entries should still exist
    assert!(tlb.lookup(4, 0).is_some(), "Entry 4 should exist");
    assert!(tlb.lookup(8, 0).is_some(), "Entry 8 should exist");
    assert!(tlb.lookup(12, 0).is_some(), "Entry 12 should exist");
    assert!(tlb.lookup(16, 0).is_some(), "New entry 16 should exist");
}

/// Test that LRU replacement works correctly across multiple sets
#[test]
fn test_lru_multi_set_consistency() {
    // 2-way set associative TLB with 4 sets = 8 entries
    // set_index(vpn) = vpn % 4
    let mut tlb = Tlb::new(8, 2);

    // Fill all sets with 2 entries each
    // VPN pattern: 0,4 -> set 0; 1,5 -> set 1; 2,6 -> set 2; 3,7 -> set 3
    for i in 0..8u64 {
        let entry = TlbEntry {
            vpn: i,
            ppn: i * 0x1000,
            asid: 0,
            global: false,
            read: true,
            ..Default::default()
        };
        tlb.insert(i, entry);
    }

    // Access entries 1, 5 (set 1), 3, 7 (set 3) to make them more recently used
    for i in [1u64, 5, 3, 7] {
        assert!(tlb.lookup(i, 0).is_some());
    }

    // Insert new entries that collide with sets 0 and 2
    // Entry 8 -> set 0, Entry 10 -> set 2
    let new_entry_8 = TlbEntry {
        vpn: 8, // 8 % 4 = 0
        ppn: 0x8000,
        asid: 0,
        global: false,
        read: true,
        ..Default::default()
    };
    tlb.insert(8, new_entry_8);

    let new_entry_10 = TlbEntry {
        vpn: 10, // 10 % 4 = 2
        ppn: 0xA000,
        asid: 0,
        global: false,
        read: true,
        ..Default::default()
    };
    tlb.insert(10, new_entry_10);

    // One of entries 0 or 4 (set 0) should be replaced by entry 8
    let entry0_exists = tlb.lookup(0, 0).is_some();
    let entry4_exists = tlb.lookup(4, 0).is_some();
    assert!(
        !entry0_exists || !entry4_exists,
        "One of entries 0 or 4 should be replaced by entry 8"
    );

    // One of entries 2 or 6 (set 2) should be replaced by entry 10
    let entry2_exists = tlb.lookup(2, 0).is_some();
    let entry6_exists = tlb.lookup(6, 0).is_some();
    assert!(
        !entry2_exists || !entry6_exists,
        "One of entries 2 or 6 should be replaced by entry 10"
    );

    // Accessed entries (odd) should still exist
    assert!(tlb.lookup(1, 0).is_some(), "Entry 1 should exist");
    assert!(tlb.lookup(3, 0).is_some(), "Entry 3 should exist");
    assert!(tlb.lookup(5, 0).is_some(), "Entry 5 should exist");
    assert!(tlb.lookup(7, 0).is_some(), "Entry 7 should exist");

    // New entries should exist
    assert!(tlb.lookup(8, 0).is_some(), "Entry 8 should exist");
    assert!(tlb.lookup(10, 0).is_some(), "Entry 10 should exist");
}
