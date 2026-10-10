//! P0 library-output transport; not a measurement driver or semantics engine.
use ruscv_sim::executor::{load_and_run, load_and_run_file};
use std::path::Path;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() != 4 && args.len() != 5 {
        return Err("usage: a10-p0-probe native-bytes|native-file ELF BUDGET [LOG]".into());
    }
    let log = args.get(4).map(Path::new);
    let budget = Some(args[3].parse()?);
    let r = match args[1].as_str() {
        "native-bytes" => load_and_run(&std::fs::read(&args[2])?, budget, None, log, false)?,
        "native-file" => {
            load_and_run_file(&args[2], budget, None, log.map(Path::to_path_buf), false)?
        }
        _ => return Err("unknown library route".into()),
    };
    // UART output precedes the delimiter. No library stdout is suppressed or
    // replaced by an internal/test callback. Transport success is not guest 0.
    println!(
        "\nA10_P0_RESULT {}",
        serde_json::json!({
            "exit_code":r.exit_code,"turns":r.cycles,"pc":r.final_pc,"timed_out":r.timed_out,
            "error":r.error,"signature_addr":r.signature_addr,"signature":r.signature_data,
        })
    );
    Ok(())
}
