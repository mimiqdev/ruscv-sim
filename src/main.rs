//! RISC-V ISS 命令行工具
//!
//! 用于测试和调试RISC-V模拟器

use ruscv_sim::{RiscvCore, SimpleMemory};
use std::sync::Arc;
use std::time::Instant;

fn main() {
    println!("RISC-V ISS Simulator v0.1.0");
    println!("============================");
    
    // 创建存储器
    let mem_size = 0x10000; // 64KB
    let instruction_mem = Arc::new(SimpleMemory::new(mem_size));
    let data_mem = instruction_mem.clone();
    
    // 创建核心
    let mut core = RiscvCore::new(instruction_mem, data_mem);
    
    // 加载简单测试程序 (LUI x1, 0x12345)
    // LUI x1, 0x12345 -> 0x12345000 | (1 << 7) | 0b011_0111
    let lui_instr = (0x12345 << 12) | (1 << 7) | 0b011_0111;
    let lui_instr_le = lui_instr.to_le();
    
    // 获取存储器可变引用
    // 注意：这里简化处理，实际应该使用更好的接口
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
