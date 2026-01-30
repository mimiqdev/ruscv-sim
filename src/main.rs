//! RISC-V ISS command-line tool
//!
//! For testing and debugging RISC-V simulator

use ruscv_sim::{RiscvCore, SimpleMemory};
use std::sync::{Arc, Mutex};
use std::time::Instant;

fn main() {
    println!("RISC-V ISS Simulator v0.1.0");
    println!("============================");

    // 创建存储器 (64KB)
    let mem_size = 0x10000;
    let mem = Arc::new(Mutex::new(SimpleMemory::new(mem_size)));

    // 创建核心
    let mut core = RiscvCore::new(mem.clone(), mem);

    // 加载简单测试程序 (LUI x1, 0x12345)
    // LUI x1, 0x12345 -> 0x12345000 | (1 << 7) | 0b011_0111
    let lui_instr: u32 = (0x12345u32 << 12) | (1u32 << 7) | 0b011_0111u32;
    let lui_instr_le = lui_instr.to_le();

    println!("Test instruction: LUI x1, 0x12345");
    println!("Instruction encoding: 0x{:08x}", lui_instr);

    // 重置核心
    core.reset(0x0);

    // 执行几条指令
    println!("\nExecute test:");
    let start = Instant::now();

    // 模拟执行
    println!("PC = 0x{:08x}", core.state().pc);

    // 手动测试解码器
    use ruscv_sim::InstructionDecoder;
    let decoder = InstructionDecoder::new();
    let decoded = decoder.decode(lui_instr_le).unwrap();
    println!("译码结果: {:?}", decoded.opcode);
    println!("目标寄存器: {:?}", decoded.rd);
    println!("Immediate: 0x{:08x}", decoded.imm.unwrap());

    let elapsed = start.elapsed();
    println!("\n执行时间: {:?}", elapsed);
    println!("\nSimulator initialization complete!");
}
