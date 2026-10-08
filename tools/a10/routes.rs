//! Real public adapters; no internal hooks or alternate execution engine.
use super::*;
use ruscv_sim::executor::RiscVSimulator;
use std::process::Command;

fn child(command: &mut Command) -> Result<std::process::Output, String> {
    command
        .output()
        .map_err(|e| format!("UNAVAILABLE: child invocation: {e}"))
}
pub fn capture_library(
    f: &Fixture,
    route: &str,
    mode: &str,
    elf: &Path,
    probe: &Path,
    log: &Path,
) -> Result<Sample, String> {
    let mut command = Command::new(probe);
    command.arg(route).arg(elf).arg((f.turns + 16).to_string());
    if mode == "file" {
        command.arg(log);
    }
    let output = child(&mut command)?;
    if output.status.code() != Some(0) || !output.stderr.is_empty() {
        return Err(format!(
            "library transport failed: {:?} {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    let stdout = String::from_utf8(output.stdout).map_err(|e| e.to_string())?;
    let (uart, json) = stdout
        .split_once("\nA10_P0_RESULT ")
        .ok_or("UNAVAILABLE: missing library structured result")?;
    let result =
        serde_json::from_str(json.trim()).map_err(|e| format!("invalid library result: {e}"))?;
    let mut sample = Sample::public(route, mode, result);
    sample.uart = Some(uart.as_bytes().to_vec());
    if mode == "file" {
        sample.log = Some(std::fs::read_to_string(log).map_err(|e| e.to_string())?)
    }
    Ok(sample)
}
/// Strict public text parsing. Missing fields/transport errors fail closed.
pub fn parse_cli(
    stdout: &str,
    stderr: &[u8],
    process_code: Option<i32>,
    mode: &str,
    log: Option<String>,
) -> Result<Sample, String> {
    if !stderr.is_empty() {
        return Err(format!("CLI stderr: {}", String::from_utf8_lossy(stderr)));
    }
    let (uart, report) = stdout
        .split_once("\n========== Execution Result ==========\n")
        .ok_or("UNAVAILABLE: missing CLI report")?;
    fn field<'a>(report: &'a str, key: &str) -> Result<&'a str, String> {
        let found: Vec<_> = report
            .lines()
            .filter_map(|line| line.strip_prefix(key))
            .collect();
        if found.len() != 1 {
            return Err(format!("UNAVAILABLE: missing/duplicate CLI {key}"));
        }
        Ok(found[0].trim())
    }
    let status = field(report, "Status:     ")?;
    let error = report
        .lines()
        .find_map(|s| s.strip_prefix("Error:      ").map(str::to_string));
    // Optional means absent, not ambiguous: parse the entire field exactly
    // once. Also reject malformed labels instead of silently treating them as
    // absence (including when a valid Signature line follows them).
    let signatures: Vec<_> = report
        .lines()
        .filter(|line| line.trim_start().starts_with("Signature"))
        .collect();
    let signature = match signatures.as_slice() {
        [] => None,
        [line] => {
            let text = line
                .strip_prefix("Signature:  ")
                .ok_or("invalid CLI Signature field")?;
            let (address, size) = text
                .split_once(" (")
                .ok_or("invalid CLI Signature metadata")?;
            let address = u64::from_str_radix(
                address
                    .strip_prefix("0x")
                    .ok_or("invalid CLI signature address")?,
                16,
            )
            .map_err(|_| "invalid CLI signature address")?;
            let size = size
                .strip_suffix(" bytes)")
                .ok_or("invalid CLI signature size")?
                .parse::<u64>()
                .map_err(|_| "invalid CLI signature size")?;
            if *line != format!("Signature:  0x{address:016x} ({size} bytes)") {
                return Err("invalid CLI Signature format".into());
            }
            Some((address, size))
        }
        _ => return Err("duplicate CLI Signature field".into()),
    };
    let signature_addr = signature.map(|(address, _)| address);
    let result = PublicResult {
        exit_code: field(report, "Exit Code:  ")?
            .parse::<u32>()
            .map_err(|e| e.to_string())?,
        turns: field(report, "Cycles:     ")?
            .parse::<u64>()
            .map_err(|e| e.to_string())?,
        pc: u64::from_str_radix(field(report, "Final PC:   ")?.trim_start_matches("0x"), 16)
            .map_err(|e| e.to_string())?,
        timed_out: status == "TIMEOUT",
        error,
        signature_addr,
        signature: None,
    };
    let expected_status = if result.timed_out {
        "TIMEOUT"
    } else if result.exit_code == 0 && result.error.is_none() {
        "SUCCESS"
    } else {
        "FAILED"
    };
    if status != expected_status || !report.ends_with("=====================================\n") {
        return Err("invalid CLI status/report termination".into());
    }
    let mut sample = Sample::public("cli", mode, result);
    sample.process_code = process_code;
    sample.cli_signature_size = signature.map(|(_, size)| size);
    sample.uart = Some(uart.as_bytes().to_vec());
    sample.log = log;
    Ok(sample)
}
pub fn capture_cli(
    f: &Fixture,
    mode: &str,
    elf: &Path,
    cli: &Path,
    log: &Path,
) -> Result<Sample, String> {
    let mut command = Command::new(cli);
    command
        .arg("run")
        .arg(elf)
        .arg("--max-cycles")
        .arg((f.turns + 16).to_string());
    if mode == "file" {
        command.arg("--log-commits").arg(log);
    }
    let output = child(&mut command)?;
    let stdout = String::from_utf8(output.stdout).map_err(|e| e.to_string())?;
    let file = if mode == "file" {
        Some(std::fs::read_to_string(log).map_err(|e| e.to_string())?)
    } else {
        None
    };
    // Retain the single parsed address/size for the shared validator; searching
    // stdout for an expected line cannot establish unambiguous evidence.
    parse_cli(&stdout, &output.stderr, output.status.code(), mode, file)
}
pub fn capture_flat(f: &Fixture, bytes: &[u8]) -> Result<Sample, String> {
    let mut flat = RiscVSimulator::new(1);
    flat.load_elf(bytes).map_err(|e| e.to_string())?;
    let result = flat.run(Some(f.turns + 16)).map_err(|e| e.to_string())?;
    let mut sample = Sample::public("flat", "off", result.into());
    let offset = f.tohost.ok_or("flat fixture has no selected RAM tohost")? - f.entry;
    sample.signal = Some([
        offset,
        u64::from_le_bytes(
            flat.read_mem(offset, 8)
                .map_err(|e| e.to_string())?
                .try_into()
                .unwrap(),
        ),
    ]);
    sample.regs = Some(flat.state().regs.to_vec());
    sample.minstret = Some(flat.state().csr.read(MINSTRET).map_err(|e| e.to_string())?);
    sample.ram = Some(
        f.ram
            .iter()
            .map(|r| {
                flat.read_mem(r.offset, r.bytes.len()).map(|bytes| Ram {
                    offset: r.offset,
                    bytes,
                })
            })
            .collect::<Result<_, _>>()
            .map_err(|e| e.to_string())?,
    );
    Ok(sample)
}
