//! Serial calibration controller over the unchanged public P1/P0 orchestrator.
use crate::{phases::*, support::*};
use serde_json::{json, Value};
use std::{
    io::Write,
    time::{Duration, Instant},
};

struct Retention {
    frames: Vec<Vec<u8>>,
    bytes: usize,
}
impl Retention {
    fn new() -> Self {
        Self {
            frames: Vec::new(),
            bytes: 0,
        }
    }
    fn emit(&mut self, value: Value) -> Result<(), String> {
        let control = !matches!(value["event"].as_str(), Some("row" | "value"));
        let mut frame = serde_json::to_vec(&value).map_err(|e| e.to_string())?;
        frame.push(b'\n');
        self.bytes += frame.len();
        self.frames.push(frame);
        // No bytes escape this process between flush boundaries. Parent
        // compression cannot overlap guest clocks. All own-oracle rows remain
        // in a bounded owned buffer, never live owners or reused verdicts.
        if control || self.frames.len() >= 256 || self.bytes >= 8 * 1024 * 1024 {
            self.flush()?;
        }
        Ok(())
    }
    fn flush(&mut self) -> Result<(), String> {
        let mut output = std::io::stdout().lock();
        for frame in &self.frames {
            output.write_all(frame).map_err(|e| e.to_string())?;
        }
        output.flush().map_err(|e| e.to_string())?;
        if std::env::var("RISCV_PERF_STREAM_ACK").as_deref() == Ok("1") {
            for _ in &self.frames {
                let mut acknowledgement = String::new();
                std::io::stdin()
                    .read_line(&mut acknowledgement)
                    .map_err(|e| e.to_string())?;
                if acknowledgement != "1\n" {
                    return Err(
                        "raw retention acknowledgement failed; no successor interval".into(),
                    );
                }
            }
        }
        self.frames.clear();
        self.bytes = 0;
        Ok(())
    }
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Stage {
    Pilot,
    Warmup,
    Basic,
    Done,
}
struct Controller<'a> {
    retention: &'a mut Retention,
    policy: &'a Value,
    deadline: Instant,
    stage: Stage,
    rep: usize,
    pilots: Vec<u64>,
    repetitions: usize,
    warmup_start: Option<Instant>,
    created: Instant,
    warmup_wall_start: Option<u128>,
    warmup_wall_stop: Option<u128>,
    warmup_ns: u128,
    warmup_min_ns: Option<u64>,
    warmup_iterations: usize,
    batch: usize,
    in_batch: usize,
    batch_ns: u128,
    failed: bool,
    error: Option<String>,
    reason: Option<String>,
    samples: Vec<Value>,
    input_plan: Option<usize>,
    not_applicable: bool,
    sample_value: Option<Sample>,
    clock_value: Option<ClockEvidence>,
    sample_id: u64,
    clock_id: u64,
}
impl<'a> Controller<'a> {
    fn new(
        policy: &'a Value,
        retention: &'a mut Retention,
        deadline: Instant,
        input_plan: Option<usize>,
    ) -> Self {
        Self {
            retention,
            policy,
            deadline,
            stage: Stage::Pilot,
            rep: 0,
            pilots: Vec::new(),
            repetitions: 0,
            warmup_start: None,
            created: Instant::now(),
            warmup_wall_start: None,
            warmup_wall_stop: None,
            warmup_ns: 0,
            warmup_min_ns: None,
            warmup_iterations: 0,
            batch: 0,
            in_batch: 0,
            batch_ns: 0,
            failed: false,
            error: None,
            reason: None,
            samples: Vec::new(),
            input_plan,
            not_applicable: false,
            sample_value: None,
            clock_value: None,
            sample_id: 0,
            clock_id: 0,
        }
    }
    fn retained_row(
        &mut self,
        mut record: Record,
        role: &str,
        batch: Option<usize>,
    ) -> Result<(), String> {
        // Equality is over the COMPLETE OWN captured value after this run's
        // fresh P0 validation. This saves serialized repetition, never execution,
        // inspection, oracle checks, or a sample verdict. Values are plain owned
        // data; no live owner/receipt/drop is retained by this encoding.
        let sample = record.sample.take();
        let clock = record.clock.take();
        let sample_ref = if let Some(sample) = sample {
            if self.sample_value.as_ref() != Some(&sample) {
                self.sample_id += 1;
                self.retention.emit(
                    json!({"event":"value","field":"sample","id":self.sample_id,"value":sample}),
                )?;
                self.sample_value = Some(sample);
            }
            Some(self.sample_id)
        } else {
            None
        };
        let clock_ref = if let Some(clock) = clock {
            if self.clock_value.as_ref() != Some(&clock) {
                self.clock_id += 1;
                self.retention.emit(
                    json!({"event":"value","field":"clock","id":self.clock_id,"value":clock}),
                )?;
                self.clock_value = Some(clock);
            }
            Some(self.clock_id)
        } else {
            None
        };
        let mut raw = serde_json::to_value(record).map_err(|e| e.to_string())?;
        if let Some(id) = sample_ref {
            raw["sample"] = json!({"retained_value":id});
        }
        if let Some(id) = clock_ref {
            raw["clock"] = json!({"retained_value":id});
        }
        self.retention
            .emit(json!({"event":"row","role":role,"batch":batch,"raw":raw}))
    }
    fn n(&self, key: &str) -> u64 {
        self.policy["sampling"][key]
            .as_u64()
            .expect("compiled policy integer")
    }
    fn stop(&mut self, reason: &str) {
        if self.stage == Stage::Warmup {
            self.warmup_wall_stop = Some(self.created.elapsed().as_nanos());
        }
        self.reason = Some(reason.into());
        self.stage = Stage::Done;
    }
    fn summary(&self) -> Value {
        if self.not_applicable {
            return json!({"pilot_count":0,"repetitions_per_sample":0,"warmup_iterations":0,"warmup_ns":0,"warmup_min_ns":null,"warmup_wall":null,"samples":[],"reason":null,"cleanup_error":null,"semantic_failure":false,"completed":true});
        }
        let mut samples = self.samples.clone();
        if self.in_batch > 0 {
            samples.push(json!({"batch":self.batch,"status":"partial","repetitions":self.in_batch,"sum_ns":self.batch_ns,"first_repetition":self.rep-self.in_batch,"last_repetition":self.rep-1,"reason":self.reason.as_deref().unwrap_or("partial rejected batch; no accepted aggregate")}));
        }
        for batch in samples.len()..self.n("accepted_samples") as usize {
            samples.push(json!({"batch":batch,"status":"unstarted","repetitions":0,"sum_ns":null,"first_repetition":null,"last_repetition":null,"reason":self.reason.as_deref().unwrap_or("prior failure; no retry/reuse")}));
        }
        let warmup_wall = self.warmup_wall_start.zip(self.warmup_wall_stop).map(|(start,stop)|json!({"engine":"std::time::Instant","unit":"ns","origin":"per-cell calibration controller, separate from guest clocks","start_ns":start,"stop_ns":stop,"elapsed_ns":stop-start,"includes":"serial restoration, own P0 validation, raw retention/backpressure and actual guest intervals"}));
        json!({"pilot_count":self.pilots.len(),"repetitions_per_sample":self.repetitions,
            "warmup_iterations":self.warmup_iterations,"warmup_ns":self.warmup_ns,"warmup_min_ns":self.warmup_min_ns,"warmup_wall":warmup_wall,
            "samples":samples,"reason":self.reason,"cleanup_error":self.error,
            "semantic_failure":self.failed,"completed":self.stage==Stage::Done&&self.samples.len()==self.n("accepted_samples") as usize})
    }
}
impl RepetitionController for Controller<'_> {
    fn next(&mut self) -> Option<(usize, bool)> {
        if self.stage == Stage::Done {
            return None;
        }
        if Instant::now() >= self.deadline {
            self.stop("session wall budget exhausted; coverage/sampling not reduced");
            return None;
        }
        if self.stage == Stage::Warmup
            && self.warmup_start.is_some_and(|start| {
                start.elapsed().as_nanos() >= u128::from(self.n("warmup_wall_cap_ns"))
            })
        {
            self.stop("warmup wall cap exhausted before BOTH minimum timed work and iterations");
            return None;
        }
        let rep = self.rep;
        self.rep += 1;
        Some((rep, self.stage != Stage::Basic))
    }
    fn failed(&self) -> bool {
        self.failed
    }
    fn cleanup_error(&mut self, reason: String) {
        self.failed = true;
        self.error = Some(reason);
    }
    fn retain(&mut self, mut record: Record) {
        let role = match self.stage {
            Stage::Pilot => "pilot",
            Stage::Warmup => "warmup",
            Stage::Basic => "basic",
            Stage::Done => "not_applicable",
        };
        if record.semantic_status == "not_applicable" {
            self.not_applicable = true;
            if let Err(error) = self
                .retention
                .emit(json!({"event":"row","role":"not_applicable","batch":null,"raw":record}))
            {
                self.error = Some(error);
                self.failed = true;
            }
            self.stage = Stage::Done;
            return;
        }
        if record.accepted_ns.is_some() {
            record.measurement_status =
                "validated-independent-interval; calibration gates separate".into();
        }
        let accepted = record.accepted_ns;
        let correct = record.semantic_status == "correct";
        let semantic_failure = record.semantic_status == "semantic_failure";
        let rep = record.repetition;
        let batch = if self.stage == Stage::Basic {
            Some(self.batch)
        } else {
            None
        };
        if let Err(error) = self.retained_row(record, role, batch) {
            self.error = Some(error);
            self.failed = true;
            self.stage = Stage::Done;
            return;
        }
        if !correct || accepted.is_none() {
            // Retain the rejected repetition's batch membership too. Only
            // correct intervals contribute time; a partial batch never passes.
            if self.stage == Stage::Basic {
                self.in_batch += 1;
            }
            self.failed = semantic_failure;
            self.stop("own P0/scope/evidence failure; retain raw prefix, no retry/reuse");
            return;
        }
        let ns = accepted.unwrap();
        match self.stage {
            Stage::Pilot => {
                self.pilots.push(ns);
                if self.pilots.len() == self.n("pilot_iterations") as usize {
                    let minimum = *self.pilots.iter().min().unwrap();
                    if minimum == 0 {
                        self.stop("zero pilot interval; unavailable clock/work calibration");
                        return;
                    }
                    let numerator = u128::from(self.n("minimum_sample_ns"))
                        * u128::from(self.n("repetition_safety_numerator"));
                    let denominator =
                        u128::from(minimum) * u128::from(self.n("repetition_safety_denominator"));
                    let chosen = numerator.div_ceil(denominator).max(1);
                    if chosen > u128::from(self.n("maximum_repetitions_per_sample")) {
                        self.stop("required independent repetition count exceeds bounded policy");
                        return;
                    }
                    self.repetitions = self.input_plan.unwrap_or(chosen as usize);
                    if self.repetitions == 0
                        || self.repetitions > self.n("maximum_repetitions_per_sample") as usize
                    {
                        self.stop("invalid frozen repetition plan");
                        return;
                    }
                    if let Err(error) = self.retention.flush() {
                        self.cleanup_error(error);
                        self.stage = Stage::Done;
                        return;
                    }
                    self.stage = Stage::Warmup;
                    self.warmup_start = Some(Instant::now());
                    self.warmup_wall_start = Some(self.created.elapsed().as_nanos());
                }
            }
            Stage::Warmup => {
                self.warmup_ns += u128::from(ns);
                self.warmup_min_ns = Some(self.warmup_min_ns.map_or(ns, |old| old.min(ns)));
                self.warmup_iterations += 1;
                self.warmup_wall_stop = Some(self.created.elapsed().as_nanos());
                if self.warmup_start.unwrap().elapsed().as_nanos()
                    > u128::from(self.n("warmup_wall_cap_ns"))
                {
                    self.stop("warmup wall cap exhausted; no implicit extension");
                    return;
                }
                if self.warmup_ns >= u128::from(self.n("minimum_warmup_ns"))
                    && self.warmup_iterations >= self.n("minimum_warmup_iterations") as usize
                {
                    if let Err(error) = self.retention.flush() {
                        self.cleanup_error(error);
                        self.stop("warmup raw retention failed; no basic interval");
                        return;
                    }
                    self.warmup_wall_stop = Some(self.created.elapsed().as_nanos());
                    if self.warmup_start.unwrap().elapsed().as_nanos()
                        > u128::from(self.n("warmup_wall_cap_ns"))
                    {
                        self.stop("warmup raw retention exceeded wall cap; no basic interval");
                        return;
                    }
                    let minimum = self.warmup_min_ns.unwrap();
                    if minimum == 0 {
                        self.stop("zero validated warmup interval; unavailable calibration");
                        return;
                    }
                    let chosen = (u128::from(self.n("minimum_sample_ns"))
                        * u128::from(self.n("repetition_safety_numerator")))
                    .div_ceil(
                        u128::from(minimum) * u128::from(self.n("repetition_safety_denominator")),
                    )
                    .max(1);
                    if chosen > u128::from(self.n("maximum_repetitions_per_sample")) {
                        self.stop("warmed required repetition count exceeds bounded policy");
                        return;
                    }
                    self.repetitions = self.input_plan.unwrap_or(chosen as usize);
                    self.stage = Stage::Basic;
                }
            }
            Stage::Basic => {
                self.batch_ns += u128::from(ns);
                self.in_batch += 1;
                if self.in_batch == self.repetitions {
                    let sufficient = self.batch_ns >= u128::from(self.n("minimum_sample_ns"));
                    self.samples.push(json!({"batch":self.batch,"status":if sufficient {"measured"}else{"insufficient-work"},
                        "repetitions":self.repetitions,"sum_ns":self.batch_ns,"first_repetition":rep+1-self.repetitions,"last_repetition":rep,
                        "reason":if sufficient {None}else{Some("actual summed independent intervals below 10ms; no setup time added")}}));
                    self.batch += 1;
                    self.in_batch = 0;
                    self.batch_ns = 0;
                    if self.batch == self.n("accepted_samples") as usize {
                        self.stage = Stage::Done;
                    }
                }
            }
            Stage::Done => {}
        }
    }
}
/// Output is framed JSONL, retained by the bounded-memory parent. Serialization,
/// backpressure, restoration and validation remain outside every guest interval
/// but are INCLUDED in the session wall budget.
pub fn session(paths: &Paths, budget: Duration, plan: Option<&Value>) -> Result<i32, String> {
    let policy: Value =
        serde_json::from_str(include_str!("calibrated-v1.json")).map_err(|e| e.to_string())?;
    let m = calibration_manifest();
    let mappings: Value = serde_json::from_str(include_str!("calibration-workloads-v1.json"))
        .map_err(|e| e.to_string())?;
    let deadline = Instant::now() + budget;
    let before = affinity::caller();
    let mut retention = Retention::new();
    retention.emit(
        json!({"event":"session_start","pid":std::process::id(),"caller":before,"policy":policy,"retention_ack":std::env::var("RISCV_PERF_STREAM_ACK").as_deref()==Ok("1")}),
    )?;
    let mut clock = HostClock::default();
    let mut failed = false;
    let mut complete = true;
    let mut plans = serde_json::Map::new();
    for mapping in mappings["mapping"].as_array().ok_or("variant mapping")? {
        for route in m.route_matrix.keys() {
            for phase in ["load_only", "execute_only", "end_to_end"] {
                let id = mapping[if phase == "load_only" {
                    "load"
                } else {
                    "execution"
                }]
                .as_str()
                .ok_or("variant identity")?;
                let f = m
                    .fixtures
                    .iter()
                    .find(|f| f.id == id)
                    .ok_or("mapped fixture missing")?;
                for mode in if phase == "load_only" {
                    vec!["none"]
                } else {
                    vec!["off", "facts", "file"]
                } {
                    let key = format!("{}/{route}/{phase}/{mode}", f.id);
                    let applicable = availability(&m, f, route, phase, mode).is_ok();
                    retention
                        .emit(json!({"event":"cell_start","cell":key,"applicable":applicable}))?;
                    let input_plan = plan.and_then(|p| p[&key].as_u64()).map(|n| n as usize);
                    let mut c = Controller::new(&policy, &mut retention, deadline, input_plan);
                    if failed && applicable {
                        c.stop("prior semantic/reporting failure; successors unstarted, no quarantine bypass");
                    }
                    if applicable && c.next().is_none() { /* next advances only when it returns Some; compensate below */
                    } else {
                        c.rep = 0;
                        measure_stream(
                            &m,
                            f,
                            Cell { route, phase, mode },
                            paths,
                            &mut clock,
                            &mut c,
                        );
                    }
                    let summary = c.summary();
                    if applicable {
                        failed |= c.failed || c.error.is_some();
                        complete &= summary["completed"] == true;
                        plans.insert(key.clone(), json!(c.repetitions));
                    }
                    retention.emit(json!({"event":"cell_end","cell":key,"summary":summary}))?;
                }
            }
        }
    }
    retention.emit(
        json!({"event":"session_end","pid":std::process::id(),"caller":affinity::caller(),"plan":plans,"semantic_failure":failed,"complete":complete}),
    )?;
    Ok(if failed {
        1
    } else if !complete {
        2
    } else {
        0
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn policy() -> Value {
        serde_json::from_str(include_str!("calibrated-v1.json")).unwrap()
    }
    fn retain(c: &mut Controller<'_>, ns: Option<u64>, status: &str) {
        let (rep, warmup) = c.next().expect("planned repetition");
        let f = &manifest().fixtures[0];
        let mut r = Record::new(f, "flat", "execute_only", "off", rep, warmup);
        r.semantic_status = status.into();
        r.accepted_ns = ns;
        c.retain(r);
    }
    fn warmed<'a>(
        p: &'a Value,
        retention: &'a mut Retention,
        plan: Option<usize>,
    ) -> Controller<'a> {
        let mut c = Controller::new(p, retention, Instant::now() + Duration::from_secs(60), plan);
        for _ in 0..5 {
            retain(&mut c, Some(1_000_000), "correct");
        }
        assert!(c.stage == Stage::Warmup);
        assert_eq!(c.repetitions, plan.unwrap_or(15));
        for _ in 0..4 {
            retain(&mut c, Some(250_000_000), "correct");
        }
        assert!(
            c.stage == Stage::Warmup,
            "one second alone cannot bless warmup"
        );
        retain(&mut c, Some(250_000_000), "correct");
        assert!(c.stage == Stage::Basic);
        c
    }
    #[test]
    fn both_warmup_gates_and_frozen_work_plan() {
        let p = policy();
        let mut retention = Retention::new();
        let c = warmed(&p, &mut retention, None);
        assert_eq!(c.repetitions, 1, "derive work from warmed own intervals");
        let mut c = warmed(&p, &mut retention, Some(2));
        assert_eq!(c.repetitions, 2, "independent sessions keep base work plan");
        retain(&mut c, Some(6_000_000), "correct");
        assert!(c.samples.is_empty());
        retain(&mut c, Some(6_000_000), "correct");
        assert_eq!(c.samples[0]["sum_ns"], 12_000_000);
        assert_eq!(c.samples[0]["repetitions"], 2);
        assert_eq!(c.samples[0]["status"], "measured");
    }
    #[test]
    fn failed_batch_keeps_last_repetition_and_failure_precedence() {
        let p = policy();
        for status in ["semantic_failure", "unavailable", "correct"] {
            let mut retention = Retention::new();
            let mut c = warmed(&p, &mut retention, Some(2));
            retain(&mut c, Some(6_000_000), "correct");
            retain(&mut c, None, status);
            assert!(c.next().is_none(), "never retry rejected work");
            assert_eq!(c.failed, status == "semantic_failure");
            let s = c.summary();
            assert_eq!(s["samples"][0]["status"], "partial");
            assert_eq!(s["samples"][0]["repetitions"], 2);
            assert_eq!(s["samples"][0]["sum_ns"], 6_000_000);
            assert_eq!(s["samples"][0]["last_repetition"], c.rep - 1);
            assert_eq!(s["samples"][1]["status"], "unstarted");
            assert_eq!(s["samples"].as_array().unwrap().len(), 30);
        }
    }
    #[test]
    fn caps_stop_without_fabricated_samples() {
        let p = policy();
        let mut retention = Retention::new();
        let mut c = Controller::new(&p, &mut retention, Instant::now(), None);
        assert!(c.next().is_none());
        assert_eq!(c.rep, 0);
        assert_eq!(c.summary()["samples"][0]["status"], "unstarted");
        let mut c = warmed(&p, &mut retention, None);
        c.stage = Stage::Warmup;
        c.warmup_start = Some(Instant::now() - Duration::from_secs(21));
        assert!(c.next().is_none());
        assert!(c.reason.unwrap().contains("cap"));
    }
}
