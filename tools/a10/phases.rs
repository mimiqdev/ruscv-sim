//! P1 clocks and public-route adapter orchestration. No calibration/comparison.
#[path = "affinity.rs"]
pub mod affinity;
#[path = "digest.rs"]
pub mod digest;
use crate::support::*;
use ruscv_sim::executor::{load_and_run, load_and_run_file, RiscVSimulator};
use ruscv_sim::machine::{Machine, MachineConfig, PlatformEvent, PlatformKind};
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
    AffinityInspect,
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
    fn evidence(&self) -> Option<ClockEvidence> {
        None
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClockEvidence {
    pub id: String,
    pub engine: String,
    pub unit: String,
    pub monotonic: bool,
    pub method: String,
    pub empty_timer_ns: Vec<u64>,
    pub successive_read_ns: Vec<u64>,
    pub observed_resolution_ns: Option<u64>,
    pub advertised_resolution_reason: String,
}
pub struct HostClock {
    origin: Instant,
    evidence: ClockEvidence,
}
impl Default for HostClock {
    fn default() -> Self {
        let origin = Instant::now();
        let id = format!(
            "instant-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("UTC")
                .as_nanos()
        );
        let mut empty = Vec::with_capacity(64);
        let mut reads = Vec::with_capacity(64);
        let mut previous = origin.elapsed().as_nanos() as u64;
        for _ in 0..64 {
            let start = Instant::now();
            empty.push(start.elapsed().as_nanos().try_into().expect("ns"));
            let now = origin.elapsed().as_nanos().try_into().expect("ns");
            reads.push(now - previous);
            previous = now;
        }
        let resolution = reads.iter().chain(&empty).copied().filter(|n| *n > 0).min();
        Self{origin,evidence:ClockEvidence{id,engine:"std::time::Instant".into(),unit:"ns".into(),monotonic:true,method:"64 serial empty Instant-now/elapsed pairs and 64 successive origin reads before guest scope; observed minimum nonzero, not calibrated sufficiency".into(),empty_timer_ns:empty,successive_read_ns:reads,observed_resolution_ns:resolution,advertised_resolution_reason:"std Instant advertises no portable hardware resolution; empirical controls retained".into()}}
    }
}
impl Clock for HostClock {
    fn evidence(&self) -> Option<ClockEvidence> {
        Some(self.evidence.clone())
    }
    fn now_ns(&mut self) -> u64 {
        self.origin
            .elapsed()
            .as_nanos()
            .try_into()
            .expect("host clock exceeds u64 ns")
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
    ops: Vec<Op>,
}
impl<C: Clock> Meter<'_, C> {
    fn event(&mut self, op: Op) {
        self.audit.event(op);
        self.clock.event(op);
        self.ops.push(op);
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
#[serde(deny_unknown_fields)]
pub struct InitialEvidence {
    pub pc: u64,
    pub minstret: u64,
    pub regs: Vec<u64>,
    pub image_sha256: String,
    pub checked_bytes: usize,
    pub reservation_clear: bool,
    pub signal: [u64; 2],
    pub events: Option<Vec<Event>>,
    pub uart: Option<UartState>,
    pub generation: Option<u64>,
    pub previous_generation: Option<u64>,
    pub drain: Option<String>,
    pub stale_isolated: Option<bool>,
    pub limits: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LoadEvidence {
    pub pc: u64,
    pub minstret: u64,
    pub completed_turns: u64,
    pub checked_bytes: usize,
    pub execution_not_started: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
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
    pub scope_error: Option<String>,
    pub sample: Option<Sample>,
    pub load: Option<LoadEvidence>,
    pub argv: Vec<String>,
    pub initial: Option<InitialEvidence>,
    pub ops: Vec<Op>,
    pub native_ops: Option<Vec<Op>>,
    pub clock: Option<ClockEvidence>,
    pub transport_code: Option<i32>,
    pub input_sha256: Option<String>,
    #[serde(default)]
    pub cli_affinity: Option<serde_json::Value>,
}
impl Record {
    pub(crate) fn new(
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
            measurement_status: "unavailable".into(),
            reason: None,
            scope_error: None,
            sample: None,
            load: None,
            argv: Vec::new(),
            initial: None,
            ops: Vec::new(),
            native_ops: None,
            clock: None,
            transport_code: None,
            input_sha256: None,
            cli_affinity: None,
        }
    }
    fn accept_interval(&mut self) {
        self.accepted_ns = if self.scope_error.is_none() {
            self.interval.as_ref().map(|i| i.elapsed_ns)
        } else {
            None
        };
        self.measurement_status = if self.accepted_ns.is_some() {
            "inconclusive-smoke-uncalibrated"
        } else {
            "unavailable-invalid-or-missing-scope"
        }
        .into();
    }
    fn reject(&mut self, reason: String) {
        self.semantic_status = "semantic_failure".into();
        self.measurement_status = "unavailable-rejected-sample".into();
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
            record.accept_interval();
        }
        Err(Rejection::Semantic(e)) => record.reject(e.clone()),
        Err(Rejection::Unavailable(e)) => {
            record.semantic_status = "unavailable".into();
            record.measurement_status = "unavailable-oracle-evidence".into();
            record.reason = Some(e.clone());
            record.accepted_ns = None;
        }
    }
    result
}
/// No aggregate can erase a bad warmup or basic repetition. Raw rows survive.
pub fn accepted_total(records: &[Record]) -> Option<u128> {
    if records.is_empty()
        || records
            .iter()
            .any(|r| r.semantic_status != "correct" || r.accepted_ns.is_none())
    {
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
#[derive(Default)]
struct ImageDigestCache(Option<(Vec<u8>, String)>);
impl ImageDigestCache {
    fn observed_hash(&mut self, actual: &[u8]) -> String {
        // Only cache the pure hash of immutable OWN observed bytes. Every
        // repetition still reads/checks its entire image/BSS and architectural
        // initial state. No prior verdict, live owner or expected state becomes
        // a later sample's observation. Equality establishes identical hashes.
        if let Some((bytes, hash)) = &self.0 {
            if bytes == actual {
                return hash.clone();
            }
        }
        let hash = digest::sha256(actual);
        self.0 = Some((actual.to_vec(), hash.clone()));
        hash
    }
}
fn initial(
    owner: &Machine,
    f: &Fixture,
    cache: &mut ImageDigestCache,
) -> Result<(LoadEvidence, InitialEvidence), String> {
    let i = owner.inspect().map_err(|e| e.to_string())?;
    image_identity(f, &i.image).map_err(|e| format!("{e:?}"))?;
    let minstret = i.hart.csr.read(MINSTRET).map_err(|e| e.to_string())?;
    if i.hart.pc != f.entry
        || i.hart.regs != [0; 32]
        || minstret != 0
        || i.hart.reservation.is_some()
        || !i.events.is_empty()
        || *i.tohost.value.as_ref().map_err(|e| e.to_string())? != 0
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
    let actual = owner
        .read_mem(0, expected.len())
        .map_err(|e| e.to_string())?;
    if actual != expected {
        return Err("initial image/zero-fill bytes mismatch".into());
    }
    let proof = InitialEvidence {
        pc: i.hart.pc,
        minstret,
        regs: i.hart.regs.to_vec(),
        image_sha256: cache.observed_hash(&actual),
        checked_bytes: actual.len(),
        reservation_clear: i.hart.reservation.is_none(),
        signal: [i.tohost.address, *i.tohost.value.as_ref().unwrap()],
        events: Some(
            i.events
                .iter()
                .map(|e| match e {
                    PlatformEvent::UartTransmit(b) => Event("uart".into(), u64::from(*b)),
                    PlatformEvent::HtifWrite(v) => Event("htif".into(), *v),
                })
                .collect(),
        ),
        uart: i.uart.map(|u| UartState {
            base_addr: u.base_addr,
            rx_fifo: u.rx_fifo,
            tx_fifo: u.tx_fifo,
            registers: u.registers,
        }),
        generation: Some(i.generation),
        previous_generation: None,
        drain: None,
        stale_isolated: None,
        limits: vec![],
    };
    Ok((
        LoadEvidence {
            pc: i.hart.pc,
            minstret,
            completed_turns: 0,
            checked_bytes: actual.len(),
            execution_not_started: true,
        },
        proof,
    ))
}
fn initial_flat(
    owner: &RiscVSimulator,
    f: &Fixture,
    bytes: &[u8],
    cache: &mut ImageDigestCache,
) -> Result<InitialEvidence, String> {
    let image = LoadImage::parse(bytes).map_err(|e| e.to_string())?;
    image_identity(f, &image).map_err(|e| format!("{e:?}"))?;
    if owner.state().pc != f.entry
        || owner.state().regs != [0; 32]
        || owner.state().reservation.is_some()
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
    let actual = owner
        .read_mem(0, expected.len())
        .map_err(|e| e.to_string())?;
    if actual != expected {
        return Err("flat initial full image/BSS/selected signal mismatch".into());
    }
    let offset = f.tohost.ok_or("flat RAM signal")? - f.entry;
    Ok(InitialEvidence{pc:owner.state().pc,minstret:owner.state().csr.read(MINSTRET).map_err(|e|e.to_string())?,regs:owner.state().regs.to_vec(),image_sha256:cache.observed_hash(&actual),checked_bytes:actual.len(),reservation_clear:owner.state().reservation.is_none(),signal:[offset,u64::from_le_bytes(owner.read_mem(offset,8).map_err(|e|e.to_string())?.try_into().unwrap())],events:None,uart:None,generation:None,previous_generation:None,drain:None,stale_isolated:None,limits:vec!["public flat facade fresh_reset coordinates drain/restoration internally; no public generation/drain/stale-handle/device introspection; no new API".into()]})
}
fn install<C: Clock>(
    bytes: &[u8],
    config: MachineConfig,
    meter: &mut Meter<C>,
    owner: &mut Option<Machine>,
) -> Result<(), String> {
    meter.event(Op::Parse);
    let image = Arc::new(LoadImage::parse(bytes).map_err(|e| e.to_string())?);
    meter.event(Op::Construct);
    *owner = Some(Machine::new(config));
    meter.event(Op::Install);
    // Even an installation error retains the owner until the caller has
    // stopped load-only timing. Error cleanup cannot hide a drop in that span.
    owner
        .as_ref()
        .unwrap()
        .install(image)
        .map_err(|e| e.to_string())
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
    pub clock: ClockEvidence,
    pub ops: Vec<Op>,
    pub input_sha256: String,
    #[serde(default)]
    pub caller_before: Option<serde_json::Value>,
    #[serde(default)]
    pub caller_after: Option<serde_json::Value>,
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
        ops: Vec::with_capacity(8),
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
    // Identity read before and after file-call scope detects changed input; the
    // public file function still performs its own real timed I/O.
    let before = std::fs::read(elf).map_err(|e| e.to_string())?;
    if bytes.as_ref().is_some_and(|data| data != &before) {
        return Err("native bytes changed during preparation".into());
    }
    let input_sha256 = digest::sha256(bytes.as_deref().unwrap_or(&before));
    let elf_text = elf.to_str().ok_or("native file path must be UTF-8")?;
    meter.event(Op::AffinityInspect);
    let caller_before = affinity::caller();
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
    meter.event(Op::AffinityInspect);
    let caller_after = affinity::caller();
    meter.audit.check()?;
    flush.map_err(|e| e.to_string())?;
    if std::fs::read(elf).map_err(|e| e.to_string())? != before {
        return Err("native input changed during call".into());
    }
    Ok(LibraryWire {
        route: route.into(),
        scope: "public-library-call+uart-flush".into(),
        interval,
        result: result.map_err(|e| e.to_string())?.into(),
        clock: meter.clock.evidence().ok_or("native clock evidence")?,
        ops: meter.ops,
        input_sha256,
        caller_before,
        caller_after,
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
    let caller_before = if library {
        None
    } else {
        meter.event(Op::AffinityInspect);
        affinity::caller()
    };
    let start = if library { None } else { Some(meter.start()) };
    meter.event(Op::ChildLaunch);
    let output = command.output();
    meter.event(Op::ChildWait);
    if let Some(start) = start {
        record.interval = Some(meter.stop(start, "driver-Instant"));
        meter.event(Op::AffinityInspect);
        let caller_after = affinity::caller();
        record.cli_affinity = Some(affinity::cli_proof(
            caller_before,
            caller_after,
            &record.argv,
            paths.cli,
        ));
    }
    let output = output.map_err(|e| e.to_string())?;
    record.transport_code = output.status.code();
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
    record.scope_error = wire.check(&record.route).err();
    record.clock = Some(wire.clock);
    record.native_ops = Some(wire.ops);
    if record.input_sha256.as_ref() != Some(&wire.input_sha256) {
        return Err("native actual input identity mismatch".into());
    }
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
    let mut collector = SmokeCollector {
        records: Vec::new(),
        next: 0,
        repetitions,
    };
    measure_stream(m, f, cell, paths, clock, &mut collector);
    collector.records
}
/// Streaming retention permits bounded-memory calibration without changing any
/// public operation or phase boundary. Every record is still independently P0
/// validated before the controller sees it. Controllers never bless a sample.
pub trait RepetitionController {
    fn next(&mut self) -> Option<(usize, bool)>;
    fn retain(&mut self, record: Record);
    fn failed(&self) -> bool;
    fn cleanup_error(&mut self, reason: String);
}
struct SmokeCollector {
    records: Vec<Record>,
    next: usize,
    repetitions: usize,
}
impl RepetitionController for SmokeCollector {
    fn next(&mut self) -> Option<(usize, bool)> {
        if self.next > self.repetitions {
            return None;
        }
        let rep = self.next;
        self.next += 1;
        Some((rep, rep == 0))
    }
    fn retain(&mut self, record: Record) {
        self.records.push(record);
    }
    fn failed(&self) -> bool {
        self.records
            .iter()
            .any(|r| r.semantic_status != "correct" || r.accepted_ns.is_none())
    }
    fn cleanup_error(&mut self, reason: String) {
        if let Some(record) = self.records.last_mut() {
            record.reject(reason);
        }
    }
}
pub fn measure_stream<C: Clock, R: RepetitionController>(
    m: &Manifest,
    f: &Fixture,
    cell: Cell<'_>,
    paths: &Paths,
    clock: &mut C,
    controller: &mut R,
) {
    let Cell { route, phase, mode } = cell;
    if let Err(reason) = availability(m, f, route, phase, mode) {
        let mut r = Record::new(f, route, phase, mode, 0, false);
        r.semantic_status = "not_applicable".into();
        r.measurement_status = "not_applicable".into();
        r.reason = Some(reason);
        controller.retain(r);
        return;
    }
    let mut meter = Meter {
        clock,
        audit: ScopeAudit::new(phase, route, mode),
        ops: Vec::with_capacity(f.turns as usize * 3 + 64),
    };
    meter.event(Op::ReadInput);
    let input = std::fs::read(paths.fixtures.join(format!("{}.elf", f.id)));
    let mut machine = None;
    let mut flat = None;
    let mut image_cache = ImageDigestCache::default();
    let input_sha256 = input.as_ref().ok().map(|bytes| digest::sha256(bytes));
    let setup = (|| -> Result<(), String> {
        let bytes = input.as_ref().map_err(|e| e.to_string())?;
        if input_sha256.as_deref() != Some(f.elf_sha256.as_str()) {
            return Err("pinned actual input ELF bytes mismatch".into());
        }
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
                let mut retained = None;
                install(bytes, config, &mut meter, &mut retained)?;
                let owner = retained.unwrap();
                meter.event(Op::Inspect);
                initial(&owner, f, &mut image_cache)?;
                machine = Some((owner, uart));
            }
        }
        Ok(())
    })();
    while let Some((rep, warmup)) = controller.next() {
        let mut record = Record::new(f, route, phase, mode, rep, warmup);
        record.input_sha256 = input_sha256.clone();
        record.clock = meter.clock.evidence();
        if setup.is_err() || controller.failed() {
            record.reason = Some(
                setup
                    .as_ref()
                    .err()
                    .cloned()
                    .unwrap_or_else(|| "prior repetition rejected; no retry/reuse".into()),
            );
            if input_sha256
                .as_deref()
                .is_some_and(|hash| hash != f.elf_sha256)
            {
                record.reject("pinned actual input ELF bytes mismatch".into());
            }
            controller.retain(record);
            continue;
        }
        meter.audit = ScopeAudit::new(phase, route, mode);
        meter.ops.clear();
        let log = paths
            .artifacts
            .join(format!("{}-{route}-{phase}-{mode}-{rep}.log", f.id));
        let result = (|| -> Result<(), String> {
            let bytes = input.as_ref().map_err(|e| e.to_string())?;
            if phase == "load_only" {
                meter.event(Op::Prepare);
                let (config, uart) = machine_config(kind(route));
                uart.lock().unwrap().reserve(f.uart.len());
                let mut retained = None;
                let start = meter.start();
                let installed = install(bytes, config, &mut meter, &mut retained);
                record.interval = Some(meter.stop(start, "driver-Instant"));
                if let Err(error) = installed {
                    if let Some(owner) = retained {
                        teardown(owner, &mut meter)
                            .map_err(|cleanup| format!("{error}; cleanup: {cleanup}"))?;
                    }
                    return Err(error);
                }
                let owner = retained.unwrap();
                meter.event(Op::Inspect);
                let checked = initial(&owner, f, &mut image_cache);
                drop(uart);
                let cleanup = teardown(owner, &mut meter);
                let (loaded, proof) = checked?;
                record.load = Some(loaded);
                record.initial = Some(proof);
                cleanup?;
                record.semantic_status = "correct".into();
                record.accepted_ns = record.interval.as_ref().map(|i| i.elapsed_ns);
            } else if phase == "execute_only" && route.starts_with("machine-") {
                let (owner, uart) = machine.as_ref().unwrap();
                meter.event(Op::Drain);
                let previous = owner.status().map_err(|e| e.to_string())?.generation;
                owner.request_quiesce().map_err(|e| e.to_string())?;
                let drained = owner.try_drain().map_err(|e| e.to_string())?;
                let stale = owner.memory().map_err(|e| e.to_string())?;
                meter.event(Op::Reset);
                owner.fresh_reset().map_err(|e| e.to_string())?;
                // Detached old-generation RAM may be edited, but cannot reach
                // the restored image. Drop that view before timing execution.
                stale
                    .lock()
                    .map_err(|e| e.to_string())?
                    .write_byte(0, 0xff)
                    .map_err(|e| e.to_string())?;
                drop(stale);
                uart.lock().unwrap().clear();
                meter.event(Op::Inspect);
                let (_, mut proof) = initial(owner, f, &mut image_cache)?;
                if proof.generation.is_none_or(|g| g <= previous) {
                    return Err("reset generation did not advance".into());
                }
                proof.previous_generation = Some(previous);
                proof.drain = Some(format!("{:?}", drained.lifecycle));
                proof.stale_isolated = Some(true);
                record.initial = Some(proof);
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
                    record.initial = Some(initial_flat(owner, f, bytes, &mut image_cache)?);
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
            let audit_error = meter.audit.check().err();
            if record.scope_error.is_none() {
                record.scope_error = audit_error;
            }
            if phase != "load_only" {
                meter.event(Op::Validate);
                accept_sample(m, f, &mut record, meter.clock).map_err(|e| format!("{e:?}"))?;
            } else {
                record.accept_interval();
            }
            Ok(())
        })();
        if let Err(error) = result {
            if record.reason.is_none() {
                record.reject(error);
            }
        }
        if let Ok(bytes) = input.as_ref() {
            if std::fs::read(paths.fixtures.join(format!("{}.elf", f.id)))
                .as_ref()
                .ok()
                != Some(bytes)
            {
                record.reject("fixture bytes changed during repetition".into());
            }
        }
        meter.event(Op::Report);
        record.ops = meter.ops.clone();
        controller.retain(record);
    }
    if let Some((owner, _)) = machine {
        if let Err(e) = teardown(owner, &mut meter) {
            controller.cleanup_error(e);
        }
    }
    if let Some(owner) = flat {
        meter.event(Op::DropOwner);
        drop(owner);
    }
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

#[cfg(test)]
mod image_cache_tests {
    use super::*;
    #[test]
    fn digest_cache_requires_complete_own_byte_equality() {
        let mut cache = ImageDigestCache::default();
        let original = vec![0; 65_536];
        let hash = cache.observed_hash(&original);
        assert_eq!(hash, digest::sha256(&original));
        assert_eq!(cache.observed_hash(&original.clone()), hash);
        for offset in [0, 32_768, 65_535] {
            let mut changed = original.clone();
            changed[offset] = 1;
            assert_ne!(cache.observed_hash(&changed), hash);
            assert_eq!(cache.observed_hash(&changed), digest::sha256(&changed));
            assert_eq!(cache.observed_hash(&original), hash);
        }
    }
}
