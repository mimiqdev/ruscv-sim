//! Instruction Dispatch Module
//!
//! Simplified dispatch architecture using:
//! - Single HashMap<InstructionKey, ExecutorFn> dispatch table
//! - LRU cache for hot path optimization
//! - Complete (opcode, funct3, funct7) matching for all instructions

use std::collections::HashMap;

use crate::core::CoreState;
use crate::decode::DecodedInstruction;
use crate::execute::{ExecuteError, Executor, ExecutorFn};
use crate::memory::MemoryInterface;

/// Instruction dispatch key
///
/// Uses complete (opcode, funct3, funct7) triplet for unambiguous instruction matching.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct InstructionKey {
    /// 7-bit opcode (0-127)
    pub opcode: u8,
    /// 3-bit funct3 (0-7), ALWAYS used
    pub funct3: u8,
    /// 7-bit funct7 (0-127), ALWAYS used
    pub funct7: u8,
}

impl InstructionKey {
    /// Create from decoded instruction
    pub fn from_instr(instr: &DecodedInstruction) -> Self {
        Self {
            opcode: instr.opcode as u8,
            funct3: instr.funct3.map_or(0, |f| f as u8),
            funct7: instr.funct7.unwrap_or(0),
        }
    }

    /// Create from raw instruction word
    pub fn from_raw(raw: u32) -> Self {
        Self {
            opcode: (raw & 0x7F) as u8,
            funct3: ((raw >> 12) & 0x7) as u8,
            funct7: ((raw >> 25) & 0x7F) as u8,
        }
    }

    /// Validate all fields are within range
    pub fn is_valid(&self) -> bool {
        self.opcode < 128 && self.funct3 < 8 && self.funct7 < 128
    }
}

/// LRU Cache entry
#[derive(Clone, Copy)]
struct LruEntry {
    key: InstructionKey,
    executor: ExecutorFn,
}

/// LRU Cache for hot instruction dispatch
///
/// Simple ring-buffer based LRU cache implementation.
pub struct LruCache {
    /// Maximum cache entries
    capacity: usize,
    /// Cache entries
    entries: Vec<LruEntry>,
    /// Current size
    size: usize,
}

impl LruCache {
    /// Create new LRU cache with specified capacity
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity,
            entries: Vec::with_capacity(capacity),
            size: 0,
        }
    }

    /// Get executor from cache, updating LRU order
    pub fn get(&mut self, key: &InstructionKey) -> Option<ExecutorFn> {
        for i in 0..self.size {
            if self.entries[i].key == *key {
                let executor = self.entries[i].executor;
                let entry = self.entries.remove(i);
                self.entries.push(entry);
                return Some(executor);
            }
        }
        None
    }

    /// Insert new entry, evicting LRU if at capacity
    pub fn insert(&mut self, key: InstructionKey, executor: ExecutorFn) {
        self.entries.push(LruEntry { key, executor });
        if self.entries.len() > self.capacity {
            self.entries.remove(0);
        }
        self.size = self.entries.len();
    }

    /// Get current size
    pub fn len(&self) -> usize {
        self.size
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.size == 0
    }

    /// Clear cache
    pub fn clear(&mut self) {
        self.entries.clear();
        self.size = 0;
    }
}

/// Main Dispatcher struct
pub struct Dispatcher {
    /// Main dispatch table: complete (opcode, funct3, funct7) → executor
    dispatch_table: HashMap<InstructionKey, ExecutorFn>,
    /// LRU cache for hot instructions
    cache: LruCache,
}

impl Dispatcher {
    /// Create new dispatcher with specified cache capacity
    pub fn new(cache_capacity: usize) -> Self {
        let mut dispatch_table = HashMap::new();
        let cache = LruCache::new(cache_capacity);

        // Register all base RISC-V instructions
        Self::register_rv32i_base(&mut dispatch_table);

        Self {
            dispatch_table,
            cache,
        }
    }

    /// Create new dispatcher with default cache capacity (32 entries)
    pub fn with_default_cache() -> Self {
        Self::new(32)
    }

    /// Main dispatch function
    pub fn dispatch(
        &mut self,
        executor: &Executor,
        instr: &DecodedInstruction,
        state: &mut CoreState,
        mem: &mut dyn MemoryInterface,
    ) -> Result<(), ExecuteError> {
        let key = InstructionKey::from_instr(instr);

        // Step 1: Check LRU cache
        if let Some(exec_fn) = self.cache.get(&key) {
            return exec_fn(executor, instr, state, mem);
        }

        // Step 2: Lookup main table
        if let Some(exec_fn) = self.dispatch_table.get(&key).copied() {
            // Step 3: Update cache
            self.cache.insert(key, exec_fn);
            return exec_fn(executor, instr, state, mem);
        }

        Err(ExecuteError::InvalidOperation)
    }

    /// Register a new instruction
    pub fn register(&mut self, opcode: u8, funct3: u8, funct7: u8, executor: ExecutorFn) {
        let key = InstructionKey {
            opcode,
            funct3,
            funct7,
        };
        self.dispatch_table.insert(key, executor);
    }

    /// Register RISC-V base instructions (RV32I)
    fn register_rv32i_base(table: &mut HashMap<InstructionKey, ExecutorFn>) {
        // LUI (opcode=0x37, funct3=0, funct7=0)
        table.insert(
            InstructionKey {
                opcode: 0x37,
                funct3: 0,
                funct7: 0,
            },
            Executor::exec_lui,
        );

        // AUIPC (opcode=0x17, funct3=0, funct7=0)
        table.insert(
            InstructionKey {
                opcode: 0x17,
                funct3: 0,
                funct7: 0,
            },
            Executor::exec_auipc,
        );

        // JAL (opcode=0x6F, funct3=0, funct7=0)
        table.insert(
            InstructionKey {
                opcode: 0x6F,
                funct3: 0,
                funct7: 0,
            },
            Executor::exec_jal,
        );

        // JALR (opcode=0x67, funct3=0, funct7=0)
        table.insert(
            InstructionKey {
                opcode: 0x67,
                funct3: 0,
                funct7: 0,
            },
            Executor::exec_jalr,
        );

        // BRANCH: BEQ, BNE, BLT, BGE, BLTU, BGEU (opcode=0x63)
        table.insert(
            InstructionKey {
                opcode: 0x63,
                funct3: 0b000,
                funct7: 0,
            },
            Executor::exec_branch,
        );
        table.insert(
            InstructionKey {
                opcode: 0x63,
                funct3: 0b001,
                funct7: 0,
            },
            Executor::exec_branch,
        );
        table.insert(
            InstructionKey {
                opcode: 0x63,
                funct3: 0b100,
                funct7: 0,
            },
            Executor::exec_branch,
        );
        table.insert(
            InstructionKey {
                opcode: 0x63,
                funct3: 0b101,
                funct7: 0,
            },
            Executor::exec_branch,
        );
        table.insert(
            InstructionKey {
                opcode: 0x63,
                funct3: 0b110,
                funct7: 0,
            },
            Executor::exec_branch,
        );
        table.insert(
            InstructionKey {
                opcode: 0x63,
                funct3: 0b111,
                funct7: 0,
            },
            Executor::exec_branch,
        );

        // LOAD: LB, LH, LW, LBU, LHU (opcode=0x03)
        table.insert(
            InstructionKey {
                opcode: 0x03,
                funct3: 0b000,
                funct7: 0,
            },
            Executor::exec_load,
        );
        table.insert(
            InstructionKey {
                opcode: 0x03,
                funct3: 0b001,
                funct7: 0,
            },
            Executor::exec_load,
        );
        table.insert(
            InstructionKey {
                opcode: 0x03,
                funct3: 0b010,
                funct7: 0,
            },
            Executor::exec_load,
        );
        table.insert(
            InstructionKey {
                opcode: 0x03,
                funct3: 0b100,
                funct7: 0,
            },
            Executor::exec_load,
        );
        table.insert(
            InstructionKey {
                opcode: 0x03,
                funct3: 0b101,
                funct7: 0,
            },
            Executor::exec_load,
        );

        // STORE: SB, SH, SW (opcode=0x23)
        table.insert(
            InstructionKey {
                opcode: 0x23,
                funct3: 0b000,
                funct7: 0,
            },
            Executor::exec_store,
        );
        table.insert(
            InstructionKey {
                opcode: 0x23,
                funct3: 0b001,
                funct7: 0,
            },
            Executor::exec_store,
        );
        table.insert(
            InstructionKey {
                opcode: 0x23,
                funct3: 0b010,
                funct7: 0,
            },
            Executor::exec_store,
        );

        // OP-IMM (opcode=0x13)
        table.insert(
            InstructionKey {
                opcode: 0x13,
                funct3: 0b000,
                funct7: 0,
            },
            Executor::exec_op_imm,
        );
        table.insert(
            InstructionKey {
                opcode: 0x13,
                funct3: 0b010,
                funct7: 0,
            },
            Executor::exec_op_imm,
        );
        table.insert(
            InstructionKey {
                opcode: 0x13,
                funct3: 0b011,
                funct7: 0,
            },
            Executor::exec_op_imm,
        );
        table.insert(
            InstructionKey {
                opcode: 0x13,
                funct3: 0b100,
                funct7: 0,
            },
            Executor::exec_op_imm,
        );
        table.insert(
            InstructionKey {
                opcode: 0x13,
                funct3: 0b110,
                funct7: 0,
            },
            Executor::exec_op_imm,
        );
        table.insert(
            InstructionKey {
                opcode: 0x13,
                funct3: 0b111,
                funct7: 0,
            },
            Executor::exec_op_imm,
        );
        table.insert(
            InstructionKey {
                opcode: 0x13,
                funct3: 0b001,
                funct7: 0,
            },
            Executor::exec_op_imm,
        );
        table.insert(
            InstructionKey {
                opcode: 0x13,
                funct3: 0b101,
                funct7: 0b0000000,
            },
            Executor::exec_op_imm,
        );
        table.insert(
            InstructionKey {
                opcode: 0x13,
                funct3: 0b101,
                funct7: 0b0100000,
            },
            Executor::exec_op_imm,
        );

        // OP (opcode=0x33)
        table.insert(
            InstructionKey {
                opcode: 0x33,
                funct3: 0b000,
                funct7: 0b0000000,
            },
            Executor::exec_op,
        );
        table.insert(
            InstructionKey {
                opcode: 0x33,
                funct3: 0b000,
                funct7: 0b0100000,
            },
            Executor::exec_op,
        );
        table.insert(
            InstructionKey {
                opcode: 0x33,
                funct3: 0b001,
                funct7: 0,
            },
            Executor::exec_op,
        );
        table.insert(
            InstructionKey {
                opcode: 0x33,
                funct3: 0b010,
                funct7: 0,
            },
            Executor::exec_op,
        );
        table.insert(
            InstructionKey {
                opcode: 0x33,
                funct3: 0b011,
                funct7: 0,
            },
            Executor::exec_op,
        );
        table.insert(
            InstructionKey {
                opcode: 0x33,
                funct3: 0b100,
                funct7: 0,
            },
            Executor::exec_op,
        );
        table.insert(
            InstructionKey {
                opcode: 0x33,
                funct3: 0b101,
                funct7: 0b0000000,
            },
            Executor::exec_op,
        );
        table.insert(
            InstructionKey {
                opcode: 0x33,
                funct3: 0b101,
                funct7: 0b0100000,
            },
            Executor::exec_op,
        );
        table.insert(
            InstructionKey {
                opcode: 0x33,
                funct3: 0b110,
                funct7: 0,
            },
            Executor::exec_op,
        );
        table.insert(
            InstructionKey {
                opcode: 0x33,
                funct3: 0b111,
                funct7: 0,
            },
            Executor::exec_op,
        );

        // SYSTEM (opcode=0x73)
        table.insert(
            InstructionKey {
                opcode: 0x73,
                funct3: 0,
                funct7: 0,
            },
            Executor::exec_system,
        );
        table.insert(
            InstructionKey {
                opcode: 0x73,
                funct3: 0,
                funct7: 0b0010000,
            },
            Executor::exec_system,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_instruction_key_creation() {
        let key = InstructionKey {
            opcode: 0x33,
            funct3: 0b000,
            funct7: 0b0000000,
        };
        assert_eq!(key.opcode, 0x33);
        assert!(key.is_valid());
    }

    #[test]
    fn test_instruction_key_from_instr() {
        use crate::decode::{InstructionDecoder, InstructionFormat, Opcode};

        let raw = 0x00_00_00_33u32; // ADD x0, x0, x0 (simplified)
        let decoder = InstructionDecoder::new();
        let instr = decoder.decode(raw).unwrap();

        let key = InstructionKey::from_instr(&instr);
        assert_eq!(key.opcode, 0x33);
    }

    #[test]
    fn test_lru_cache_basic() {
        let mut cache = LruCache::new(4);

        let key1 = InstructionKey {
            opcode: 0x13,
            funct3: 0,
            funct7: 0,
        };
        let key2 = InstructionKey {
            opcode: 0x13,
            funct3: 1,
            funct7: 0,
        };

        // Insert entries
        let dummy_executor: ExecutorFn = |_, _, _, _| Ok(());
        cache.insert(key1, dummy_executor);
        cache.insert(key2, dummy_executor);

        assert_eq!(cache.len(), 2);
        assert!(!cache.is_empty());
    }

    #[test]
    fn test_lru_cache_eviction() {
        let mut cache = LruCache::new(3);

        let key1 = InstructionKey {
            opcode: 0x13,
            funct3: 0,
            funct7: 0,
        };
        let key2 = InstructionKey {
            opcode: 0x13,
            funct3: 1,
            funct7: 0,
        };
        let key3 = InstructionKey {
            opcode: 0x13,
            funct3: 2,
            funct7: 0,
        };
        let key4 = InstructionKey {
            opcode: 0x13,
            funct3: 3,
            funct7: 0,
        };

        let dummy_executor: ExecutorFn = |_, _, _, _| Ok(());

        // Fill cache
        cache.insert(key1, dummy_executor);
        cache.insert(key2, dummy_executor);
        cache.insert(key3, dummy_executor);
        assert_eq!(cache.len(), 3);

        // Add 4th entry, should evict key1 (LRU)
        cache.insert(key4, dummy_executor);
        assert_eq!(cache.len(), 3);

        // key1 should be evicted
        assert!(cache.get(&key1).is_none());
        // key4 should be present
        assert!(cache.get(&key4).is_some());
    }

    #[test]
    fn test_lru_cache_clear() {
        let mut cache = LruCache::new(4);

        let key = InstructionKey {
            opcode: 0x13,
            funct3: 0,
            funct7: 0,
        };
        let dummy_executor: ExecutorFn = |_, _, _, _| Ok(());

        cache.insert(key, dummy_executor);
        assert_eq!(cache.len(), 1);

        cache.clear();
        assert!(cache.is_empty());
    }

    #[test]
    fn test_dispatcher_creation() {
        let dispatcher = Dispatcher::with_default_cache();
        assert_eq!(dispatcher.cache.len(), 0);

        // Verify dispatch table has entries
        assert!(!dispatcher.dispatch_table.is_empty());
    }

    #[test]
    fn test_dispatcher_custom_capacity() {
        let dispatcher = Dispatcher::new(16);
        assert_eq!(dispatcher.cache.len(), 0);
    }

    #[test]
    fn test_register_instruction() {
        let mut dispatcher = Dispatcher::with_default_cache();

        let dummy_executor: ExecutorFn = |_, _, _, _| Ok(());

        // Register custom instruction
        dispatcher.register(0x10, 0, 0, dummy_executor);

        let key = InstructionKey {
            opcode: 0x10,
            funct3: 0,
            funct7: 0,
        };
        assert!(dispatcher.dispatch_table.contains_key(&key));
    }
}
