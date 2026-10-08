//! Versioned semantic reader: reuse P0, never execute guest semantics in a parser.
use crate::{
    phases::{self, Op, Record, ScopeAudit},
    support::*,
};
use serde::de::{self, MapAccess, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer};
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
    path::Path,
};
/// serde_json::Value normally silently keeps the last duplicate key. Refuse
/// duplicates at every depth before typed sample deserialization/replay.
struct Unique(Value);
impl<'de> Deserialize<'de> for Unique {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = Unique;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("unique-key finite JSON")
            }
            fn visit_map<A: MapAccess<'de>>(self, mut a: A) -> Result<Unique, A::Error> {
                let mut m = serde_json::Map::new();
                while let Some((key, Unique(value))) = a.next_entry::<String, Unique>()? {
                    if m.insert(key.clone(), value).is_some() {
                        return Err(de::Error::custom(format!("duplicate JSON key {key}")));
                    }
                }
                Ok(Unique(Value::Object(m)))
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut a: A) -> Result<Unique, A::Error> {
                let mut v = Vec::new();
                while let Some(Unique(e)) = a.next_element()? {
                    v.push(e);
                }
                Ok(Unique(Value::Array(v)))
            }
            fn visit_bool<E: de::Error>(self, v: bool) -> Result<Unique, E> {
                Ok(Unique(v.into()))
            }
            fn visit_u64<E: de::Error>(self, v: u64) -> Result<Unique, E> {
                Ok(Unique(v.into()))
            }
            fn visit_i64<E: de::Error>(self, v: i64) -> Result<Unique, E> {
                Ok(Unique(v.into()))
            }
            fn visit_f64<E: de::Error>(self, v: f64) -> Result<Unique, E> {
                serde_json::Number::from_f64(v)
                    .map(|n| Unique(Value::Number(n)))
                    .ok_or_else(|| E::custom("nonfinite JSON"))
            }
            fn visit_str<E: de::Error>(self, v: &str) -> Result<Unique, E> {
                Ok(Unique(v.into()))
            }
            fn visit_string<E: de::Error>(self, v: String) -> Result<Unique, E> {
                Ok(Unique(v.into()))
            }
            fn visit_unit<E: de::Error>(self) -> Result<Unique, E> {
                Ok(Unique(Value::Null))
            }
            fn visit_none<E: de::Error>(self) -> Result<Unique, E> {
                Ok(Unique(Value::Null))
            }
        }
        d.deserialize_any(V)
    }
}
pub fn unique_json(bytes: &[u8]) -> Result<Value, String> {
    serde_json::from_slice::<Unique>(bytes)
        .map(|v| v.0)
        .map_err(|e| e.to_string())
}
fn need(ok: bool, why: &str) -> Result<(), String> {
    if ok {
        Ok(())
    } else {
        Err(why.into())
    }
}
fn read(root: &Path, reference: &Value) -> Result<Vec<u8>, String> {
    let name = reference.as_str().ok_or("artifact reference type")?;
    let path = Path::new(name);
    need(
        !path.is_absolute()
            && !name.is_empty()
            && path
                .components()
                .all(|p| matches!(p, std::path::Component::Normal(_))),
        "unsafe artifact reference",
    )?;
    let mut full = root.to_path_buf();
    for part in path.components() {
        full.push(part);
        need(!full.is_symlink(), "symlink artifact reference")?;
    }
    std::fs::read(full).map_err(|e| e.to_string())
}
fn expected_ops(route: &str, phase: &str, mode: &str, turns: usize) -> Vec<Op> {
    use Op::*;
    let mut ops = if phase == "load_only" {
        vec![
            Prepare, Start, Parse, Construct, Install, Stop, Inspect, Drain, DropOwner,
        ]
    } else if route.starts_with("machine-") {
        let mut v = vec![Drain, Reset, Inspect, Resume, Prepare, Start];
        for _ in 0..turns {
            v.push(Step);
            if mode != "off" {
                v.push(Deliver);
            }
            v.push(Consume);
        }
        v.extend([Stop, CloseLog, Inspect]);
        v
    } else if route == "flat" && phase == "execute_only" {
        vec![Reset, Inspect, Start, FlatRun, Stop, Inspect]
    } else if route == "flat" {
        vec![
            Start, Construct, Install, FlatRun, FinalCopy, DropOwner, Stop,
        ]
    } else if route == "cli" {
        vec![Start, ChildLaunch, ChildWait, Stop]
    } else {
        vec![ChildLaunch, ChildWait]
    };
    if phase != "load_only" {
        ops.push(Validate);
    }
    ops.push(Report);
    ops
}
/// Python schema/bundle reader calls this exact compiled public driver on the
/// same raw report after structural/integrity checks. No claimed verdict is proof.
pub fn validate_report(root: &Path, report: &Value) -> Result<i32, String> {
    need(
        report["schema"] == "ruscv-perf/1",
        "unknown schema/major; legacy is not a baseline",
    )?;
    let reps = report["policy"]["basic_repetitions"]
        .as_u64()
        .ok_or("repetitions")?;
    need((1..=16).contains(&reps), "P2 smoke repetitions")?;
    let m = manifest();
    // Parse/reconstruct each immutable pinned initial image once per read. This
    // caches expected bytes only, never a sample verdict or companion run.
    let mut images = BTreeMap::new();
    for f in &m.fixtures {
        let elf = read(root, &Value::String(format!("fixtures/{}.elf", f.id)))?;
        need(
            phases::digest::sha256(&elf) == f.elf_sha256,
            "retained ELF bytes mismatch",
        )?;
        let image = ruscv_sim::elf::LoadImage::parse(&elf).map_err(|e| e.to_string())?;
        image_identity(f, &image).map_err(|e| format!("{e:?}"))?;
        let mut memory = vec![0; image.memory_size()];
        for seg in image.segments() {
            let offset = (seg.guest_address - image.base_addr()) as usize;
            memory[offset..offset + seg.file_bytes.len()].copy_from_slice(&seg.file_bytes);
        }
        images.insert(
            f.id.clone(),
            (phases::digest::sha256(&memory), memory.len()),
        );
    }
    let mut expected = Vec::new();
    for f in &m.fixtures {
        for route in m.route_matrix.keys() {
            for phase in ["load_only", "execute_only", "end_to_end"] {
                for mode in if phase == "load_only" {
                    vec!["none"]
                } else {
                    vec!["off", "facts", "file"]
                } {
                    let applicable = phases::availability(&m, f, route, phase, mode).is_ok();
                    for rep in 0..=if applicable { reps } else { 0 } {
                        expected.push((
                            f.id.as_str(),
                            route.as_str(),
                            phase,
                            mode,
                            rep,
                            applicable,
                        ));
                    }
                }
            }
        }
    }
    let rows = report["records"].as_array().ok_or("records")?;
    need(
        rows.len() == expected.len(),
        "missing/extra cell/repetition",
    )?;
    let mut ids = BTreeSet::new();
    let mut failed = false;
    let mut unavailable = false;
    let mut semantic_unavailable = false;
    for (index, (entry, &(fixture, route, phase, mode, rep, applicable))) in
        rows.iter().zip(&expected).enumerate()
    {
        let id = entry["id"].as_str().ok_or("sample id")?;
        need(ids.insert(id), "duplicate sample id")?;
        need(
            entry["sequence"].as_u64() == Some(index as u64),
            "sample order",
        )?;
        let r: Record = serde_json::from_value(entry["raw"].clone()).map_err(|e| e.to_string())?;
        need(
            (
                r.fixture.as_str(),
                r.route.as_str(),
                r.phase.as_str(),
                r.mode.as_str(),
                r.repetition as u64,
            ) == (fixture, route, phase, mode, rep),
            "cell/repetition identity mismatch",
        )?;
        need(r.warmup == (applicable && rep == 0), "warmup identity")?;
        let f = m.fixtures.iter().find(|f| f.id == fixture).unwrap();
        let (scope, policy) = phases::scope(route, phase);
        need(
            r.scope == scope && r.capture_policy == policy,
            "phase/capture scope identity",
        )?;
        if !applicable {
            need(
                r.semantic_status == "not_applicable"
                    && r.interval.is_none()
                    && r.accepted_ns.is_none()
                    && r.sample.is_none()
                    && r.load.is_none()
                    && r.reason.is_some(),
                "fabricated N/A timing/oracle",
            )?;
            need(
                r.reason == phases::availability(&m, f, route, phase, mode).err()
                    && r.initial.is_none()
                    && r.clock.is_none()
                    && r.input_sha256.is_none()
                    && r.ops.is_empty()
                    && r.native_ops.is_none()
                    && r.argv.is_empty()
                    && r.transport_code.is_none()
                    && r.measurement_status == "not_applicable",
                "N/A fabricated lifecycle/clock/result",
            )?;
            continue;
        }
        if r.semantic_status == "correct" {
            need(
                r.accepted_ns
                    == if r.scope_error.is_none() {
                        r.interval.as_ref().map(|i| i.elapsed_ns)
                    } else {
                        None
                    },
                "discarded/changed correct raw interval",
            )?;
        } else {
            need(r.accepted_ns.is_none(), "noncorrect sample received timing")?;
        }
        need(
            r.input_sha256.as_deref() == Some(f.elf_sha256.as_str()),
            "actual input ELF identity",
        )?;
        if let Some(interval) = &r.interval {
            let backwards = interval.stop_ns < interval.start_ns;
            if backwards {
                need(
                    interval.elapsed_ns == 0
                        && r.scope_error.as_deref() == Some("nonmonotonic clock")
                        && r.accepted_ns.is_none(),
                    "nonmonotonic clock accepted/unreported",
                )?;
                unavailable = true;
            } else {
                need(
                    interval.stop_ns.checked_sub(interval.start_ns) == Some(interval.elapsed_ns),
                    "invalid/overflowed interval",
                )?;
            }
            need(
                interval.origin
                    == if route.starts_with("native-") {
                        "library-child-Instant"
                    } else {
                        "driver-Instant"
                    },
                "parent/child origin mismatch",
            )?;
            let c = r.clock.as_ref().ok_or("missing actual clock")?;
            need(
                c.engine == "std::time::Instant"
                    && c.unit == "ns"
                    && c.monotonic
                    && c.empty_timer_ns.len() == 64
                    && c.successive_read_ns.len() == 64,
                "clock method/control count",
            )?;
            let resolution = c
                .empty_timer_ns
                .iter()
                .chain(&c.successive_read_ns)
                .copied()
                .filter(|v| *v > 0)
                .min();
            need(
                c.observed_resolution_ns == resolution,
                "clock resolution accounting",
            )?;
            let mut scope = ScopeAudit::new(phase, route, mode);
            for op in &r.ops {
                scope.event(*op);
            }
            if let Err(error) = scope.check() {
                need(
                    r.scope_error.as_deref() == Some(error.as_str()) && r.accepted_ns.is_none(),
                    "scope invariant violation accepted/unreported",
                )?;
                unavailable = true;
            } else if !backwards && !route.starts_with("native-") {
                need(
                    r.scope_error.is_none(),
                    "fabricated unavailable scope/discard",
                )?;
            }
            if r.scope_error.is_none() && r.semantic_status == "correct" {
                let expected = expected_ops(route, phase, mode, f.turns as usize);
                need(
                    r.ops == expected,
                    "ordered actual phase/reset/receipt trace mismatch",
                )?;
            }
            if route.starts_with("native-") {
                let mut child = ScopeAudit::new(phase, route, mode);
                for op in r.native_ops.as_ref().ok_or("missing native scope ops")? {
                    child.event(*op);
                }
                child.check()?;
                let mut expected = vec![Op::Start, Op::NativeCall, Op::UartFlush, Op::Stop];
                if route == "native-bytes" {
                    expected.insert(0, Op::ReadInput);
                }
                need(
                    r.native_ops.as_ref() == Some(&expected),
                    "native ordered actual call scope",
                )?;
            }
            if r.scope_error.is_some() {
                unavailable = true;
                need(r.accepted_ns.is_none(), "bad scope accepted")?;
            }
        } else {
            need(
                r.accepted_ns.is_none() && (r.reason.is_some() || r.scope_error.is_some()),
                "missing interval without diagnostic",
            )?;
            unavailable = true;
        }
        if phase == "load_only" {
            if let Some(l) = &r.load {
                need(
                    r.sample.is_none()
                        && l.execution_not_started
                        && l.pc == f.entry
                        && l.minstret == 0
                        && l.completed_turns == 0
                        && l.checked_bytes == f.memory_size,
                    "load executed/invented guest result",
                )?;
            } else {
                unavailable = true;
                need(
                    r.semantic_status != "correct",
                    "missing load evidence claimed correct",
                )?;
            }
        } else if let Some(s) = &r.sample {
            need(
                s.route == route && s.mode == mode,
                "own sample route mismatch",
            )?;
            let verdict = validate(&m, f, s);
            if let Err(e) = verdict {
                failed = true;
                need(
                    r.accepted_ns.is_none(),
                    &format!("bad oracle received timing: {e:?}"),
                )?;
                if r.semantic_status == "correct" {
                    return Err(format!("claimed-correct bad repetition: {e:?}"));
                }
            }
            if r.semantic_status == "semantic_failure" {
                failed = true;
                need(r.reason.is_some(), "failure diagnostic")?;
            }
            if route.starts_with("machine-") {
                let details = s
                    .fact_details
                    .as_ref()
                    .ok_or("missing direct fact details")?;
                need(
                    details.len() == if mode == "off" { 0 } else { f.trace.len() },
                    "fact detail count",
                )?;
                for (i, (d, t)) in details.iter().zip(&f.trace).enumerate() {
                    let base = if route == "machine-native" {
                        0
                    } else {
                        f.entry
                    };
                    need(
                        d.hart_id == 0
                            && d.instruction_length == 4
                            && d.privilege == 3
                            && d.next_privilege == 3
                            && d.minstret == i as u64 + 1
                            && !d.explicit_minstret_write
                            && d.csr == vec![[0xb02, i as u64, i as u64 + 1, 1]]
                            && d.fpr.is_empty()
                            && d.fcsr.is_none(),
                        "direct Hart/CSR/FPR details",
                    )?;
                    need(
                        d.issued
                            == t.memory
                                .iter()
                                .map(|e| e.address.checked_sub(base))
                                .collect::<Vec<_>>()
                            && d.indivisible
                                == t.memory
                                    .iter()
                                    .map(|e| e.atomic.as_ref().map(|_| true))
                                    .collect::<Vec<_>>(),
                        "issued/indivisible fact details",
                    )?;
                }
            } else {
                need(
                    s.fact_details.is_none(),
                    "facade fabricated subscriber facts",
                )?;
            }
            let refs = &entry["artifacts"];
            if mode == "file" {
                let log = read(root, &refs["log"])?;
                need(
                    s.log.as_deref().map(str::as_bytes) == Some(log.as_slice()),
                    "raw log bytes disagree with sample",
                )?;
            }
            if route == "cli" {
                let stdout =
                    String::from_utf8(read(root, &refs["stdout"])?).map_err(|e| e.to_string())?;
                let stderr = read(root, &refs["stderr"])?;
                let captured =
                    routes::parse_cli(&stdout, &stderr, r.transport_code, mode, s.log.clone())?;
                need(
                    serde_json::to_value(captured).unwrap() == serde_json::to_value(s).unwrap(),
                    "CLI sample not its retained own stdout/status",
                )?;
            }
            if route.starts_with("native-") {
                need(r.transport_code == Some(0), "native transport process code")?;
                need(
                    read(root, &refs["stderr"])?.is_empty(),
                    "native stderr failure",
                )?;
                let stdout =
                    String::from_utf8(read(root, &refs["stdout"])?).map_err(|e| e.to_string())?;
                let (uart, json) = stdout
                    .split_once("\nA10_P1_NATIVE ")
                    .ok_or("native transport marker")?;
                let wire: phases::LibraryWire =
                    serde_json::from_value(unique_json(json.trim().as_bytes())?)
                        .map_err(|e| e.to_string())?;
                wire.check(route)?;
                need(r.scope_error.is_none(), "fabricated native scope/discard")?;
                need(
                    r.clock.as_ref() == Some(&wire.clock)
                        && r.interval.as_ref() == Some(&wire.interval)
                        && r.native_ops.as_ref() == Some(&wire.ops)
                        && r.input_sha256.as_ref() == Some(&wire.input_sha256),
                    "native interval/clock not own retained transport",
                )?;
                // The public native facade exports only this result, UART and
                // optional file log. Compare the complete reconstructed carrier
                // so unsupported introspection cannot be labeled as observed.
                let mut captured = Sample::public(route, mode, wire.result);
                captured.uart = Some(uart.as_bytes().to_vec());
                captured.log = if mode == "file" {
                    Some(String::from_utf8(read(root, &refs["log"])?).map_err(|e| e.to_string())?)
                } else {
                    None
                };
                need(
                    serde_json::to_value(captured).unwrap() == serde_json::to_value(s).unwrap(),
                    "native sample not its complete retained own public capture",
                )?;
            }
        } else {
            unavailable = true;
            need(
                r.semantic_status != "correct" && r.reason.is_some(),
                "missing oracle claimed correct",
            )?;
        }
        if phase == "load_only" || phase == "execute_only" {
            if let Some(p) = &r.initial {
                need(
                    entry["fixture_elf"] == format!("fixtures/{}.elf", f.id),
                    "fixture reference mismatch",
                )?;
                let (hash, length) = &images[&f.id];
                need(
                    &p.image_sha256 == hash
                        && p.checked_bytes == *length
                        && p.pc == f.entry
                        && p.regs == vec![0; 32]
                        && p.minstret == 0
                        && p.reservation_clear
                        && p.signal[1] == 0,
                    "initial image/BSS/counter/reservation proof",
                )?;
                if route.starts_with("machine-") {
                    need(
                        p.generation.is_some() && p.events.as_ref().is_some_and(Vec::is_empty),
                        "Machine initial generation/events",
                    )?;
                    need(
                        p.signal[0]
                            == if route == "machine-native" {
                                f.tohost.unwrap_or(0x40008000)
                            } else {
                                f.tohost.unwrap() - f.entry
                            },
                        "initial selected signal",
                    )?;
                    if route == "machine-native" {
                        let u = p.uart.as_ref().ok_or("initial UART")?;
                        need(
                            u.base_addr == 0x10000000
                                && u.rx_fifo.is_empty()
                                && u.tx_fifo.is_empty()
                                && u.registers == [0, 0, 0, 0, 0x60, 0, 0, 0, 0, 1],
                            "initial UART state",
                        )?;
                    } else {
                        need(p.uart.is_none(), "flat initial device invented")?;
                    }
                    if phase == "execute_only" {
                        need(
                            p.previous_generation
                                .zip(p.generation)
                                .is_some_and(|(a, b)| a.checked_add(1) == Some(b))
                                && p.drain.as_deref() == Some("DrainComplete")
                                && p.stale_isolated == Some(true),
                            "reset/drain/generation/stale proof",
                        )?;
                    }
                } else {
                    need(
                        p.generation.is_none()
                            && p.previous_generation.is_none()
                            && p.drain.is_none()
                            && p.stale_isolated.is_none()
                            && !p.limits.is_empty(),
                        "facade generation/drain fabricated",
                    )?;
                }
            } else {
                unavailable = true;
                need(r.semantic_status != "correct", "missing initial proof")?;
            }
        }
        if r.semantic_status == "semantic_failure" {
            failed = true;
        }
        if r.semantic_status == "unavailable" {
            unavailable = true;
            semantic_unavailable = true;
        }
    }
    let code = if failed {
        1
    } else if unavailable {
        2
    } else {
        0
    };
    need(
        report["semantic_status"]
            == if failed {
                "semantic_failure"
            } else if semantic_unavailable {
                "unavailable"
            } else {
                "correct"
            },
        "top semantic status disagrees with replay",
    )?;
    Ok(code)
}
