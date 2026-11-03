//! F/D U3a: replay the pinned U2 reference rows through the public Hart.
//!
//! `tools/fpu/fpu-reference-v1.tsv` is the committed Berkeley SoftFloat 3e
//! reference set (contract U2). `tests/fpu_arith.rs` replays it against the
//! arithmetic core directly. This test replays the same rows through encoded
//! F/D instruction words and the Hart step path, so a dispatch or operand
//! wiring defect in `src/isa/fd` fails even though the core itself is
//! correct (contract U3a: "execute every F/D instruction on the public path
//! through the U2 core").
//!
//! Each row is `op rm a b c result flags`. Fused multiply-add rows are
//! `FMADD`; the other three fused forms are executed with the corresponding
//! sign flips and must produce the same row result and flags.

use ruscv_sim::core::{RiscvCore, StepOutcome};
use ruscv_sim::csr::{machine, mstatus};
use ruscv_sim::fpu::NAN_BOX_MASK;
use ruscv_sim::memory::{MemoryInterface, SimpleMemory};
use ruscv_sim::physical::{NativeRamBackend, ValidatedPhysicalAccess};
use ruscv_sim::IsaProfile;
use std::sync::{Arc, Mutex};

const VECTOR_FILE: &str = "tools/fpu/fpu-reference-v1.tsv";
const RAM_SIZE: usize = 0x1000;
const SIGN32: u32 = 0x8000_0000;
const SIGN64: u64 = 0x8000_0000_0000_0000;

// ============== Instruction encoders ==============
//
// FPR operands live in f1..f3 and the destination in f5; integer sources live
// in x1 and integer destinations in x4.

fn op_fp(funct7: u32, rs2: u32, rm: u32, rd: u32) -> u32 {
    (funct7 << 25) | (rs2 << 20) | (1 << 15) | (rm << 12) | (rd << 7) | 0b101_0011
}

fn compare(funct7: u32, selector: u32) -> u32 {
    (funct7 << 25) | (2 << 20) | (1 << 15) | (selector << 12) | (4 << 7) | 0b101_0011
}

fn r4(opcode: u32, fmt: u32, rm: u32) -> u32 {
    (3 << 27) | (fmt << 25) | (2 << 20) | (1 << 15) | (rm << 12) | (5 << 7) | opcode
}

fn rounding(name: &str) -> u32 {
    match name {
        "RNE" => 0,
        "RTZ" => 1,
        "RDN" => 2,
        "RUP" => 3,
        "RMM" => 4,
        // Compare rows carry no rounding mode; their rm bits are ignored.
        "-" => 0,
        other => panic!("unknown rounding mode {other}"),
    }
}

// ============== Reference rows ==============

struct Row<'a> {
    op: &'a str,
    rm: &'a str,
    a: u64,
    b: u64,
    c: Option<u64>,
    result: u64,
    flags: u8,
}

fn hex(value: &str) -> u64 {
    u64::from_str_radix(value, 16).unwrap_or_else(|e| panic!("hex operand {value:?}: {e}"))
}

impl<'a> Row<'a> {
    fn parse(line: &'a str) -> Self {
        let fields: Vec<&str> = line.split('\t').collect();
        assert_eq!(fields.len(), 7, "malformed vector row: {line}");
        Row {
            op: fields[0],
            rm: fields[1],
            a: hex(fields[2]),
            b: if fields[3] == "-" { 0 } else { hex(fields[3]) },
            c: if fields[4] == "-" {
                None
            } else {
                Some(hex(fields[4]))
            },
            result: hex(fields[5]),
            flags: u8::from_str_radix(fields[6], 16).expect("flag field"),
        }
    }
}

fn rows() -> String {
    let path = std::env::var("FPU_VECTOR_FILE")
        .unwrap_or_else(|_| format!("{}/{}", env!("CARGO_MANIFEST_DIR"), VECTOR_FILE));
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {path}: {e}"))
}

// ============== Hart fixture and checks ==============

#[derive(Clone, Copy)]
enum Operands {
    Single(u32, u32, Option<u32>),
    Double(u64, u64, Option<u64>),
    Integer(u64),
}

#[derive(Clone, Copy)]
enum Destination {
    /// The destination is an FPR; the result is compared with the full
    /// NaN-boxed register value (singles) or the raw double bits.
    Fpr { single: bool },
    /// The destination is a GPR.
    Gpr,
}

struct Hart {
    core: RiscvCore,
    ram: Arc<Mutex<SimpleMemory>>,
}

impl Hart {
    fn new() -> Self {
        let ram = Arc::new(Mutex::new(SimpleMemory::new(RAM_SIZE)));
        let typed: Arc<Mutex<dyn MemoryInterface + Send + Sync>> = ram.clone();
        let fetch = Arc::new(Mutex::new(ValidatedPhysicalAccess::new(
            NativeRamBackend::new(ram.clone(), 0, RAM_SIZE),
        )));
        let data = Arc::new(Mutex::new(ValidatedPhysicalAccess::new(
            NativeRamBackend::new(ram.clone(), 0, RAM_SIZE),
        )));
        let mut core = RiscvCore::new_with_physical_ports(typed.clone(), typed, fetch, data);
        core.set_isa_profile(IsaProfile::Rv64imafd);
        core.reset(0, 0);
        core.state_mut()
            .csr
            .write(machine::MSTATUS, mstatus::FS_DIRTY)
            .expect("mstatus is writable");
        Self { core, ram }
    }

    fn seed(&mut self, operands: &Operands) {
        let state = self.core.state_mut();
        match *operands {
            Operands::Single(a, b, c) => {
                state.fpr.write_u32(1, a);
                state.fpr.write_u32(2, b);
                if let Some(c) = c {
                    state.fpr.write_u32(3, c);
                }
            }
            Operands::Double(a, b, c) => {
                state.fpr.write(1, ruscv_sim::Fpr::from_bits(a));
                state.fpr.write(2, ruscv_sim::Fpr::from_bits(b));
                if let Some(c) = c {
                    state.fpr.write(3, ruscv_sim::Fpr::from_bits(c));
                }
            }
            Operands::Integer(a) => state.regs[1] = a,
        }
    }

    fn check(
        &mut self,
        raw: u32,
        operands: Operands,
        destination: Destination,
        expected: u64,
        expected_flags: u8,
        context: &str,
    ) {
        self.seed(&operands);
        self.core.state_mut().fcsr.clear_flags();
        self.ram.lock().unwrap().write_word(0, raw).unwrap();
        self.core.state_mut().pc = 0;
        match self.core.step_outcome() {
            StepOutcome::InstructionRetired(_) => {}
            other => panic!("{context} ({raw:#010x}) did not retire: {other:?}"),
        }
        match destination {
            Destination::Fpr { single: true } => {
                let got = self.core.state().fpr.read(5).bits();
                assert_eq!(got, NAN_BOX_MASK | expected, "{context} ({raw:#010x})");
            }
            Destination::Fpr { single: false } => {
                assert_eq!(
                    self.core.state().fpr.read(5).bits(),
                    expected,
                    "{context} ({raw:#010x})"
                );
            }
            Destination::Gpr => {
                assert_eq!(
                    self.core.state().regs[4],
                    expected,
                    "{context} ({raw:#010x})"
                );
            }
        }
        assert_eq!(
            self.core.state().fcsr.flags_bits(),
            expected_flags,
            "{context} ({raw:#010x}) flags"
        );
    }
}

// ============== Replay ==============

fn replay(hart: &mut Hart, row: &Row<'_>) {
    let rm = rounding(row.rm);
    let (a32, b32) = (row.a as u32, row.b as u32);
    let c32 = row.c.map(|c| c as u32);
    let context = format!("{} rm={}", row.op, row.rm);
    let single = Destination::Fpr { single: true };
    let double = Destination::Fpr { single: false };

    match row.op {
        "f32_add" => hart.check(
            op_fp(0x00, 2, rm, 5),
            Operands::Single(a32, b32, None),
            single,
            row.result,
            row.flags,
            &context,
        ),
        "f32_sub" => hart.check(
            op_fp(0x04, 2, rm, 5),
            Operands::Single(a32, b32, None),
            single,
            row.result,
            row.flags,
            &context,
        ),
        "f32_mul" => hart.check(
            op_fp(0x08, 2, rm, 5),
            Operands::Single(a32, b32, None),
            single,
            row.result,
            row.flags,
            &context,
        ),
        "f32_div" => hart.check(
            op_fp(0x0c, 2, rm, 5),
            Operands::Single(a32, b32, None),
            single,
            row.result,
            row.flags,
            &context,
        ),
        "f32_sqrt" => hart.check(
            op_fp(0x2c, 0, rm, 5),
            Operands::Single(a32, 0, None),
            single,
            row.result,
            row.flags,
            &context,
        ),
        "f32_fmadd" => {
            let c = c32.expect("FMA addend");
            // FMADD(a, b, c), FMSUB(a, b, -c), FNMSUB(-a, b, c),
            // FNMADD(-a, b, -c) are the same fused operation.
            for (opcode, operands) in [
                (0b100_0011, Operands::Single(a32, b32, Some(c))),
                (0b100_0111, Operands::Single(a32, b32, Some(c ^ SIGN32))),
                (0b100_1011, Operands::Single(a32 ^ SIGN32, b32, Some(c))),
                (
                    0b100_1111,
                    Operands::Single(a32 ^ SIGN32, b32, Some(c ^ SIGN32)),
                ),
            ] {
                hart.check(
                    r4(opcode, 0, rm),
                    operands,
                    single,
                    row.result,
                    row.flags,
                    &context,
                );
            }
        }
        "f64_add" => hart.check(
            op_fp(0x01, 2, rm, 5),
            Operands::Double(row.a, row.b, None),
            double,
            row.result,
            row.flags,
            &context,
        ),
        "f64_sub" => hart.check(
            op_fp(0x05, 2, rm, 5),
            Operands::Double(row.a, row.b, None),
            double,
            row.result,
            row.flags,
            &context,
        ),
        "f64_mul" => hart.check(
            op_fp(0x09, 2, rm, 5),
            Operands::Double(row.a, row.b, None),
            double,
            row.result,
            row.flags,
            &context,
        ),
        "f64_div" => hart.check(
            op_fp(0x0d, 2, rm, 5),
            Operands::Double(row.a, row.b, None),
            double,
            row.result,
            row.flags,
            &context,
        ),
        "f64_sqrt" => hart.check(
            op_fp(0x2d, 0, rm, 5),
            Operands::Double(row.a, 0, None),
            double,
            row.result,
            row.flags,
            &context,
        ),
        "f64_fmadd" => {
            let c = row.c.expect("FMA addend");
            for (opcode, operands) in [
                (0b100_0011, Operands::Double(row.a, row.b, Some(c))),
                (0b100_0111, Operands::Double(row.a, row.b, Some(c ^ SIGN64))),
                (0b100_1011, Operands::Double(row.a ^ SIGN64, row.b, Some(c))),
                (
                    0b100_1111,
                    Operands::Double(row.a ^ SIGN64, row.b, Some(c ^ SIGN64)),
                ),
            ] {
                hart.check(
                    r4(opcode, 1, rm),
                    operands,
                    double,
                    row.result,
                    row.flags,
                    &context,
                );
            }
        }
        "f32_to_i32" => hart.check(
            op_fp(0x60, 0, rm, 4),
            Operands::Single(a32, 0, None),
            Destination::Gpr,
            row.result,
            row.flags,
            &context,
        ),
        "f32_to_ui32" => hart.check(
            op_fp(0x60, 1, rm, 4),
            Operands::Single(a32, 0, None),
            Destination::Gpr,
            row.result,
            row.flags,
            &context,
        ),
        "f32_to_i64" => hart.check(
            op_fp(0x60, 2, rm, 4),
            Operands::Single(a32, 0, None),
            Destination::Gpr,
            row.result,
            row.flags,
            &context,
        ),
        "f32_to_ui64" => hart.check(
            op_fp(0x60, 3, rm, 4),
            Operands::Single(a32, 0, None),
            Destination::Gpr,
            row.result,
            row.flags,
            &context,
        ),
        "f64_to_i32" => hart.check(
            op_fp(0x61, 0, rm, 4),
            Operands::Double(row.a, 0, None),
            Destination::Gpr,
            row.result,
            row.flags,
            &context,
        ),
        "f64_to_ui32" => hart.check(
            op_fp(0x61, 1, rm, 4),
            Operands::Double(row.a, 0, None),
            Destination::Gpr,
            row.result,
            row.flags,
            &context,
        ),
        "f64_to_i64" => hart.check(
            op_fp(0x61, 2, rm, 4),
            Operands::Double(row.a, 0, None),
            Destination::Gpr,
            row.result,
            row.flags,
            &context,
        ),
        "f64_to_ui64" => hart.check(
            op_fp(0x61, 3, rm, 4),
            Operands::Double(row.a, 0, None),
            Destination::Gpr,
            row.result,
            row.flags,
            &context,
        ),
        "i32_to_f32" => hart.check(
            op_fp(0x68, 0, rm, 5),
            Operands::Integer(row.a),
            single,
            row.result,
            row.flags,
            &context,
        ),
        "ui32_to_f32" => hart.check(
            op_fp(0x68, 1, rm, 5),
            Operands::Integer(row.a),
            single,
            row.result,
            row.flags,
            &context,
        ),
        "i64_to_f32" => hart.check(
            op_fp(0x68, 2, rm, 5),
            Operands::Integer(row.a),
            single,
            row.result,
            row.flags,
            &context,
        ),
        "ui64_to_f32" => hart.check(
            op_fp(0x68, 3, rm, 5),
            Operands::Integer(row.a),
            single,
            row.result,
            row.flags,
            &context,
        ),
        "i32_to_f64" => hart.check(
            op_fp(0x69, 0, rm, 5),
            Operands::Integer(row.a),
            double,
            row.result,
            row.flags,
            &context,
        ),
        "ui32_to_f64" => hart.check(
            op_fp(0x69, 1, rm, 5),
            Operands::Integer(row.a),
            double,
            row.result,
            row.flags,
            &context,
        ),
        "i64_to_f64" => hart.check(
            op_fp(0x69, 2, rm, 5),
            Operands::Integer(row.a),
            double,
            row.result,
            row.flags,
            &context,
        ),
        "ui64_to_f64" => hart.check(
            op_fp(0x69, 3, rm, 5),
            Operands::Integer(row.a),
            double,
            row.result,
            row.flags,
            &context,
        ),
        "f32_to_f64" => hart.check(
            op_fp(0x21, 0, rm, 5),
            Operands::Single(a32, 0, None),
            double,
            row.result,
            row.flags,
            &context,
        ),
        "f64_to_f32" => hart.check(
            op_fp(0x20, 1, rm, 5),
            Operands::Double(row.a, 0, None),
            single,
            row.result,
            row.flags,
            &context,
        ),
        "f32_eq" => hart.check(
            compare(0x50, 2),
            Operands::Single(a32, b32, None),
            Destination::Gpr,
            row.result,
            row.flags,
            &context,
        ),
        "f32_lt" => hart.check(
            compare(0x50, 1),
            Operands::Single(a32, b32, None),
            Destination::Gpr,
            row.result,
            row.flags,
            &context,
        ),
        "f32_le" => hart.check(
            compare(0x50, 0),
            Operands::Single(a32, b32, None),
            Destination::Gpr,
            row.result,
            row.flags,
            &context,
        ),
        "f64_eq" => hart.check(
            compare(0x51, 2),
            Operands::Double(row.a, row.b, None),
            Destination::Gpr,
            row.result,
            row.flags,
            &context,
        ),
        "f64_lt" => hart.check(
            compare(0x51, 1),
            Operands::Double(row.a, row.b, None),
            Destination::Gpr,
            row.result,
            row.flags,
            &context,
        ),
        "f64_le" => hart.check(
            compare(0x51, 0),
            Operands::Double(row.a, row.b, None),
            Destination::Gpr,
            row.result,
            row.flags,
            &context,
        ),
        other => panic!("unknown reference operation {other}"),
    }
}

#[test]
fn reference_rows_replay_bit_exactly_through_the_hart() {
    let text = rows();
    let mut hart = Hart::new();
    let mut rows_seen = 0usize;
    for line in text.lines() {
        if line.trim().is_empty() {
            continue;
        }
        rows_seen += 1;
        let row = Row::parse(line);
        replay(&mut hart, &row);
    }
    assert_eq!(
        rows_seen,
        text.lines().filter(|line| !line.trim().is_empty()).count(),
        "vector file contains unprocessed rows"
    );
    assert!(
        rows_seen > 10_000,
        "reference vector file looks truncated: {rows_seen} rows"
    );
}
