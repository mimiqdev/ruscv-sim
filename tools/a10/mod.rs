//! Reusable P0 correctness support. No clock, phases, calibration or timing verdict.
//! P1 must pass every independently captured repetition to `validate`.
use ruscv_sim::core::commits::CommitLogger;
use ruscv_sim::core::observation::{Observation, ObservationSink};
use ruscv_sim::csr::machine::MINSTRET;
use ruscv_sim::elf::LoadImage;
use ruscv_sim::executor::ExecutionResult;
use ruscv_sim::machine::{Machine, MachineConfig, PlatformEvent, PlatformKind};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::{Arc, Mutex};
pub mod routes;

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub schema: String,
    pub version: u32,
    pub image: String,
    pub source_baseline: String,
    pub build_flags: Vec<String>,
    pub tool_versions: BTreeMap<String, String>,
    pub arm64_tool_sha256: BTreeMap<String, String>,
    pub route_matrix: BTreeMap<String, Vec<String>>,
    pub na: BTreeMap<String, String>,
    pub fixtures: Vec<Fixture>,
}
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Fixture {
    pub id: String,
    pub source: String,
    pub linker: String,
    pub native_only: bool,
    pub source_sha256: String,
    pub linker_sha256: String,
    pub elf_sha256: String,
    pub entry: u64,
    pub final_pc: u64,
    pub exit_code: u32,
    pub process_code: i32,
    pub attempts: u64,
    pub turns: u64,
    pub retirements: u64,
    pub traps: u64,
    pub work: u64,
    pub checksum: u64,
    pub signature_addr: Option<u64>,
    pub signature: Option<Vec<u8>>,
    pub uart: Vec<u8>,
    pub events: Vec<Event>,
    pub ram: Vec<Ram>,
    pub regs: Vec<u64>,
    pub trace: Vec<Trace>,
    pub segments: Vec<Segment>,
    pub tohost: Option<u64>,
    pub memory_size: usize,
    pub signature_file_offset: Option<u64>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Ram {
    pub offset: u64,
    pub bytes: Vec<u8>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Segment {
    pub address: u64,
    pub physical_address: u64,
    pub flags: u32,
    pub file_size: usize,
    pub zero_fill: usize,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UartState {
    pub base_addr: u64,
    pub rx_fifo: Vec<u8>,
    pub tx_fifo: Vec<u8>,
    pub registers: [u8; 10],
}
// Ordered platform-event encoding avoids sorting away causal order.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Event(pub String, pub u64);
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Write {
    pub index: u8,
    pub before: u64,
    pub after: u64,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Memory {
    pub address: u64,
    pub width: usize,
    pub read: Option<Vec<u8>>,
    pub write: Option<Vec<u8>>,
    pub atomic: Option<String>,
    pub conditional: Option<String>,
    pub aq: bool,
    pub rl: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Trace {
    pub pc: u64,
    pub instruction: u32,
    pub next_pc: u64,
    pub gpr: Vec<Write>,
    pub memory: Vec<Memory>,
    pub reservation: Option<[bool; 2]>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Counts {
    pub attempts: u64,
    pub turns: u64,
    pub retirements: u64,
    pub traps: u64,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PublicResult {
    pub exit_code: u32,
    pub turns: u64,
    pub pc: u64,
    pub timed_out: bool,
    pub error: Option<String>,
    pub signature_addr: Option<u64>,
    pub signature: Option<Vec<u8>>,
}
impl From<ExecutionResult> for PublicResult {
    fn from(r: ExecutionResult) -> Self {
        Self {
            exit_code: r.exit_code,
            turns: r.cycles,
            pc: r.final_pc,
            timed_out: r.timed_out,
            error: r.error,
            signature_addr: r.signature_addr,
            signature: r.signature_data,
        }
    }
}
/// Absence is not an empty successful capture. Route capabilities determine
/// which evidence is mandatory; unsupported modes never receive a pass.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FactDetail {
    pub hart_id: u64,
    pub instruction_length: u8,
    pub privilege: u8,
    pub next_privilege: u8,
    pub minstret: u64,
    pub explicit_minstret_write: bool,
    pub csr: Vec<[u64; 4]>,
    pub fpr: Vec<Write>,
    pub fcsr: Option<[u32; 2]>,
    pub issued: Vec<Option<u64>>,
    pub indivisible: Vec<Option<bool>>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Sample {
    pub route: String,
    pub mode: String,
    pub result: Option<PublicResult>,
    pub process_code: Option<i32>,
    /// CLI metadata only: bytes are not exposed by the public CLI.
    pub cli_signature_size: Option<u64>,
    pub counts: Option<Counts>,
    pub minstret: Option<u64>,
    pub regs: Option<Vec<u64>>,
    pub ram: Option<Vec<Ram>>,
    pub uart: Option<Vec<u8>>,
    pub uart_state: Option<UartState>,
    pub signal: Option<[u64; 2]>,
    pub events: Option<Vec<Event>>,
    pub facts: Option<Vec<Trace>>,
    pub fact_details: Option<Vec<FactDetail>>,
    pub log: Option<String>,
    pub reporting_error: Option<String>,
}
impl Sample {
    pub fn public(route: &str, mode: &str, result: PublicResult) -> Self {
        Self {
            route: route.into(),
            mode: mode.into(),
            result: Some(result),
            process_code: None,
            cli_signature_size: None,
            counts: None,
            minstret: None,
            regs: None,
            ram: None,
            uart: None,
            uart_state: None,
            signal: None,
            events: None,
            facts: None,
            fact_details: None,
            log: None,
            reporting_error: None,
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Rejection {
    Semantic(String),
    Unavailable(String),
}
pub type Check = Result<(), Rejection>;
fn equal<T: PartialEq + std::fmt::Debug>(name: &str, actual: &T, expected: &T) -> Check {
    if actual == expected {
        Ok(())
    } else {
        Err(Rejection::Semantic(format!(
            "{name}: observed {actual:?}, expected {expected:?}"
        )))
    }
}
fn required<'a, T>(name: &str, value: &'a Option<T>) -> Result<&'a T, Rejection> {
    value
        .as_ref()
        .ok_or_else(|| Rejection::Unavailable(format!("missing {name}")))
}
pub fn manifest() -> Manifest {
    let m: Manifest =
        serde_json::from_str(include_str!("public-v1.json")).expect("versioned oracle manifest");
    manifest_capabilities(&m).expect("supported manifest version and capabilities");
    m
}
pub fn calibration_manifest() -> Manifest {
    let m: Manifest = serde_json::from_str(include_str!("calibration-oracle-v1.json"))
        .expect("versioned independent calibration oracle");
    manifest_capabilities(&m).expect("calibration oracle capabilities");
    m
}
pub fn manifest_capabilities(m: &Manifest) -> Check {
    if !matches!(
        m.schema.as_str(),
        "a10-oracle/1" | "a10-calibration-oracle/1"
    ) || m.version != 1
    {
        return Err(Rejection::Semantic("unknown oracle schema/version".into()));
    }
    let expected: BTreeMap<String, Vec<String>> = [
        ("machine-native", vec!["off", "facts", "file"]),
        ("machine-flat", vec!["off", "facts", "file"]),
        ("native-bytes", vec!["off", "file"]),
        ("native-file", vec!["off", "file"]),
        ("cli", vec!["off", "file"]),
        ("flat", vec!["off"]),
    ]
    .into_iter()
    .map(|(r, m)| (r.into(), m.into_iter().map(str::to_string).collect()))
    .collect();
    equal("capability/N/A classification", &m.route_matrix, &expected)
}
pub fn capability(m: &Manifest, f: &Fixture, route: &str, mode: &str) -> Check {
    manifest_capabilities(m)?;
    let anchor =
        f.id.strip_prefix("cal-")
            .and_then(|id| {
                id.strip_suffix("-exec")
                    .or_else(|| id.strip_suffix("-load"))
            })
            .unwrap_or(&f.id);
    if f.native_only != matches!(anchor, "hello" | "native_device") {
        return Err(Rejection::Semantic(
            "fixture native/flat N/A classification".into(),
        ));
    }
    if f.native_only && matches!(route, "flat" | "machine-flat") {
        return Err(Rejection::Unavailable("N/A: no flat device map".into()));
    }
    let modes = m
        .route_matrix
        .get(route)
        .ok_or_else(|| Rejection::Semantic("unknown route".into()))?;
    if !modes.iter().any(|s| s == mode) {
        return Err(Rejection::Unavailable(format!("N/A: {route}/{mode}")));
    }
    Ok(())
}
/// Text is a presentation oracle, never a substitute for immutable Hart facts.
pub fn expected_log(f: &Fixture) -> String {
    let mut log = String::new();
    for t in &f.trace {
        log.push_str(&format!(
            "core   0: 3 {:#018x} ({:#010x})",
            t.pc, t.instruction
        ));
        for w in &t.gpr {
            if w.before != w.after {
                log.push_str(&format!(" x{}  {:#018x}", w.index, w.after));
            }
        }
        log.push('\n');
    }
    log
}
/// Validates one actual public sample, including future warmup/calibration.
/// Nontrapping equality is declared by these sequences, not a general rename
/// of ExecutionResult.cycles into retirement. CLI exposes no signature bytes;
/// its guest self-check + exact linked work path/result is its own oracle.
pub fn validate(m: &Manifest, f: &Fixture, s: &Sample) -> Check {
    capability(m, f, &s.route, &s.mode)?;
    if s.reporting_error.is_some() {
        return Err(Rejection::Semantic(format!(
            "reporting: {:?}",
            s.reporting_error
        )));
    }
    let r = required("public result", &s.result)?;
    equal("exit", &r.exit_code, &f.exit_code)?;
    equal("PC", &r.pc, &f.final_pc)?;
    equal("completed turns", &r.turns, &f.turns)?;
    equal("timed_out", &r.timed_out, &false)?;
    equal("error", &r.error, &None)?;
    equal("signature address", &r.signature_addr, &f.signature_addr)?;
    // For CLI the bytes are NOT exposed. Do not copy a companion's bytes.
    if s.route != "cli" {
        if f.signature.is_some() {
            let bytes = required("signature bytes", &r.signature)?;
            if bytes.len() != 24 {
                return Err(Rejection::Semantic(
                    "signature/work/checksum/counter reader size".into(),
                ));
            }
            let reader = f.retirements.checked_sub(6).ok_or_else(|| {
                Rejection::Semantic("signature/work/checksum/counter reader underflow".into())
            })?;
            for (index, expected) in [(0, f.work), (8, f.checksum), (16, reader)] {
                let value = u64::from_le_bytes(bytes[index..index + 8].try_into().unwrap());
                equal("signature/work/checksum/counter reader", &value, &expected)?;
            }
        }
        equal(
            "signature/work/checksum/counter reader",
            &r.signature,
            &f.signature,
        )?;
    } else {
        if let Some(signature) = &f.signature {
            equal(
                "CLI signature size",
                required("CLI signature size", &s.cli_signature_size)?,
                &(signature.len() as u64),
            )?;
        } else {
            equal("CLI signature size", &s.cli_signature_size, &None)?;
        }
        equal(
            "process code",
            required("process code", &s.process_code)?,
            &f.process_code,
        )?;
        if r.signature.is_some() {
            return Err(Rejection::Semantic(
                "CLI unexpectedly claims signature bytes".into(),
            ));
        }
    }
    if s.route.starts_with("machine-") {
        equal(
            "attempts/turns/traps/retirements",
            required("counts", &s.counts)?,
            &Counts {
                attempts: f.attempts,
                turns: f.turns,
                retirements: f.retirements,
                traps: f.traps,
            },
        )?;
        equal(
            "device events/order",
            required("events", &s.events)?,
            &f.events,
        )?;
        let expected = if s.mode == "off" {
            Vec::new()
        } else {
            f.trace.clone()
        };
        let actual = required("facts", &s.facts)?;
        equal(
            "immutable facts/effects/order length",
            &actual.len(),
            &expected.len(),
        )?;
        for (index, (a, e)) in actual.iter().zip(&expected).enumerate() {
            equal(&format!("immutable facts/effects/order [{index}]"), a, e)?;
        }
    }
    if s.route == "flat" || s.route.starts_with("machine-") {
        equal(
            "MINSTRET",
            required("MINSTRET", &s.minstret)?,
            &f.retirements,
        )?;
        equal("GPR/x0", required("GPR", &s.regs)?, &f.regs)?;
        equal("RAM/neighbor/BSS", required("RAM", &s.ram)?, &f.ram)?;
        let address = f.tohost.unwrap_or(0x4000_8000);
        let payload = if f.tohost.is_some() {
            u64::from_le_bytes(
                f.trace.last().unwrap().memory[0]
                    .write
                    .as_ref()
                    .unwrap()
                    .as_slice()
                    .try_into()
                    .unwrap(),
            )
        } else {
            0
        };
        let expected_signal = if s.route == "machine-native" {
            [address, payload]
        } else {
            [
                address
                    .checked_sub(f.entry)
                    .ok_or_else(|| Rejection::Semantic("flat signal address".into()))?,
                if s.route == "flat" { 0 } else { payload },
            ]
        };
        equal(
            "selected signal/clear",
            required("signal", &s.signal)?,
            &expected_signal,
        )?;
        if s.route == "machine-native" {
            equal(
                "UART device state",
                required("UART device state", &s.uart_state)?,
                &UartState {
                    base_addr: 0x1000_0000,
                    rx_fifo: vec![],
                    tx_fifo: f.uart.iter().copied().take(16).collect(),
                    registers: [
                        0,
                        0,
                        0,
                        0,
                        if f.uart.is_empty() {
                            0x60
                        } else if f.uart.len() < 16 {
                            0x20
                        } else {
                            0
                        },
                        0,
                        0,
                        0,
                        0,
                        1,
                    ],
                },
            )?;
        } else if s.uart_state.is_some() {
            return Err(Rejection::Semantic(
                "flat route fabricated UART device state".into(),
            ));
        }
    }
    if matches!(
        s.route.as_str(),
        "cli" | "native-bytes" | "native-file" | "machine-native" | "machine-flat"
    ) {
        equal("UART", required("UART capture", &s.uart)?, &f.uart)?;
    }
    if s.mode == "file" {
        equal(
            "serialized file log",
            required("file log", &s.log)?,
            &expected_log(f),
        )?;
    } else if s.log.is_some() {
        return Err(Rejection::Semantic(
            "non-file mode claims serialized log".into(),
        ));
    }
    Ok(())
}

pub fn image_identity(f: &Fixture, image: &LoadImage) -> Check {
    equal("entry", &image.entry_point(), &f.entry)?;
    equal("base", &image.base_addr(), &f.entry)?;
    equal("memory footprint", &image.memory_size(), &f.memory_size)?;
    equal(
        "signature file offset",
        &image.signature().map(|s| s.file_offset),
        &f.signature_file_offset,
    )?;
    equal("tohost metadata", &image.tohost(), &f.tohost)?;
    equal(
        "signature metadata",
        &image.signature().map(|s| (s.vaddr, s.size)),
        &f.signature_addr
            .map(|a| (a, f.signature.as_ref().unwrap().len() as u64)),
    )?;
    let segments: Vec<_> = image
        .segments()
        .iter()
        .map(|s| Segment {
            address: s.guest_address,
            physical_address: s.physical_address,
            flags: s.flags.0,
            file_size: s.file_bytes.len(),
            zero_fill: s.zero_fill,
        })
        .collect();
    equal("ELF segments", &segments, &f.segments)
}
/// Capture only existing immutable facts; never snapshots/refetches for commits.
fn capture_fact(
    observation: &Observation,
    kind: PlatformKind,
    index: usize,
    base: u64,
) -> Result<Trace, String> {
    use ruscv_sim::physical::{AtomicAccessKind, ConditionalStatus};
    let Observation::Commit(c) = observation else {
        return Err("unexpected trap record".into());
    };
    let retired = c.retired;
    if c.hart_id != 0
        || c.instruction_length != 4
        || retired.explicit_minstret_write
        || retired.minstret != index as u64 + 1
        || retired.privilege as u8 != 3
        || retired.next_privilege as u8 != 3
        || !c.effects.fpr.is_empty()
        || c.effects.fcsr.is_some()
        || c.effects.csr
            != [ruscv_sim::csr::CsrAccess {
                addr: MINSTRET,
                old_value: index as u64,
                new_value: index as u64 + 1,
                wrote: true,
            }]
    {
        return Err("Hart identity/privilege/CSR/counter or unexpected FPR/FCSR effects".into());
    }
    let mut memory = Vec::new();
    for effect in &c.effects.memory {
        let issued = if kind == PlatformKind::Native {
            effect.guest_address
        } else {
            effect
                .guest_address
                .checked_sub(base)
                .ok_or("flat issued underflow")?
        };
        if effect.issued_address != Some(issued) {
            return Err("issued address/storage adaptation".into());
        }
        let atomic = effect.atomic;
        if atomic.is_some_and(|a| !a.indivisible) {
            return Err("typed atomic substitute".into());
        }
        memory.push(Memory {
            address: effect.guest_address,
            width: effect.width.bytes(),
            read: effect.read.map(|b| b[..effect.width.bytes()].to_vec()),
            write: effect.write.map(|b| b[..effect.width.bytes()].to_vec()),
            atomic: atomic.map(|a| {
                match a.kind {
                    AtomicAccessKind::Rmw => "rmw",
                    AtomicAccessKind::LoadReserved => "lr",
                    AtomicAccessKind::StoreConditional => "sc",
                }
                .into()
            }),
            conditional: atomic.and_then(|a| a.conditional).map(|s| {
                match s {
                    ConditionalStatus::Success => "success",
                    ConditionalStatus::Failure => "failure",
                }
                .into()
            }),
            aq: atomic.is_some_and(|a| a.ordering.aq),
            rl: atomic.is_some_and(|a| a.ordering.rl),
        });
    }
    Ok(Trace {
        pc: retired.pc,
        instruction: retired.instruction,
        next_pc: retired.next_pc,
        gpr: c
            .effects
            .gpr
            .iter()
            .map(|w| Write {
                index: w.index,
                before: w.before,
                after: w.after,
            })
            .collect(),
        memory,
        reservation: c
            .effects
            .reservation
            .as_ref()
            .map(|r| [r.before.is_some(), r.after.is_some()]),
    })
}
fn event(e: &PlatformEvent) -> Event {
    match e {
        PlatformEvent::UartTransmit(b) => Event("uart".into(), u64::from(*b)),
        PlatformEvent::HtifWrite(v) => Event("htif".into(), *v),
    }
}
fn exit(value: u64) -> Option<u32> {
    // Only two fixture exit encodings, no instruction semantics.
    if value >> 63 != 0 {
        Some(value as u32)
    } else if value & 1 != 0 && value >> 48 == 0 {
        Some((value >> 1) as u32)
    } else {
        None
    }
}
pub type UartCapture = Arc<Mutex<Vec<u8>>>;
pub fn new_machine(
    f: &Fixture,
    bytes: &[u8],
    kind: PlatformKind,
) -> Result<(Machine, UartCapture), String> {
    let (config, uart) = machine_config(kind);
    let owner = Machine::new(config);
    let image = Arc::new(LoadImage::parse(bytes).map_err(|e| e.to_string())?);
    image_identity(f, &image).map_err(|e| format!("{e:?}"))?;
    owner.install(image).map_err(|e| e.to_string())?;
    Ok((owner, uart))
}
pub fn machine_config(kind: PlatformKind) -> (MachineConfig, UartCapture) {
    let uart = Arc::new(Mutex::new(Vec::new()));
    let captured = uart.clone();
    let mut config = MachineConfig::new(kind);
    config.uart_output = Some(Arc::new(move |b| captured.lock().unwrap().push(b)));
    (config, uart)
}
struct FactSink {
    facts: Vec<Trace>,
    details: Vec<FactDetail>,
    kind: PlatformKind,
    base: u64,
}
impl ObservationSink for FactSink {
    type Error = String;
    fn observe(&mut self, observation: &Observation) -> Result<(), String> {
        let record = capture_fact(observation, self.kind, self.facts.len(), self.base)?;
        let Observation::Commit(c) = observation else {
            return Err("unexpected trap".into());
        };
        self.details.push(FactDetail {
            hart_id: c.hart_id,
            instruction_length: c.instruction_length,
            privilege: c.retired.privilege as u8,
            next_privilege: c.retired.next_privilege as u8,
            minstret: c.retired.minstret,
            explicit_minstret_write: c.retired.explicit_minstret_write,
            csr: c
                .effects
                .csr
                .iter()
                .map(|a| [a.addr as u64, a.old_value, a.new_value, u64::from(a.wrote)])
                .collect(),
            fpr: c
                .effects
                .fpr
                .iter()
                .map(|w| Write {
                    index: w.index,
                    before: w.before,
                    after: w.after,
                })
                .collect(),
            fcsr: c.effects.fcsr.map(|(a, b)| [a, b]),
            issued: c.effects.memory.iter().map(|m| m.issued_address).collect(),
            indivisible: c
                .effects
                .memory
                .iter()
                .map(|m| m.atomic.map(|a| a.indivisible))
                .collect(),
        });
        self.facts.push(record);
        Ok(())
    }
}
/// Harness-owned preparation and immutable capture, not a production hook.
#[derive(Debug, Clone, Copy)]
pub enum TurnEvent {
    Step,
    Deliver,
    Consume,
}
pub struct MachineRun {
    logger: Option<CommitLogger>,
    sink: FactSink,
    counts: Counts,
    events: Vec<Event>,
    code: Option<u32>,
    reporting_error: Option<String>,
    observe: bool,
}
impl MachineRun {
    pub fn prepare(
        f: &Fixture,
        kind: PlatformKind,
        mode: &str,
        log: &Path,
    ) -> Result<Self, String> {
        if !matches!(mode, "off" | "facts" | "file") {
            return Err("unsupported Machine observation mode".into());
        }
        Ok(Self {
            logger: if mode == "file" {
                Some(CommitLogger::new_file(log).map_err(|e| e.to_string())?)
            } else {
                None
            },
            sink: FactSink {
                details: if mode == "off" {
                    Vec::new()
                } else {
                    Vec::with_capacity(f.turns as usize)
                },
                facts: if mode == "off" {
                    Vec::new()
                } else {
                    Vec::with_capacity(f.turns as usize)
                },
                kind,
                base: f.entry,
            },
            counts: Counts {
                attempts: 0,
                turns: 0,
                retirements: 0,
                traps: 0,
            },
            events: Vec::with_capacity(f.events.len()),
            code: None,
            reporting_error: None,
            observe: mode != "off",
        })
    }
    /// Only actual turns, synchronous delivery and receipt consumption. No
    /// resume, logger construction/close, final inspection or validation here.
    pub fn execute(
        &mut self,
        f: &Fixture,
        owner: &Machine,
        mut notify: impl FnMut(TurnEvent),
    ) -> Result<(), String> {
        let Self {
            logger,
            sink,
            counts,
            events,
            code,
            reporting_error,
            observe,
        } = self;
        for _ in 0..f.turns + 16 {
            notify(TurnEvent::Step);
            let turn = owner.step(*observe).map_err(|e| e.to_string())?;
            let hart = turn.hart();
            let c = hart.control;
            counts.attempts += u64::from(c.instruction_attempted);
            counts.retirements += u64::from(c.retired);
            counts.traps += u64::from(c.trap_entered);
            counts.turns += u64::from(c.retired || c.trap_entered);
            if c.minstret_before + u64::from(c.retired) != c.minstret_after {
                *reporting_error = Some("control counter mismatch".into());
            }
            if *observe {
                notify(TurnEvent::Deliver);
                if let Err(error) = turn.deliver(sink) {
                    *reporting_error = Some(error);
                }
                if let Some(logger) = logger {
                    if let Err(e) = turn.deliver(logger) {
                        *reporting_error = Some(e.to_string());
                    }
                }
            }
            for e in turn.events() {
                events.push(event(e));
                if let PlatformEvent::HtifWrite(value) = e {
                    *code = exit(*value)
                }
            }
            if code.is_none() {
                *code = turn
                    .tohost()
                    .and_then(|s| s.value.as_ref().ok())
                    .and_then(|v| exit(*v));
            }
            let failed = matches!(
                hart.outcome,
                ruscv_sim::core::StepOutcome::SimulatorFailure(_)
            );
            drop(turn);
            notify(TurnEvent::Consume);
            if code.is_some() || failed {
                break;
            }
        }
        Ok(())
    }
    /// Unbuffered public file writes complete during delivery. Closing has no
    /// public flush error seam; no buffering/fsync/durability claim is made.
    pub fn close_log(&mut self) {
        drop(self.logger.take());
    }
    pub fn inspect(
        self,
        f: &Fixture,
        owner: &Machine,
        uart: &UartCapture,
        kind: PlatformKind,
        mode: &str,
        log_path: &Path,
    ) -> Result<Sample, String> {
        let Self {
            sink,
            counts,
            events,
            code,
            reporting_error,
            ..
        } = self;
        let facts = sink.facts;
        let details = sink.details;
        let inspection = owner.inspect().map_err(|e| e.to_string())?;
        let signature = inspection
            .signature
            .transpose()
            .map_err(|e| e.to_string())?;
        let mut sample = Sample::public(
            if kind == PlatformKind::Native {
                "machine-native"
            } else {
                "machine-flat"
            },
            mode,
            PublicResult {
                exit_code: code.unwrap_or(1),
                turns: counts.turns,
                pc: inspection.hart.pc,
                timed_out: code.is_none(),
                error: None,
                signature_addr: inspection.image.signature().map(|s| s.vaddr),
                signature,
            },
        );
        sample.counts = Some(counts);
        sample.minstret = Some(
            inspection
                .hart
                .csr
                .read(MINSTRET)
                .map_err(|e| e.to_string())?,
        );
        sample.regs = Some(inspection.hart.regs.to_vec());
        sample.ram = Some(
            f.ram
                .iter()
                .map(|r| {
                    owner.read_mem(r.offset, r.bytes.len()).map(|bytes| Ram {
                        offset: r.offset,
                        bytes,
                    })
                })
                .collect::<Result<_, _>>()
                .map_err(|e| e.to_string())?,
        );
        sample.events = Some(events);
        sample.signal = Some([
            inspection.tohost.address,
            inspection.tohost.value.map_err(|e| e.to_string())?,
        ]);
        sample.uart_state = inspection.uart.map(|u| UartState {
            base_addr: u.base_addr,
            rx_fifo: u.rx_fifo,
            tx_fifo: u.tx_fifo,
            registers: u.registers,
        });
        sample.facts = Some(facts);
        sample.fact_details = Some(details);
        sample.uart = Some(uart.lock().unwrap().clone());
        sample.reporting_error = reporting_error;
        if mode == "file" {
            sample.log = Some(std::fs::read_to_string(log_path).map_err(|e| e.to_string())?)
        }
        Ok(sample)
    }
}
/// P0 behavior preserved; P1 clocks only `MachineRun::execute` after preparation.
pub fn capture_machine(
    f: &Fixture,
    owner: &Machine,
    uart: &UartCapture,
    kind: PlatformKind,
    mode: &str,
    log_path: &Path,
) -> Result<Sample, String> {
    owner.resume().map_err(|e| e.to_string())?;
    let mut run = MachineRun::prepare(f, kind, mode, log_path)?;
    run.execute(f, owner, |_| {})?;
    run.close_log();
    run.inspect(f, owner, uart, kind, mode, log_path)
}
