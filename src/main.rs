//! RISC-V ISS command-line tool
//!
//! For testing and debugging RISC-V simulator

use clap::{Parser, Subcommand};
use ruscv_sim::{load_and_run_file, ExecutionResult};
use std::path::PathBuf;

/// Parse address string that may be in decimal or hexadecimal format
fn parse_addr(s: &str) -> Result<u64, String> {
    if s.starts_with("0x") || s.starts_with("0X") {
        u64::from_str_radix(&s[2..], 16).map_err(|e| format!("Invalid hexadecimal address: {}", e))
    } else {
        s.parse::<u64>()
            .map_err(|e| format!("Invalid decimal address: {}", e))
    }
}

/// RISC-V ISS Simulator CLI
#[derive(Parser, Debug)]
#[command(name = "ruscv-sim")]
#[command(author, version, about, long_about = None)]
struct Args {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Run a RISC-V ELF program
    Run {
        /// Path to the ELF file to execute
        #[arg(value_name = "ELF_FILE")]
        elf: PathBuf,

        /// Maximum number of cycles to execute
        #[arg(short, long, value_name = "CYCLES")]
        max_cycles: Option<u64>,

        /// Tohost address for exit detection (virtual address, e.g., 0x40008000)
        #[arg(short, long, value_name = "ADDR", value_parser = parse_addr)]
        tohost: Option<u64>,

        /// Show verbose output
        #[arg(short, long)]
        verbose: bool,

        /// Output commit log to file (Spike-compatible format)
        #[arg(long, value_name = "FILE")]
        log_commits: Option<PathBuf>,
    },
}

fn main() {
    let args = Args::parse();

    match args.command {
        Commands::Run {
            elf,
            max_cycles,
            tohost,
            verbose,
            log_commits,
        } => {
            if verbose {
                eprintln!("Loading ELF file: {:?}", elf);
                eprintln!("Max cycles: {:?}", max_cycles);
                eprintln!("Tohost address: 0x{:016x}", tohost.unwrap_or(0));
                if let Some(ref path) = log_commits {
                    eprintln!("Commit log: {:?}", path);
                }
            }

            match run_elf(&elf, max_cycles, tohost, log_commits, verbose) {
                Ok(result) => {
                    print_result(&result);
                    std::process::exit(result.exit_code as i32);
                }
                Err(e) => {
                    eprintln!("Error: {}", e);
                    std::process::exit(1);
                }
            }
        }
    }
}

fn run_elf(
    elf_path: &std::path::Path,
    max_cycles: Option<u64>,
    tohost: Option<u64>,
    log_commits: Option<PathBuf>,
    verbose: bool,
) -> Result<ExecutionResult, String> {
    load_and_run_file(
        elf_path.to_str().unwrap(),
        max_cycles,
        tohost,
        log_commits,
        verbose,
    )
    .map_err(|e| format!("Execution failed: {}", e))
}

fn print_result(result: &ExecutionResult) {
    println!();
    println!("========== Execution Result ==========");
    println!("Exit Code:  {}", result.exit_code);
    println!("Cycles:     {}", result.cycles);
    println!("Final PC:   0x{:016x}", result.final_pc);

    if result.timed_out {
        println!("Status:     TIMEOUT");
    } else if result.exit_code == 0 {
        println!("Status:     SUCCESS");
    } else {
        println!("Status:     FAILED");
    }

    if let Some(ref error) = result.error {
        println!("Error:      {}", error);
    }

    if let Some(addr) = result.signature_addr {
        println!(
            "Signature:  0x{:016x} ({} bytes)",
            addr,
            result.signature_data.as_ref().map(|d| d.len()).unwrap_or(0)
        );
    }

    println!("=====================================");
}
