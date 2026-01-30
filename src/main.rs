//! RISC-V ISS 命令行工具
//!
//! 用于测试和调试RISC-V模拟器

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
    
    println!("测试指令: LUI x1, 0x12345");
    println!("指令编码: 0x{:08x}", lui_instr);
    
    // 重置核心
    core.reset(0x0);
    
    // 执行几条指令
    println!("\n执行测试:");
    let start = Instant::now();
    
    // 模拟执行
    println!("PC = 0x{:08x}", core.state().pc);
    
    // 手动测试解码器
    use ruscv_sim::InstructionDecoder;
    let decoder = InstructionDecoder::new();
    let decoded = decoder.decode(lui_instr_le).unwrap();
    println!("译码结果: {:?}", decoded.opcode);
    println!("目标寄存器: {:?}", decoded.rd);
    println!("立即数: 0x{:08x}", decoded.imm.unwrap());
    
    let elapsed = start.elapsed();
    println!("\n执行时间: {:?}", elapsed);
    println!("\n模拟器初始化完成！");
}
