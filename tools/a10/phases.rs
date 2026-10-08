//! P1 clocks and public-route adapter orchestration. No calibration/comparison.
use crate::support::*;
use ruscv_sim::executor::{load_and_run, load_and_run_file, RiscVSimulator};
use ruscv_sim::machine::{Machine, MachineConfig, PlatformKind};
use ruscv_sim::{csr::machine::MINSTRET, elf::LoadImage};
use serde::{Deserialize, Serialize};
use std::{
    path::Path,
    process::Command,
    sync::Arc,
    time::{Instant, SystemTime, UNIX_EPOCH},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Op {
    Prepare,
    ReadInput,
    Parse,
    Construct,
    Install,
    Drain,
    Reset,
    Resume,
    Inspect,
    Validate,
    CloseLog,
    FinalCopy,
    DropOwner,
    Step,
    Deliver,
    Consume,
    FlatRun,
    NativeCall,
    UartFlush,
    ChildLaunch,
    ChildWait,
    Start,
    Stop,
    Report,
}
/// Injectable control is harness-owned. Production has no hook or extra engine.
pub trait Clock {
    fn now_ns(&mut self) -> u64;
    fn event(&mut self, _op: Op) {}
    fn sample(&mut self, _sample: &mut Sample, _warmup: bool) {}
}
pub struct HostClock {
    origin: Instant,
}
impl Default for HostClock {
    fn default() -> Self {
        Self {
            origin: Instant::now(),
        }
    }
}
impl Clock for HostClock {
    fn now_ns(&mut self) -> u64 {
        self.origin
            .elapsed()
            .as_nanos()
            .try_into()
            .expect("host clock exceeds u64 ns")
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Interval {
    pub start_ns: u64,
    pub stop_ns: u64,
    pub elapsed_ns: u64,
    pub origin: String,
}
#[derive(Default)]
pub struct ScopeAudit {
    active: bool,
    phase: String,
    route: String,
    mode: String,
    receipt: bool,
    delivered: bool,
    started: bool,
    progress: usize,
    violation: Option<String>,
}
impl ScopeAudit {
    pub fn new(phase: &str, route: &str, mode: &str) -> Self {
        Self {
            phase: phase.into(),
            route: route.into(),
            mode: mode.into(),
            ..Self::default()
        }
    }
    pub fn event(&mut self, op: Op) {
        let allowed = if op == Op::Start {
            !self.active
        } else if op == Op::Stop {
            self.active && !self.receipt
        } else if !self.active {
            true
        } else {
            match self.phase.as_str() {
                "load_only" => matches!(op, Op::Parse | Op::Construct | Op::Install),
                "execute_only" if self.route.starts_with("machine-") => {
                    matches!(op, Op::Step | Op::Consume)
                        || (op == Op::Deliver && self.mode != "off")
                }
                "execute_only" => op == Op::FlatRun,
                "end_to_end" if self.route.starts_with("native-") => {
                    matches!(op, Op::NativeCall | Op::UartFlush)
                }
                "end_to_end" if self.route == "cli" => {
                    matches!(op, Op::ChildLaunch | Op::ChildWait)
                }
                "end_to_end" if self.route == "flat" => matches!(
                    op,
                    Op::Construct | Op::Install | Op::FlatRun | Op::FinalCopy | Op::DropOwner
                ),
                _ => false,
            }
        };
        if self.active
            && !matches!(op, Op::Start | Op::Stop)
            && !(self.phase == "execute_only" && self.route.starts_with("machine-"))
        {
            let sequence: &[Op] = match (self.phase.as_str(), self.route.as_str()) {
                ("load_only", _) => &[Op::Parse, Op::Construct, Op::Install],
                ("execute_only", "flat") => &[Op::FlatRun],
                ("end_to_end", "flat") => &[
                    Op::Construct,
                    Op::Install,
                    Op::FlatRun,
                    Op::FinalCopy,
                    Op::DropOwner,
                ],
                ("end_to_end", "cli") => &[Op::ChildLaunch, Op::ChildWait],
                ("end_to_end", _) => &[Op::NativeCall, Op::UartFlush],
                _ => &[],
            };
            if sequence.get(self.progress) != Some(&op) {
                self.violation = Some("ordered phase scope mismatch".into());
            }
            self.progress += 1;
        }
        if op == Op::Stop {
            let required = match (self.phase.as_str(), self.route.as_str()) {
                ("load_only", _) => 3,
                ("execute_only", _) => 1,
                ("end_to_end", "flat") => 5,
                ("end_to_end", _) => 2,
                _ => usize::MAX,
            };
            if self.progress < required {
                self.violation = Some("required phase operation/destruction omitted".into());
            }
        }
        if !allowed {
            self.violation = Some(format!(
                "scope contamination {}/{}/{}: {op:?}",
                self.route, self.phase, self.mode
            ));
        }
        match op {
            Op::Start => {
                self.active = true;
                self.started = true;
            }
            Op::Stop => self.active = false,
            Op::Step => {
                self.progress += 1;
                if self.receipt {
                    self.violation = Some("unconsumed receipt before next turn".into());
                }
                self.receipt = true;
                self.delivered = self.mode == "off";
            }
            Op::Deliver => self.delivered = true,
            Op::Consume => {
                if !self.receipt || !self.delivered {
                    self.violation =
                        Some("receipt consumed before required synchronous delivery".into());
                }
                self.receipt = false;
            }
            _ => {}
        }
    }
    pub fn check(&self) -> Result<(), String> {
        if self.active || self.receipt || (!self.started && !self.route.starts_with("native-")) {
            Err("incomplete clock/receipt scope".into())
        } else if let Some(v) = &self.violation {
            Err(v.clone())
        } else {
            Ok(())
        }
    }
}
struct Meter<'a, C: Clock> {
    clock: &'a mut C,
    audit: ScopeAudit,
}
impl<C: Clock> Meter<'_, C> {
    fn event(&mut self, op: Op) {
        self.audit.event(op);
        self.clock.event(op);
    }
    fn start(&mut self) -> u64 {
        self.event(Op::Start);
        self.clock.now_ns()
    }
    fn stop(&mut self, start_ns: u64, origin: &str) -> Interval {
        let stop_ns = self.clock.now_ns();
        if stop_ns < start_ns {
            self.audit.violation = Some("nonmonotonic clock".into());
        }
        self.event(Op::Stop);
        Interval {
            start_ns,
            stop_ns,
            elapsed_ns: stop_ns.saturating_sub(start_ns),
            origin: origin.into(),
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoadEvidence {
    pub pc: u64,
    pub minstret: u64,
    pub completed_turns: u64,
    pub checked_bytes: usize,
    pub execution_not_started: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Record {
    pub fixture: String,
    pub route: String,
    pub phase: String,
    pub mode: String,
    pub repetition: usize,
    pub warmup: bool,
    pub utc_unix_ns: u128,
    pub scope: String,
    pub capture_policy: String,
    pub interval: Option<Interval>,
    pub accepted_ns: Option<u64>,
    pub semantic_status: String,
    pub measurement_status: String,
    pub reason: Option<String>,
    pub sample: Option<Sample>,
    pub load: Option<LoadEvidence>,
    pub argv: Vec<String>,
}
impl Record {
    fn new(
        f: &Fixture,
        route: &str,
        phase: &str,
        mode: &str,
        repetition: usize,
        warmup: bool,
    ) -> Self {
        Self {
            fixture: f.id.clone(),
            route: route.into(),
            phase: phase.into(),
            mode: mode.into(),
            repetition,
            warmup,
            utc_unix_ns: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("UTC epoch")
                .as_nanos(),
            scope: scope(route, phase).0.into(),
            capture_policy: scope(route, phase).1.into(),
            interval: None,
            accepted_ns: None,
            semantic_status: "unavailable".into(),
            measurement_status: "inconclusive-smoke-uncalibrated".into(),
            reason: None,
            sample: None,
            load: None,
            argv: Vec::new(),
        }
    }
    fn reject(&mut self, reason: String) {
        self.semantic_status = "semantic_failure".into();
        self.reason = Some(reason);
        self.accepted_ns = None;
    }
}
pub fn scope(route: &str, phase: &str) -> (&'static str, &'static str) {
    match (route,phase) {
        ("flat","end_to_end")=>("continuous flat construction/load_elf/run/owned final-state capture/normal destruction","flat-live-owned-final-copy-before-drop/1"),
        ("flat","execute_only")=>("preloaded flat public run incl public result/artifact handling; direct capture/drop excluded","flat-live-owned-final-copy-after-stop/1"),
        (_,"load_only")=>("pre-read bytes: LoadImage parse through Machine construction/install; retained owner","installed-image-check-after-stop/1"),
        ("machine-native"|"machine-flat","execute_only")=>("preloaded Machine turns and synchronous checked delivery; no setup/inspection/drop","immutable-facts-in-sink-final-state-after-stop/1"),
        ("cli","end_to_end")=>("direct CLI child launch through wait incl output/log/process teardown","own-CLI-report-after-wait/1"),
        ("native-bytes"|"native-file","end_to_end")=>("in-child public library call plus UART stdout flush; child startup/JSON excluded","own-library-result-plus-stdout-after-stop/1"),
        _=>("not applicable","none"),
    }
}
pub fn accept_sample<C: Clock>(
    m: &Manifest,
    f: &Fixture,
    record: &mut Record,
    clock: &mut C,
) -> Check {
    let sample = record
        .sample
        .as_mut()
        .ok_or_else(|| Rejection::Unavailable("missing repetition sample".into()))?;
    clock.sample(sample, record.warmup);
    let result = validate(m, f, sample);
    match &result {
        Ok(()) => {
            record.semantic_status = "correct".into();
            record.accepted_ns = record.interval.as_ref().map(|i| i.elapsed_ns);
        }
        Err(Rejection::Semantic(e)) => record.reject(e.clone()),
        Err(Rejection::Unavailable(e)) => {
            record.semantic_status = "unavailable".into();
            record.reason = Some(e.clone());
            record.accepted_ns = None;
        }
    }
    result
}
/// No aggregate can erase a bad warmup or basic repetition. Raw rows survive.
pub fn accepted_total(records: &[Record]) -> Option<u128> {
    if records.is_empty() || records.iter().any(|r| r.semantic_status != "correct") {
        return None;
    }
    records
        .iter()
        .filter(|r| !r.warmup)
        .try_fold(0u128, |sum, r| sum.checked_add(u128::from(r.accepted_ns?)))
}
pub fn availability(
    m: &Manifest,
    f: &Fixture,
    route: &str,
    phase: &str,
    mode: &str,
) -> Result<(), String> {
    if phase == "load_only" {
        if !route.starts_with("machine-") {
            return Err("N/A: load_only is combined public Machine parse/construction/install, not a facade/CLI subphase".into());
        }
        if mode != "none" {
            return Err("N/A: load_only has no Hart or observation variant".into());
        }
        capability(m, f, route, "off").map_err(|e| format!("{e:?}"))
    } else {
        capability(m, f, route, mode).map_err(|e| format!("{e:?}"))?;
        match (phase,route) {
            ("execute_only","flat"|"machine-native"|"machine-flat")=>Ok(()),
            ("end_to_end","cli"|"native-bytes"|"native-file"|"flat")=>Ok(()),
            ("execute_only",_)=>Err("N/A: convenience ELF APIs have no preload seam; no subtraction".into()),
            ("end_to_end",_)=>Err("N/A: Machine is measured as load_only/execute_only, not attributed to a facade end_to_end".into()),
            _=>Err("unknown phase".into()),
        }
    }
}
fn kind(route: &str) -> PlatformKind {
    if route == "machine-native" {
        PlatformKind::Native
    } else {
        PlatformKind::Flat
    }
}
fn initial(owner: &Machine, f: &Fixture) -> Result<LoadEvidence, String> {
    let i = owner.inspect().map_err(|e| e.to_string())?;
    image_identity(f, &i.image).map_err(|e| format!("{e:?}"))?;
    let minstret = i.hart.csr.read(MINSTRET).map_err(|e| e.to_string())?;
    if i.hart.pc != f.entry
        || i.hart.regs != [0; 32]
        || minstret != 0
        || !i.events.is_empty()
        || i.tohost.value.map_err(|e| e.to_string())? != 0
        || i.uart.as_ref().is_some_and(|u| {
            !u.tx_fifo.is_empty() || !u.rx_fifo.is_empty() || u.registers[4] != 0x60
        })
    {
        return Err("initial PC/GPR/counter/device/signal mismatch".into());
    }
    let mut expected = vec![0; i.image.memory_size()];
    for s in i.image.segments() {
        let offset = (s.guest_address - i.image.base_addr()) as usize;
        expected[offset..offset + s.file_bytes.len()].copy_from_slice(&s.file_bytes);
        expected[offset + s.file_bytes.len()..offset + s.file_bytes.len() + s.zero_fill].fill(0);
    }
    if owner
        .read_mem(0, expected.len())
        .map_err(|e| e.to_string())?
        != expected
    {
        return Err("initial image/zero-fill bytes mismatch".into());
    }
    Ok(LoadEvidence {
        pc: i.hart.pc,
        minstret,
        completed_turns: 0,
        checked_bytes: expected.len(),
        execution_not_started: true,
    })
}
fn initial_flat(owner: &RiscVSimulator, f: &Fixture, bytes: &[u8]) -> Result<(), String> {
    let image = LoadImage::parse(bytes).map_err(|e| e.to_string())?;
    image_identity(f, &image).map_err(|e| format!("{e:?}"))?;
    if owner.state().pc != f.entry
        || owner.state().regs != [0; 32]
        || owner
            .state()
            .csr
            .read(MINSTRET)
            .map_err(|e| e.to_string())?
            != 0
    {
        return Err("flat initial PC/GPR/counter mismatch".into());
    }
    let mut expected = vec![0; image.memory_size()];
    for s in image.segments() {
        let offset = (s.guest_address - image.base_addr()) as usize;
        expected[offset..offset + s.file_bytes.len()].copy_from_slice(&s.file_bytes);
        expected[offset + s.file_bytes.len()..offset + s.file_bytes.len() + s.zero_fill].fill(0);
    }
    if owner
        .read_mem(0, expected.len())
        .map_err(|e| e.to_string())?
        != expected
    {
        return Err("flat initial full image/BSS/selected signal mismatch".into());
    }
    Ok(())
}
fn install<C: Clock>(
    bytes: &[u8],
    config: MachineConfig,
    meter: &mut Meter<C>,
) -> Result<Machine, String> {
    meter.event(Op::Parse);
    let image = Arc::new(LoadImage::parse(bytes).map_err(|e| e.to_string())?);
    meter.event(Op::Construct);
    let owner = Machine::new(config);
    meter.event(Op::Install);
    owner.install(image).map_err(|e| e.to_string())?;
    Ok(owner)
}
fn teardown<C: Clock>(owner: Machine, meter: &mut Meter<C>) -> Result<(), String> {
    meter.event(Op::Drain);
    owner.request_quiesce().map_err(|e| e.to_string())?;
    owner.try_drain().map_err(|e| e.to_string())?;
    meter.event(Op::DropOwner);
    owner.teardown().map_err(|e| e.to_string())?;
    drop(owner);
    Ok(())
}
pub struct Paths<'a> {
    pub fixtures: &'a Path,
    pub cli: &'a Path,
    pub driver: &'a Path,
    pub artifacts: &'a Path,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LibraryWire {
    pub route: String,
    pub scope: String,
    pub interval: Interval,
    pub result: PublicResult,
}
impl LibraryWire {
    pub fn check(&self, route: &str) -> Result<(), String> {
        if self.route != route
            || self.scope != "public-library-call+uart-flush"
            || self.interval.origin != "library-child-Instant"
            || self.interval.stop_ns.checked_sub(self.interval.start_ns)
                != Some(self.interval.elapsed_ns)
        {
            Err("native interval is not public function scope".into())
        } else {
            Ok(())
        }
    }
}
/// Invoked only in the transport child. Preparation/read and JSON/child startup
/// are outside its function-scope clock. Normal API-local teardown is included.
pub fn library_probe(
    route: &str,
    elf: &Path,
    budget: u64,
    log: Option<&Path>,
) -> Result<LibraryWire, String> {
    let mut clock = HostClock::default();
    let mut meter = Meter {
        clock: &mut clock,
        audit: ScopeAudit::new(
            "end_to_end",
            route,
            if log.is_some() { "file" } else { "off" },
        ),
    };
    let bytes = if route == "native-bytes" {
        meter.event(Op::ReadInput);
        Some(std::fs::read(elf).map_err(|e| e.to_string())?)
    } else {
        None
    };
    let elf_text = elf.to_str().ok_or("native file path must be UTF-8")?;
    let start = meter.start();
    meter.event(Op::NativeCall);
    let result = match route {
        "native-bytes" => load_and_run(bytes.as_ref().unwrap(), Some(budget), None, log, false),
        "native-file" => load_and_run_file(
            elf_text,
            Some(budget),
            None,
            log.map(Path::to_path_buf),
            false,
        ),
        _ => return Err("unsupported library route".into()),
    };
    // Library UART uses process stdout. This explicit synchronous flush belongs
    // to the declared API+UART-flush sink policy, never to JSON transport.
    meter.event(Op::UartFlush);
    let flush = std::io::Write::flush(&mut std::io::stdout());
    let interval = meter.stop(start, "library-child-Instant");
    meter.audit.check()?;
    flush.map_err(|e| e.to_string())?;
    Ok(LibraryWire {
        route: route.into(),
        scope: "public-library-call+uart-flush".into(),
        interval,
        result: result.map_err(|e| e.to_string())?.into(),
    })
}
fn capture_child<C: Clock>(
    f: &Fixture,
    record: &mut Record,
    paths: &Paths,
    log: &Path,
    meter: &mut Meter<C>,
) -> Result<Sample, String> {
    let elf = paths.fixtures.join(format!("{}.elf", f.id));
    let library = record.route.starts_with("native-");
    let mut command = Command::new(if library { paths.driver } else { paths.cli });
    if library {
        command
            .arg("probe-library")
            .arg(&record.route)
            .arg(&elf)
            .arg((f.turns + 16).to_string());
    } else {
        command
            .arg("run")
            .arg(&elf)
            .arg("--max-cycles")
            .arg((f.turns + 16).to_string());
    }
    if record.mode == "file" {
        if !library {
            command.arg("--log-commits");
        }
        command.arg(log);
    }
    record.argv = std::iter::once(command.get_program())
        .chain(command.get_args())
        .map(|a| a.to_string_lossy().into_owned())
        .collect();
    let start = if library { None } else { Some(meter.start()) };
    meter.event(Op::ChildLaunch);
    let output = command.output();
    meter.event(Op::ChildWait);
    if let Some(start) = start {
        record.interval = Some(meter.stop(start, "driver-Instant"));
    }
    let output = output.map_err(|e| e.to_string())?;
    // Raw output retention, parsing and log readback are outside all intervals.
    std::fs::write(log.with_extension("stdout"), &output.stdout).map_err(|e| e.to_string())?;
    std::fs::write(log.with_extension("stderr"), &output.stderr).map_err(|e| e.to_string())?;
    let stdout = String::from_utf8(output.stdout).map_err(|e| e.to_string())?;
    let text_log = if record.mode == "file" {
        Some(std::fs::read_to_string(log).map_err(|e| e.to_string())?)
    } else {
        None
    };
    if !library {
        return routes::parse_cli(
            &stdout,
            &output.stderr,
            output.status.code(),
            &record.mode,
            text_log,
        );
    }
    if output.status.code() != Some(0) || !output.stderr.is_empty() {
        return Err("library transport status/stderr failure".into());
    }
    let (uart, json) = stdout
        .split_once("\nA10_P1_NATIVE ")
        .ok_or("missing native function-scope transport")?;
    let wire: LibraryWire = serde_json::from_str(json.trim()).map_err(|e| e.to_string())?;
    wire.check(&record.route)?;
    record.interval = Some(wire.interval);
    let mut sample = Sample::public(&record.route, &record.mode, wire.result);
    sample.uart = Some(uart.as_bytes().to_vec());
    sample.log = text_log;
    Ok(sample)
}
/// All warmup/basic repetitions use identical orchestration and P0 validation.
#[derive(Clone, Copy)]
pub struct Cell<'a> {
    pub route: &'a str,
    pub phase: &'a str,
    pub mode: &'a str,
}
pub fn measure_cell<C: Clock>(
    m: &Manifest,
    f: &Fixture,
    cell: Cell<'_>,
    paths: &Paths,
    repetitions: usize,
    clock: &mut C,
) -> Vec<Record> {
    let Cell { route, phase, mode } = cell;
    if let Err(reason) = availability(m, f, route, phase, mode) {
        let mut r = Record::new(f, route, phase, mode, 0, false);
        r.semantic_status = "not_applicable".into();
        r.reason = Some(reason);
        return vec![r];
    }
    let mut meter = Meter {
        clock,
        audit: ScopeAudit::new(phase, route, mode),
    };
    meter.event(Op::ReadInput);
    let input = std::fs::read(paths.fixtures.join(format!("{}.elf", f.id)));
    let mut machine = None;
    let mut flat = None;
    let setup = (|| -> Result<(), String> {
        let bytes = input.as_ref().map_err(|e| e.to_string())?;
        if phase == "execute_only" {
            if route == "flat" {
                meter.event(Op::Construct);
                let mut owner = RiscVSimulator::new(1);
                meter.event(Op::Install);
                owner.load_elf(bytes).map_err(|e| e.to_string())?;
                flat = Some(owner);
            } else {
                meter.event(Op::Prepare);
                let (config, uart) = machine_config(kind(route));
                uart.lock().unwrap().reserve(f.uart.len());
                let owner = install(bytes, config, &mut meter)?;
                meter.event(Op::Inspect);
                initial(&owner, f)?;
                machine = Some((owner, uart));
            }
        }
        Ok(())
    })();
    let mut records = Vec::new();
    for rep in 0..=repetitions {
        let mut record = Record::new(f, route, phase, mode, rep, rep == 0);
        if setup.is_err()
            || records
                .iter()
                .any(|r: &Record| r.semantic_status != "correct")
        {
            record.reason = Some(
                setup
                    .as_ref()
                    .err()
                    .cloned()
                    .unwrap_or_else(|| "prior repetition rejected; no retry/reuse".into()),
            );
            records.push(record);
            break;
        }
        meter.audit = ScopeAudit::new(phase, route, mode);
        let log = paths
            .artifacts
            .join(format!("{}-{route}-{phase}-{mode}-{rep}.log", f.id));
        let result = (|| -> Result<(), String> {
            let bytes = input.as_ref().map_err(|e| e.to_string())?;
            if phase == "load_only" {
                meter.event(Op::Prepare);
                let (config, uart) = machine_config(kind(route));
                uart.lock().unwrap().reserve(f.uart.len());
                let start = meter.start();
                let owner = install(bytes, config, &mut meter);
                record.interval = Some(meter.stop(start, "driver-Instant"));
                let owner = owner?;
                meter.event(Op::Inspect);
                let checked = initial(&owner, f);
                drop(uart);
                let cleanup = teardown(owner, &mut meter);
                record.load = Some(checked?);
                cleanup?;
                record.semantic_status = "correct".into();
                record.accepted_ns = record.interval.as_ref().map(|i| i.elapsed_ns);
            } else if phase == "execute_only" && route.starts_with("machine-") {
                let (owner, uart) = machine.as_ref().unwrap();
                meter.event(Op::Drain);
                owner.request_quiesce().map_err(|e| e.to_string())?;
                owner.try_drain().map_err(|e| e.to_string())?;
                meter.event(Op::Reset);
                owner.fresh_reset().map_err(|e| e.to_string())?;
                uart.lock().unwrap().clear();
                meter.event(Op::Inspect);
                initial(owner, f)?;
                meter.event(Op::Resume);
                owner.resume().map_err(|e| e.to_string())?;
                meter.event(Op::Prepare);
                let mut run = MachineRun::prepare(f, kind(route), mode, &log)?;
                let start = meter.start();
                let outcome = run.execute(f, owner, |e| {
                    meter.event(match e {
                        TurnEvent::Step => Op::Step,
                        TurnEvent::Deliver => Op::Deliver,
                        TurnEvent::Consume => Op::Consume,
                    })
                });
                record.interval = Some(meter.stop(start, "driver-Instant"));
                meter.event(Op::CloseLog);
                run.close_log();
                outcome?;
                meter.event(Op::Inspect);
                record.sample = Some(run.inspect(f, owner, uart, kind(route), mode, &log)?);
            } else if route == "flat" {
                if phase == "execute_only" {
                    let owner = flat.as_mut().unwrap();
                    meter.event(Op::Reset);
                    owner.fresh_reset().map_err(|e| e.to_string())?;
                    meter.event(Op::Inspect);
                    initial_flat(owner, f, bytes)?;
                    let start = meter.start();
                    meter.event(Op::FlatRun);
                    let outcome = owner.run(Some(f.turns + 16));
                    record.interval = Some(meter.stop(start, "driver-Instant"));
                    meter.event(Op::Inspect);
                    record.sample = Some(Sample::public(
                        "flat",
                        "off",
                        outcome.map_err(|e| e.to_string())?.into(),
                    ));
                    routes::copy_flat_into(f, owner, record.sample.as_mut().unwrap())?;
                } else {
                    let start = meter.start();
                    meter.event(Op::Construct);
                    let mut owner = RiscVSimulator::new(1);
                    let captured = (|| -> Result<(), String> {
                        meter.event(Op::Install);
                        owner.load_elf(bytes).map_err(|e| e.to_string())?;
                        meter.event(Op::FlatRun);
                        let result = owner.run(Some(f.turns + 16)).map_err(|e| e.to_string())?;
                        meter.event(Op::FinalCopy);
                        record.sample = Some(Sample::public("flat", "off", result.into()));
                        routes::copy_flat_into(f, &owner, record.sample.as_mut().unwrap())
                    })();
                    // Sample contains plain owned data only: no facade, memory
                    // Arc, Machine handle or lease can defer actual destruction.
                    meter.event(Op::DropOwner);
                    drop(owner);
                    record.interval = Some(meter.stop(start, "driver-Instant"));
                    captured?;
                }
            } else {
                record.sample = Some(capture_child(f, &mut record, paths, &log, &mut meter)?);
            }
            meter.audit.check()?;
            if phase != "load_only" {
                meter.event(Op::Validate);
                accept_sample(m, f, &mut record, meter.clock).map_err(|e| format!("{e:?}"))?;
            }
            Ok(())
        })();
        if let Err(error) = result {
            if record.reason.is_none() {
                record.reject(error);
            }
        }
        meter.event(Op::Report);
        records.push(record);
    }
    if let Some((owner, _)) = machine {
        if let Err(e) = teardown(owner, &mut meter) {
            if let Some(r) = records.last_mut() {
                r.reject(e);
            }
        }
    }
    if let Some(owner) = flat {
        meter.event(Op::DropOwner);
        drop(owner);
    }
    records
}
pub fn matrix<C: Clock>(
    m: &Manifest,
    paths: &Paths,
    repetitions: usize,
    clock: &mut C,
) -> Vec<Record> {
    let mut records = Vec::new();
    for f in &m.fixtures {
        for route in m.route_matrix.keys() {
            for phase in ["load_only", "execute_only", "end_to_end"] {
                for mode in if phase == "load_only" {
                    vec!["none"]
                } else {
                    vec!["off", "facts", "file"]
                } {
                    records.extend(measure_cell(
                        m,
                        f,
                        Cell { route, phase, mode },
                        paths,
                        repetitions,
                        clock,
                    ));
                }
            }
        }
    }
    records
}
