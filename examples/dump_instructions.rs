use ruscv_sim::InstructionDecoder;

fn main() {
    let elf_data = std::fs::read("tests/bare-metal-riscv-test/rv64i/add.elf").unwrap();
    let decoder = InstructionDecoder::new();

    println!("ELF size: {} bytes", elf_data.len());

    // Skip ELF header (64 bytes)
    let program_start = 64;

    // Dump instructions at virtual address 0x80000000
    let base_addr = 0x80000000;

    for i in 0..20 {
        let offset = program_start + i * 4;
        if offset + 4 > elf_data.len() {
            break;
        }

        let mut instr_bytes = [0u8; 4];
        instr_bytes.copy_from_slice(&elf_data[offset..offset + 4]);
        let instr = u32::from_le_bytes(instr_bytes);

        let va = base_addr + i * 4;
        match decoder.decode(instr) {
            Ok(decoded) => {
                println!(
                    "{:#010x}: {:#010x} opcode={:?} funct3={:?} rd={:?} rs1={:?} rs2={:?} imm={:?}",
                    va,
                    instr,
                    decoded.opcode,
                    decoded.funct3,
                    decoded.rd,
                    decoded.rs1,
                    decoded.rs2,
                    decoded.imm
                );
            }
            Err(e) => {
                println!("{:#010x}: {:#010x} DECODE ERROR: {}", va, instr, e);
            }
        }
    }
}
