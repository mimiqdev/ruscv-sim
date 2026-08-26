use ruscv_sim::load_and_run;
use std::process::Command;

/// Compile the add.S assembly file to ELF if it doesn't exist
fn compile_add_elf() -> std::io::Result<std::path::PathBuf> {
    let elf_path = std::path::PathBuf::from("tests/bare-metal-riscv-test/rv64i/add.elf");

    // Return existing ELF if it already exists
    if elf_path.exists() {
        return Ok(elf_path);
    }

    // Compile add.S to ELF
    let asm_path = elf_path.with_extension("S");
    let obj_path = elf_path.with_extension("o");

    println!("Compiling {} to ELF...", asm_path.display());

    // Use riscv64-unknown-elf toolchain
    let riscv_prefix =
        std::env::var("RISCV_PREFIX").unwrap_or_else(|_| "riscv64-unknown-elf-".to_string());
    let assembler = format!("{}as", riscv_prefix);
    if Command::new(&assembler).arg("--version").output().is_err() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("missing assembler {assembler}"),
        ));
    }

    // Assemble: as -march=rv64ima_zicsr -mabi=lp64 add.S -o add.o
    let as_status = Command::new(&assembler)
        .args(["-march=rv64ima_zicsr", "-mabi=lp64"])
        .arg(&asm_path)
        .arg("-o")
        .arg(&obj_path)
        .status()?;

    if !as_status.success() {
        return Err(std::io::Error::other("Failed to assemble add.S"));
    }

    // Link: ld -Tlinker.ld add.o -o add.elf
    let ld_status = Command::new(format!("{}ld", riscv_prefix))
        .args(["-Ttests/bare-metal-riscv-test/linker.ld"])
        .arg(&obj_path)
        .arg("-o")
        .arg(&elf_path)
        .status()?;

    // Clean up object file
    let _ = std::fs::remove_file(&obj_path);

    if !ld_status.success() {
        return Err(std::io::Error::other("Failed to link add.elf"));
    }

    println!("Successfully compiled {}", elf_path.display());
    Ok(elf_path)
}

#[test]
fn test_add_program() {
    // Compile ELF if needed, otherwise use existing one
    let elf_path = match compile_add_elf() {
        Ok(path) => path,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            eprintln!("skipping test_add_program: {e}");
            return;
        }
        Err(e) => panic!("Failed to compile add.elf: {e}"),
    };

    let elf_data = std::fs::read(&elf_path).unwrap();
    println!(
        "Loading ELF: {} bytes ({}))",
        elf_data.len(),
        elf_path.display()
    );

    // Let the simulator auto-detect tohost address from ELF
    // tohost is at 0x80001000 based on linker script
    let result = load_and_run(&elf_data, Some(1000), None, None, false);

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
