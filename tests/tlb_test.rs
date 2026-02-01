//! TLB (Translation Lookaside Buffer) integration tests
//!
//! Tests TLB functionality including:
//! - Basic lookup/insert operations
//! - ASID-aware lookups
//! - Global entries
//! - Flush operations
//! - LRU replacement policy
//! - Statistics tracking

use ruscv_sim::mmu::{Tlb, TlbEntry, TlbStats};

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
        vpn: 5,  // Maps to set 1, same as VPN 1
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
