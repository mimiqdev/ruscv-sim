//! RISC-V ISS command-line tool
//!
//! For testing and debugging RISC-V simulator

use clap::{Parser, Subcommand};
use ruscv_sim::machine::{MachineConfig, PlatformKind};
use ruscv_sim::{
    load_and_run_file_with_config, load_and_run_file_with_debug_isa, load_and_run_file_with_isa,
    ExecutionResult, IsaProfile, RemoteBitbangConfig, Transport, ZeroDelayRam,
};
use std::path::PathBuf;
use std::sync::Arc;

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

        /// TCP port for an OpenOCD `remote_bitbang` client
        #[arg(long, value_name = "PORT")]
        rbb_port: Option<u16>,

        /// Halt before the first instruction. Requires `--rbb-port`
        #[arg(long)]
        halted: bool,

        /// Machine ISA profile: `rv64imafdc` (default), `rv64ima`,
        /// `rv64imac` or `rv64imafd`
        #[arg(long, value_name = "PROFILE", default_value = "rv64imafdc")]
        isa: IsaProfileArg,

        /// Guest-RAM transport: `native` (default) or `tlm`. `tlm` uses the
        /// in-tree zero-delay Rust RAM target and requires the native Platform.
        #[arg(long, value_name = "TRANSPORT", default_value = "native")]
        transport: TransportArg,

        /// Let the TLM target grant host pointers for guest-RAM fetches,
        /// loads and stores. Requires `--transport tlm`.
        #[arg(long = "dmi")]
        direct_mem: bool,
    },
}

/// Approved `--transport` names (dev-plan D2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TransportArg {
    Native,
    Tlm,
}

impl std::str::FromStr for TransportArg {
    type Err = String;
    fn from_str(name: &str) -> Result<Self, Self::Err> {
        match name {
            "native" => Ok(Self::Native),
            "tlm" => Ok(Self::Tlm),
            other => Err(format!(
                "invalid transport '{other}': expected native or tlm"
            )),
        }
    }
}

/// Approved `--isa` profile names (contract D3). No ELF inference applies.
#[derive(Debug, Clone, Copy)]
struct IsaProfileArg(IsaProfile);

impl std::str::FromStr for IsaProfileArg {
    type Err = String;
    fn from_str(name: &str) -> Result<Self, Self::Err> {
        IsaProfile::parse(name).map(Self).ok_or_else(|| {
            format!(
                "invalid ISA profile '{name}': expected rv64ima, rv64imac, rv64imafd or rv64imafdc"
            )
        })
    }
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
            rbb_port,
            halted,
            isa,
            transport,
            direct_mem,
        } => {
            if halted && rbb_port.is_none() {
                eprintln!("error: --halted requires --rbb-port");
                std::process::exit(2);
            }
            if direct_mem && transport != TransportArg::Tlm {
                eprintln!("error: --dmi requires --transport tlm");
                std::process::exit(2);
            }

            if verbose {
                eprintln!("Loading ELF file: {:?}", elf);
                eprintln!("Max cycles: {:?}", max_cycles);
                eprintln!("Tohost address: 0x{:016x}", tohost.unwrap_or(0));
                if let Some(ref path) = log_commits {
                    eprintln!("Commit log: {:?}", path);
                }
                eprintln!("Transport: {:?}", transport);
                if direct_mem {
                    eprintln!("Direct memory: enabled");
                }
                if let Some(port) = rbb_port {
                    eprintln!("remote_bitbang port: {port}");
                    if halted {
                        eprintln!("Halting before the first instruction");
                    }
                }
            }

            match run_elf(
                &elf,
                RunOptions {
                    max_cycles,
                    tohost,
                    log_commits,
                    verbose,
                    rbb_port,
                    halted,
                    isa: isa.0,
                    transport,
                    direct_mem,
                },
            ) {
                Ok(result) => {
                    print_result(&result);
                    std::process::exit(process_exit_code(&result));
                }
                Err(e) => {
                    eprintln!("Error: {}", e);
                    std::process::exit(1);
                }
            }
        }
    }
}

/// Optional run knobs beyond the ELF path.
struct RunOptions {
    max_cycles: Option<u64>,
    tohost: Option<u64>,
    log_commits: Option<PathBuf>,
    verbose: bool,
    rbb_port: Option<u16>,
    halted: bool,
    isa: IsaProfile,
    transport: TransportArg,
    direct_mem: bool,
}

fn run_elf(elf_path: &std::path::Path, options: RunOptions) -> Result<ExecutionResult, String> {
    let path = elf_path.to_str().unwrap();
    match options.transport {
        TransportArg::Native => match options.rbb_port {
            Some(port) => load_and_run_file_with_debug_isa(
                path,
                options.max_cycles,
                options.tohost,
                options.log_commits,
                options.verbose,
                RemoteBitbangConfig::listen(port).halted(options.halted),
                options.isa,
            ),
            None => load_and_run_file_with_isa(
                path,
                options.max_cycles,
                options.tohost,
                options.log_commits,
                options.verbose,
                options.isa,
            ),
        },
        TransportArg::Tlm => {
            let mut config = MachineConfig::new(PlatformKind::Native);
            config.isa = options.isa;
            config.verbose = options.verbose;
            config.tohost_override = options.tohost;
            config.uart_output = Some(Arc::new(|byte| print!("{}", byte as char)));
            config.transport = Transport::tlm(Arc::new(ZeroDelayRam::new()));
            config.dmi = options.direct_mem;
            let debug = options
                .rbb_port
                .map(|port| RemoteBitbangConfig::listen(port).halted(options.halted));
            load_and_run_file_with_config(
                path,
                config,
                options.max_cycles,
                options.tohost,
                options.log_commits,
                options.verbose,
                debug,
            )
        }
    }
    .map_err(|e| format!("Execution failed: {}", e))
}

/// Whether the run is reported as `Status:     SUCCESS`.
fn is_success(result: &ExecutionResult) -> bool {
    !result.timed_out && result.exit_code == 0 && result.error.is_none()
}

/// Map a run result to the host process exit status.
///
/// The process status is zero exactly when the run is reported as SUCCESS.
/// A host keeps only the low 8 bits of the status, so a failing 32-bit guest
/// code whose low byte is zero (for example `0x10000400`) would otherwise exit
/// 0. Such codes, and failures that retain a zero guest code (a reporting
/// error), exit 1; any other failing code keeps its low byte. The full guest
/// code is still printed on the `Exit Code:` line.
fn process_exit_code(result: &ExecutionResult) -> i32 {
    if is_success(result) {
        return 0;
    }
    match result.exit_code & 0xff {
        0 => 1,
        low => low as i32,
    }
}

fn print_result(result: &ExecutionResult) {
    println!();
    println!("========== Execution Result ==========");
    println!("Exit Code:  {}", result.exit_code);
    println!("Cycles:     {}", result.cycles);
    println!("Final PC:   0x{:016x}", result.final_pc);

    if result.timed_out {
        println!("Status:     TIMEOUT");
    } else if is_success(result) {
        println!("Status:     SUCCESS");
    } else {
        println!("Status:     FAILED");
    }

    if let Some(ref error) = result.error {
        println!("Error:      {}", error);
    }

    // quantum D10: one line, only when a quantum was selected, so a run with
    // the quantum off keeps byte-identical stdout.
    if let Some(line) = quantum_report_line(&result.quantum) {
        println!("{line}");
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

/// quantum D10: the single `Quantum:` report line, or `None` when the run has
/// no quantum fact so the stdout stays byte-identical.
fn quantum_report_line(quantum: &Option<ruscv_sim::machine::QuantumFacts>) -> Option<String> {
    quantum.as_ref().map(|quantum| {
        format!(
            "Quantum:    {} ticks, {} syncs, latency bound {} ticks{}",
            quantum.ticks,
            quantum.syncs,
            quantum.interrupt_latency_bound_ticks,
            if quantum.reduced_accuracy {
                " (reduced accuracy)"
            } else {
                ""
            }
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn result(exit_code: u32, timed_out: bool, error: Option<&str>) -> ExecutionResult {
        ExecutionResult {
            exit_code,
            timed_out,
            error: error.map(str::to_string),
            ..ExecutionResult::default()
        }
    }

    // quantum D10: the `Quantum:` line exists only when the run has a
    // quantum fact, and it reports the selected facts exactly.
    #[test]
    fn no_quantum_line_is_reported_when_the_fact_is_absent() {
        assert_eq!(quantum_report_line(&None), None);
        assert_eq!(quantum_report_line(&result(0, false, None).quantum), None);
    }

    #[test]
    fn the_quantum_line_reports_the_selected_facts() {
        let facts = ruscv_sim::machine::QuantumFacts {
            ticks: 1024,
            interrupt_latency_bound_ticks: 1041,
            reduced_accuracy: true,
            syncs: 3,
            reached: true,
        };
        let line = quantum_report_line(&Some(facts)).expect("a quantum fact must print");
        assert!(line.starts_with("Quantum:    "), "{line}");
        assert!(line.contains("1024 ticks"), "{line}");
        assert!(line.contains("3 syncs"), "{line}");
        assert!(line.contains("1041 ticks"), "{line}");
        assert!(line.contains("reduced accuracy"), "{line}");
        let precise = ruscv_sim::machine::QuantumFacts {
            ticks: 1,
            interrupt_latency_bound_ticks: 0,
            reduced_accuracy: false,
            syncs: 2,
            reached: true,
        };
        let line = quantum_report_line(&Some(precise)).expect("a quantum fact must print");
        assert!(!line.contains("reduced accuracy"), "{line}");
    }

    #[test]
    fn only_success_exits_zero() {
        assert_eq!(process_exit_code(&result(0, false, None)), 0);
        assert_eq!(process_exit_code(&result(0, false, Some("log"))), 1);
        assert_eq!(process_exit_code(&result(1, true, Some("Timeout"))), 1);
    }

    #[test]
    fn failing_code_with_zero_low_byte_exits_nonzero() {
        for code in [0x100, 0x400, 0x1000_0400, 0xffff_ff00] {
            assert!(!is_success(&result(code, false, None)));
            assert_eq!(
                process_exit_code(&result(code, false, None)),
                1,
                "{code:#x}"
            );
        }
    }

    #[test]
    fn failing_code_keeps_nonzero_low_byte() {
        assert_eq!(process_exit_code(&result(3, false, None)), 3);
        assert_eq!(process_exit_code(&result(0xff, false, None)), 0xff);
        assert_eq!(process_exit_code(&result(0x1234_5605, false, None)), 5);
    }
}
