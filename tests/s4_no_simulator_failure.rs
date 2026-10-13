//! No guest instruction is a simulator failure (dev-plan §3 profile).
//!
//! Every 32-bit encoding is either an instruction of the RV64IMA + Zicsr +
//! M/S/U profile, which retires or takes its architectural exception, or
//! an encoding the profile does not implement, which is an
//! illegal-instruction trap. None of them may end the step as a
//! `SimulatorFailure`, in particular not as
//! `SimulatorFailureKind::UnsupportedLegalInstruction`.
//!
//! The sweep runs one step per encoding in M, S, and U on a Hart with the
//! validated physical ports, with every source register pointing into RAM
//! so loads, stores, and AMOs reach the data port. It covers every major
//! opcode with every funct3, every funct7, and a spread of register fields;
//! every CSR address with every Zicsr funct3; and a deterministic pseudo-
//! random sample of the whole 32-bit space.

use ruscv_sim::core::{PrivilegeMode, RiscvCore, StepOutcome};
use ruscv_sim::memory::{MemoryInterface, SimpleMemory};
use ruscv_sim::physical::{NativeRamBackend, ValidatedPhysicalAccess};
use std::sync::{Arc, Mutex};

const RAM_SIZE: usize = 0x2_0000;
const BASE: u64 = 0x8000_0000;
/// Storage offset of the single instruction under test.
const CODE: u64 = 0x1000;
/// Every GPR points here, inside RAM and 8-byte aligned, far from `CODE`.
const DATA: u64 = BASE + 0x1_0000;
const MODES: [PrivilegeMode; 3] = [
    PrivilegeMode::Machine,
    PrivilegeMode::Supervisor,
    PrivilegeMode::User,
];

struct Sweep {
    core: RiscvCore,
    ram: Arc<Mutex<SimpleMemory>>,
    steps: u64,
    retired: u64,
    trapped: u64,
}

impl Sweep {
    fn new() -> Self {
        let ram = Arc::new(Mutex::new(SimpleMemory::new(RAM_SIZE)));
        let typed: Arc<Mutex<dyn MemoryInterface + Send + Sync>> = ram.clone();
        let fetch = Arc::new(Mutex::new(ValidatedPhysicalAccess::new(
            NativeRamBackend::new(ram.clone(), 0, RAM_SIZE),
        )));
        let data = Arc::new(Mutex::new(ValidatedPhysicalAccess::new(
            NativeRamBackend::new(ram.clone(), 0, RAM_SIZE),
        )));
        let core = RiscvCore::new_with_physical_ports(typed.clone(), typed, fetch, data);
        Self {
            core,
            ram,
            steps: 0,
            retired: 0,
            trapped: 0,
        }
    }

    /// Runs `raw` once in every mode from reset and fails on any
    /// `SimulatorFailure`.
    fn check(&mut self, raw: u32) {
        self.ram.lock().unwrap().write_word(CODE, raw).unwrap();
        for mode in MODES {
            self.core.reset(BASE + CODE, BASE);
            let state = self.core.state_mut();
            for register in 1..32 {
                state.regs[register] = DATA;
            }
            state.privilege = mode;
            state.csr.set_privilege(mode);
            let outcome = self.core.step_outcome();
            self.steps += 1;
            match outcome {
                StepOutcome::InstructionRetired(_) => self.retired += 1,
                StepOutcome::TrapEntered(_) => self.trapped += 1,
                StepOutcome::SimulatorFailure(failure) => {
                    panic!("encoding {raw:#010x} in {mode:?} is a simulator failure: {failure:?}")
                }
            }
        }
    }
}

fn encode(opcode: u32, rd: u32, funct3: u32, rs1: u32, rs2: u32, funct7: u32) -> u32 {
    (funct7 << 25) | (rs2 << 20) | (rs1 << 15) | (funct3 << 12) | (rd << 7) | opcode
}

#[test]
fn every_major_opcode_funct3_and_funct7_is_never_a_simulator_failure() {
    let mut sweep = Sweep::new();
    for major in 0..32u32 {
        let opcode = (major << 2) | 0b11;
        for funct3 in 0..8 {
            for funct7 in 0..128 {
                for (rd, rs1, rs2) in [(0, 0, 0), (1, 2, 0), (5, 6, 1), (31, 0, 2), (0, 7, 31)] {
                    sweep.check(encode(opcode, rd, funct3, rs1, rs2, funct7));
                }
            }
        }
    }
    assert_eq!(sweep.steps, 32 * 8 * 128 * 5 * 3);
    assert!(
        sweep.retired > 0 && sweep.trapped > 0,
        "sweep is not vacuous"
    );
}

#[test]
fn every_csr_address_and_zicsr_funct3_is_never_a_simulator_failure() {
    let mut sweep = Sweep::new();
    for csr in 0..4096u32 {
        for funct3 in [1, 2, 3, 5, 6, 7] {
            // rs1 = x0 is the read-only form of CSRRS/CSRRC; rs1 = 1 writes.
            for rs1 in [0, 1] {
                sweep.check((csr << 20) | (rs1 << 15) | (funct3 << 12) | (5 << 7) | 0x73);
            }
        }
    }
    assert_eq!(sweep.steps, 4096 * 6 * 2 * 3);
    assert!(
        sweep.retired > 0 && sweep.trapped > 0,
        "sweep is not vacuous"
    );
}

#[test]
fn pseudo_random_encodings_are_never_a_simulator_failure() {
    let mut sweep = Sweep::new();
    // xorshift32, fixed seed: deterministic across runs and hosts.
    let mut x: u32 = 0x5eed_1234;
    for _ in 0..100_000 {
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        sweep.check(x);
    }
    assert_eq!(sweep.steps, 100_000 * 3);
    assert!(
        sweep.retired > 0 && sweep.trapped > 0,
        "sweep is not vacuous"
    );
}
