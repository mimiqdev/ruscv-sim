use ruscv_sim::load_and_run;

#[test]
fn test_add_program() {
    let elf_data = std::fs::read("tests/riscv-tests/add.elf").unwrap();
    println!("Loading ELF: {} bytes", elf_data.len());

    // Tohost address is at 0x80002000 (from .tohost section in ELF)
    // This is read from the ELF file's .tohost section
    let tohost_addr = 0x80002000u64;
    let result = load_and_run(&elf_data, Some(1000), Some(tohost_addr), false);

    match result {
        Ok(r) => {
            println!("Exit Code: {}", r.exit_code);
            println!("Cycles: {}", r.cycles);
            println!("Final PC: 0x{:016x}", r.final_pc);
            if let Some(err) = &r.error {
                println!("Error: {}", err);
            }
            // Expected exit code is 0 (success) from add.S
            // add.S now returns 0 if calculation is correct (sum = 55)
            assert_eq!(r.exit_code, 0);
        }
        Err(e) => {
            println!("Error: {}", e);
            panic!("ELF execution failed: {}", e);
        }
    }
}
