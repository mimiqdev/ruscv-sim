//! Translation Lookaside Buffer (TLB) implementation

/// TLB entry
#[derive(Debug, Clone, Copy, Default)]
pub struct TlbEntry {
    pub vpn: u64,
    pub ppn: u64,
    pub asid: u16,
    pub global: bool,
    pub read: bool,
    pub write: bool,
    pub execute: bool,
    pub user: bool,
    pub accessed: bool,
    pub dirty: bool,
}

/// TLB statistics
#[derive(Debug, Clone, Copy, Default)]
pub struct TlbStats {
    pub accesses: u64,
    pub hits: u64,
    pub misses: u64,
    pub flushes: u64,
}

impl TlbStats {
    pub fn hit_rate(&self) -> f64 {
        if self.accesses == 0 {
            0.0
        } else {
            self.hits as f64 / self.accesses as f64
        }
    }
}

/// TLB with configurable size and associativity
pub struct Tlb {
    _size: usize,
    ways: usize,
    sets: usize,
    entries: Vec<Option<TlbEntry>>,
    /// LRU counters - use u32 to prevent overflow in high-frequency scenarios
    lru_counters: Vec<u32>,
    stats: TlbStats,
}

impl Tlb {
    /// Maximum LRU counter value before aging
    const MAX_LRU_COUNTER: u32 = u32::MAX / 2;

    pub fn new(size: usize, ways: usize) -> Self {
        let sets = size / ways;
        Self {
            _size: size,
            ways,
            sets,
            entries: vec![None; size],
            lru_counters: vec![0; size],
            stats: TlbStats::default(),
        }
    }

    fn set_index(&self, vpn: u64) -> usize {
        (vpn as usize) % self.sets
    }

    fn entry_index(&self, set: usize, way: usize) -> usize {
        set * self.ways + way
    }

    pub fn lookup(&mut self, vpn: u64, asid: u16) -> Option<TlbEntry> {
        self.stats.accesses += 1;

        let set = self.set_index(vpn);

        for way in 0..self.ways {
            let idx = self.entry_index(set, way);
            if let Some(entry) = self.entries[idx] {
                if entry.vpn == vpn && (entry.global || entry.asid == asid) {
                    self.stats.hits += 1;
                    self.update_lru(set, way);
                    return Some(entry);
                }
            }
        }

        self.stats.misses += 1;
        None
    }

    pub fn insert(&mut self, vpn: u64, entry: TlbEntry) {
        let set = self.set_index(vpn);
        let way = self.find_lru_way(set);
        let idx = self.entry_index(set, way);

        self.entries[idx] = Some(entry);
        self.update_lru(set, way);
    }

    fn find_lru_way(&self, set: usize) -> usize {
        let mut min_counter = u32::MAX;
        let mut min_way = 0;

        for way in 0..self.ways {
            let idx = self.entry_index(set, way);
            if self.entries[idx].is_none() {
                return way;
            }
            if self.lru_counters[idx] < min_counter {
                min_counter = self.lru_counters[idx];
                min_way = way;
            }
        }

        min_way
    }

    /// Update LRU counters for a set after an access
    ///
    /// Uses aging mechanism to prevent counter overflow:
    /// - Sets the accessed entry to MAX_LRU_COUNTER
    /// - Other entries are aged (decremented if > 0)
    /// - When any counter would exceed MAX_LRU_COUNTER, all counters are halved
    fn update_lru(&mut self, set: usize, used_way: usize) {
        // Check if we need to age all counters
        let used_idx = self.entry_index(set, used_way);
        if self.lru_counters[used_idx] >= Self::MAX_LRU_COUNTER {
            // Age all counters in this set by halving them
            for way in 0..self.ways {
                let idx = self.entry_index(set, way);
                self.lru_counters[idx] /= 2;
            }
        }

        // Update counters
        for way in 0..self.ways {
            let idx = self.entry_index(set, way);
            if way == used_way {
                self.lru_counters[idx] = Self::MAX_LRU_COUNTER;
            } else if self.lru_counters[idx] > 0 {
                self.lru_counters[idx] -= 1;
            }
        }
    }

    pub fn flush_all(&mut self) {
        for entry in &mut self.entries {
            *entry = None;
        }
        for counter in &mut self.lru_counters {
            *counter = 0;
        }
        self.stats.flushes += 1;
    }

    pub fn flush_asid(&mut self, asid: u16) {
        for entry in &mut self.entries {
            if let Some(e) = entry {
                if !e.global && e.asid == asid {
                    *entry = None;
                }
            }
        }
    }

    pub fn flush_va(&mut self, vpn: u64) {
        let set = self.set_index(vpn);
        for way in 0..self.ways {
            let idx = self.entry_index(set, way);
            if let Some(e) = self.entries[idx] {
                if e.vpn == vpn {
                    self.entries[idx] = None;
                }
            }
        }
    }

    pub fn flush_asid_va(&mut self, asid: u16, vpn: u64) {
        let set = self.set_index(vpn);
        for way in 0..self.ways {
            let idx = self.entry_index(set, way);
            if let Some(e) = self.entries[idx] {
                if e.vpn == vpn && (e.global || e.asid == asid) {
                    self.entries[idx] = None;
                }
            }
        }
    }

    pub fn stats(&self) -> TlbStats {
        self.stats
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tlb_basic() {
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
            accessed: false,
            dirty: false,
        };

        tlb.insert(0x100, entry);

        let found = tlb.lookup(0x100, 0);
        assert!(found.is_some());
        assert_eq!(found.unwrap().ppn, 0x200);

        assert_eq!(tlb.stats().hits, 1);
    }

    #[test]
    fn test_tlb_miss() {
        let mut tlb = Tlb::new(64, 4);

        let found = tlb.lookup(0x100, 0);
        assert!(found.is_none());
        assert_eq!(tlb.stats().misses, 1);
    }

    #[test]
    fn test_tlb_asid() {
        let mut tlb = Tlb::new(64, 4);

        let entry = TlbEntry {
            vpn: 0x100,
            ppn: 0x200,
            asid: 1,
            global: false,
            read: true,
            ..Default::default()
        };

        tlb.insert(0x100, entry);

        // Same ASID - hit
        assert!(tlb.lookup(0x100, 1).is_some());

        // Different ASID - miss
        assert!(tlb.lookup(0x100, 2).is_none());
    }

    #[test]
    fn test_tlb_global() {
        let mut tlb = Tlb::new(64, 4);

        let entry = TlbEntry {
            vpn: 0x100,
            ppn: 0x200,
            asid: 1,
            global: true,
            read: true,
            ..Default::default()
        };

        tlb.insert(0x100, entry);

        // Global entry matches any ASID
        assert!(tlb.lookup(0x100, 0).is_some());
        assert!(tlb.lookup(0x100, 99).is_some());
    }

    #[test]
    fn test_tlb_flush() {
        let mut tlb = Tlb::new(64, 4);

        let entry = TlbEntry {
            vpn: 0x100,
            ppn: 0x200,
            asid: 1,
            global: false,
            read: true,
            ..Default::default()
        };

        tlb.insert(0x100, entry);
        assert!(tlb.lookup(0x100, 1).is_some());

        tlb.flush_all();
        assert!(tlb.lookup(0x100, 1).is_none());
    }
}
