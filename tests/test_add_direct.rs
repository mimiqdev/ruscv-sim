use ruscv_sim::load_and_run;

// Skip this test if the ELF file doesn't exist (e.g., in CI before riscv-tests job)
#[test]
fn test_add_program() {
    let elf_path = "tests/bare-metal-riscv-test/rv64i/add.elf";
    if !std::path::Path::new(elf_path).exists() {
        println!("ELF file not found at {}, skipping test", elf_path);
        return;
    }

    let elf_data = std::fs::read(elf_path).unwrap();
    println!("Loading ELF: {} bytes", elf_data.len());

    // Let the simulator auto-detect tohost address from ELF
    // tohost is at 0x80001000 based on linker script
    let result = load_and_run(&elf_data, Some(1000), None, false);

    match result {
        Ok(r) => {
            println!("Exit Code: {}", r.exit_code);
            println!("Cycles: {}", r.cycles);
            println!("Final PC: 0x{:016x}", r.final_pc);
            if let Some(err) = &r.error {
                println!("Error: {}", err);
            }
            // Expected exit code is 0 (success) from add.S
            // add.S returns 0 if calculation is correct (sum = 55)
            assert_eq!(r.exit_code, 0);
        }
        Err(e) => {
            println!("Error: {}", e);
            panic!("ELF execution failed: {}", e);
        }
    }
}
