//! C ABI hosting facade over one Machine (dev-plan D8).
//!
//! The `capi` feature adds this module and the hand-written header
//! `include/ruscv_sim.h`; it adds no crate type and no C/C++ build dependency,
//! so `cargo build`/`test --all-features` needs no C++ toolchain.  The static
//! library is produced explicitly:
//!
//! ```text
//! cargo rustc --lib --features capi --crate-type staticlib
//! ```
//!
//! The facade is a Runner-side hosting adapter: an external kernel owns global
//! time and the outer thread, grants bounded budgets here, and classifies the
//! returned facts itself.  Nothing in this module can make a transport status,
//! a callback return or a host stop become a ruscv-sim terminal result
//! (dev-plan D8/D9, ADR-0003 2026-11-04 amendment).
//!
//! Rules this module owns:
//!
//! * **Creator-thread affinity.** A handle records the thread that created it;
//!   every call from another thread returns [`Status::NotCreatorThread`] and
//!   touches no state.
//! * **Synchronous, non-reentrant callbacks.** A callback runs only inside a
//!   ruscv-sim call on that thread.  Any call back into the ABI from inside a
//!   callback is rejected with [`Status::Reentrant`] before it can observe or
//!   mutate the handle.  The one exception is
//!   [`ruscv_machine_invalidate_direct_mem`], which a target may call from a
//!   callback because it touches only the direct-memory registry and no run
//!   state (direct-memory D12).
//! * **Panic poisoning.** A Rust panic is caught at the boundary, the handle is
//!   poisoned, and the call returns [`Status::Panic`]; no Rust panic ever
//!   unwinds into C.  Every later call returns [`Status::Poisoned`].
//! * **Integer status codes.** Every fallible entry point returns a
//!   [`Status`] code; no Rust or C++ exception crosses the boundary.
//! * **Rust-owned payload buffers.** The target callbacks receive a pointer into
//!   a Rust-owned buffer that is valid only for the duration of that one
//!   callback (see [`target`]).
//! * **Composition is fixed before load.** A device table attached with
//!   [`ruscv_machine_attach_device`] is copied, chosen once per handle and
//!   answered by the TLM route's device path.  Attaching later, twice for one
//!   window, or on a handle created without a `ruscv_target` is a configuration
//!   error, so no installed generation can have its windows rewired under it
//!   (device D8).

use crate::blocking::{BlockingDirectMemRegistry, DeviceTargets, Transport};
use crate::elf::LoadImage;
use crate::machine::{IsaProfile, MachineConfig};
use host::{GrantFacts, Session};
use std::ffi::{c_char, CStr};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::ThreadId;
use target::{CBlockingTarget, CDevice, CDeviceTarget, CTarget};

mod host;
pub mod target;
#[cfg(test)]
#[path = "tests.rs"]
mod tests;

/// One C ABI status code (the header's `ruscv_status`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum Status {
    /// The call completed.
    Ok = 0,
    /// A pointer, profile code, period or grant field was unusable.
    InvalidArgument = 1,
    /// The call came from a thread other than the creating thread.
    NotCreatorThread = 2,
    /// The call came from inside a target callback.
    Reentrant = 3,
    /// The handle is poisoned by an earlier panic.
    Poisoned = 4,
    /// A Rust panic was caught at the boundary; the handle is now poisoned.
    Panic = 5,
    /// A file could not be read, created or written.
    Io = 6,
    /// The ELF could not be parsed or installed.
    Elf = 7,
    /// The declared configuration cannot serve a Machine.
    Config = 8,
    /// The grant's time authority is invalid (earlier start, reversed horizon).
    TimeAuthority = 9,
    /// The Machine refused the operation (busy, failed, torn down, drained).
    Machine = 10,
    /// No image is installed.
    NoImage = 11,
    /// The transport has an unknown completion; only reset resolves it.
    UnknownCompletion = 12,
}

/// One API outcome with an optional diagnostic message.
#[derive(Debug)]
pub(crate) struct ApiError {
    status: Status,
    message: Option<String>,
}

impl ApiError {
    pub(crate) const fn status(status: Status) -> Self {
        Self {
            status,
            message: None,
        }
    }

    pub(crate) fn new(status: Status, message: impl Into<String>) -> Self {
        Self {
            status,
            message: Some(message.into()),
        }
    }

    pub(crate) const fn code(&self) -> i32 {
        self.status as i32
    }

    pub(crate) fn message(&self) -> Option<&str> {
        self.message.as_deref()
    }
}

impl From<Status> for ApiError {
    fn from(status: Status) -> Self {
        Self::status(status)
    }
}

/// The C-visible boundary and grant facts (`ruscv_boundary`).
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct CBoundary {
    /// The started-slot budget ended the grant.
    pub budget_exhausted: i32,
    /// The cursor reached the inclusive grant deadline.
    pub deadline_reached: i32,
    /// No Runnable turn's conservative completion bound fit the slack.
    pub deadline_blocked: i32,
    /// The Hart is waiting and this grant retired nothing.
    pub waiting: i32,
    /// A wait has no future Platform event.
    pub no_progress: i32,
    /// An unknown completion left the run non-resumable.
    pub uncertain: i32,
    /// A simulator or host failure ended the run.
    pub failed: i32,
    /// Machine lifecycle acknowledgment (`ruscv_lifecycle`).
    pub lifecycle: i32,
    /// Remaining slack at a reached or blocked deadline, in femtoseconds.
    pub residual_slack_fs: u64,
    /// The refused turn's conservative completion bound, in femtoseconds.
    pub candidate_bound_fs: u64,
    /// Total consumed blocking-transport delay so far, in femtoseconds.
    pub transport_delay_fs: u64,
    /// Completed Hart turns since the current image was installed.
    pub turns: u64,
    /// The selected quantum in Platform ticks, zero when off (quantum D12).
    pub quantum_ticks: u64,
    /// The declared interrupt-latency bound in ticks (quantum D8/D10).
    pub interrupt_latency_bound_ticks: u64,
    /// Cumulative counted syncs, read after the grant-return sync (D10/D12).
    pub quantum_syncs: u64,
    /// quantum D5's threshold result at the grant's last boundary.
    pub quantum_reached: i32,
}

/// One grant result (`ruscv_grant`).
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct CGrant {
    /// Platform time this grant consumed, in femtoseconds.
    pub consumed_fs: u64,
    /// The Machine's committed time at grant entry, in femtoseconds.
    pub committed_start_fs: u64,
    /// The Machine's committed time after the grant, in femtoseconds.
    pub committed_end_fs: u64,
    /// Nonzero when a grant bound ended the grant.
    pub boundary_reached: i32,
    /// The grant facts.
    pub facts: CBoundary,
    /// nb D5: the host time that passed inside this grant's transactions, in
    /// exact femtoseconds.
    ///
    /// It is the increase of the generation's host elapsed total from the
    /// grant's start to its return, never the cumulative value, and it is set
    /// for every grant, zero for one that ran no turn.  A host waits
    /// `consumed_fs - host_elapsed_fs` and keeps its clock at `committed_end_fs`.
    pub host_elapsed_fs: u64,
}

/// The run classification (`ruscv_result`).
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct CResult {
    /// `ruscv_run_state`: running, exited or failed.
    pub state: i32,
    /// The guest exit code, valid for `RUSCV_RUN_EXITED`.
    pub exit_code: u32,
    /// Completed Hart turns since the current image was installed.
    pub turns: u64,
    /// The program counter at the last boundary.
    pub pc: u64,
}

thread_local! {
    /// True while a target callback runs on this thread.
    ///
    /// dev-plan D8 rejects any call back into the ABI from a callback.  The
    /// check is thread-scoped so a callback cannot enter a *different* handle
    /// while it holds one, and it is independent of the per-handle call gate.
    static IN_CALLBACK: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Marks the current thread as running one target callback.
pub(crate) struct CallbackScope;

impl CallbackScope {
    pub(crate) fn enter() -> Self {
        IN_CALLBACK.with(|flag| flag.set(true));
        Self
    }
}

impl Drop for CallbackScope {
    fn drop(&mut self) {
        IN_CALLBACK.with(|flag| flag.set(false));
    }
}

/// True when the current call came from inside a target callback.
pub(crate) fn in_callback() -> bool {
    IN_CALLBACK.with(|flag| flag.get())
}

/// The creator-thread and re-entry gate of one handle.
struct CallGate {
    creator: ThreadId,
    /// A call is in progress, which means every nested call is a re-entry.
    active: AtomicBool,
}

/// Proof that one ABI call owns the handle until it returns.
///
/// Dropping it clears the flag on every path, including an unwinding panic, so
/// a poisoned handle cannot keep reporting re-entry.
struct ActiveCall<'a>(&'a CallGate);

impl CallGate {
    fn enter(&self) -> Option<ActiveCall<'_>> {
        if self.active.swap(true, Ordering::SeqCst) {
            return None;
        }
        Some(ActiveCall(self))
    }
}

impl Drop for ActiveCall<'_> {
    fn drop(&mut self) {
        self.0.active.store(false, Ordering::SeqCst);
    }
}

/// The mutable state of one handle.
struct HandleState {
    poisoned: bool,
    config: MachineConfig,
    session: Option<Session>,
    /// The most recent API-level diagnostic, when no session produced one.
    diagnostic: Option<String>,
}

/// One opaque C-ABI handle (the header's `ruscv_machine`).
pub struct RuscvMachine {
    gate: CallGate,
    /// The direct-memory registry owned by the C target wrapper, when a C target
    /// is installed.
    ///
    /// It is created with the wrapper in [`ruscv_machine_create`], lives outside
    /// the run-state lock and persists across Machine replacement on reset, so
    /// [`ruscv_machine_invalidate_direct_mem`] can be accepted inside a callback
    /// and on a poisoned handle (direct-memory D12).  A native route has no
    /// wrapper, so there is nothing to invalidate.
    direct_mem: Option<Arc<BlockingDirectMemRegistry>>,
    state: Mutex<HandleState>,
}

impl RuscvMachine {
    /// Locks the handle state, never blocking.
    ///
    /// A blocking acquisition would turn a missed re-entry guard into a
    /// deadlock, so an already-held lock is reported as re-entry.  A panic
    /// while serving a call poisons the mutex; the handle is marked poisoned
    /// then, so recovering the guard cannot expose state a caller may use.
    fn lock(&self) -> Result<MutexGuard<'_, HandleState>, ApiError> {
        match self.state.try_lock() {
            Ok(state) => Ok(state),
            Err(std::sync::TryLockError::Poisoned(poisoned)) => Ok(poisoned.into_inner()),
            Err(std::sync::TryLockError::WouldBlock) => Err(ApiError::new(
                Status::Reentrant,
                "the handle is already in use by the current call",
            )),
        }
    }
}

/// Runs one ABI call against the handle state.
///
/// Enforces creator-thread affinity, re-entry rejection, poisoning and panic
/// containment in that order, so no body can run twice for one handle and no
/// panic can cross the boundary.
unsafe fn with_handle<T>(
    handle: *mut RuscvMachine,
    body: impl FnOnce(&mut HandleState) -> Result<T, ApiError>,
) -> Result<T, ApiError> {
    let Some(machine) = handle.as_ref() else {
        return Err(ApiError::new(
            Status::InvalidArgument,
            "the machine handle is null",
        ));
    };
    if in_callback() {
        return Err(ApiError::new(
            Status::Reentrant,
            "the C ABI is inside a target callback; a callback cannot re-enter it",
        ));
    }
    if std::thread::current().id() != machine.gate.creator {
        return Err(ApiError::new(
            Status::NotCreatorThread,
            "the C ABI accepts calls only on the creating thread",
        ));
    }
    let Some(_active) = machine.gate.enter() else {
        return Err(ApiError::new(
            Status::Reentrant,
            "the C ABI is already inside a call; a target callback cannot re-enter it",
        ));
    };
    let outcome = {
        let mut state = machine.lock()?;
        if state.poisoned {
            Err(ApiError::new(
                Status::Poisoned,
                "the handle is poisoned by an earlier panic",
            ))
        } else {
            catch_unwind(AssertUnwindSafe(|| body(&mut state))).unwrap_or_else(|_| {
                // The body panicked.  The catch is the whole point of the
                // boundary: mark the handle unusable and report it as a status.
                Err(ApiError::new(
                    Status::Panic,
                    "a Rust panic was caught at the C ABI boundary",
                ))
            })
        }
    };
    if outcome
        .as_ref()
        .is_err_and(|error| error.status == Status::Panic)
    {
        if let Ok(mut state) = machine.lock() {
            state.poisoned = true;
        }
    }
    if let Err(error) = &outcome {
        let Ok(mut state) = machine.lock() else {
            return outcome;
        };
        if let Some(message) = error.message() {
            if let Some(session) = state.session.as_ref() {
                if session.diagnostic().is_none() {
                    state.diagnostic = Some(message.to_owned());
                }
            } else {
                state.diagnostic = Some(message.to_owned());
            }
        }
    }
    outcome
}

/// Runs one ABI call that needs an installed image.
unsafe fn with_session<T>(
    handle: *mut RuscvMachine,
    body: impl FnOnce(&mut Session) -> Result<T, ApiError>,
) -> Result<T, ApiError> {
    with_handle(handle, |state| {
        let session = state
            .session
            .as_mut()
            .ok_or_else(|| ApiError::new(Status::NoImage, "no ELF image is installed"))?;
        body(session)
    })
}

/// Runs one ABI call and reports it as an integer status code.
unsafe fn call(
    handle: *mut RuscvMachine,
    body: impl FnOnce(&mut Session) -> Result<(), ApiError>,
) -> i32 {
    match with_session(handle, body) {
        Ok(()) => Status::Ok as i32,
        Err(error) => error.code(),
    }
}

/// Copies one C string into a path.
unsafe fn c_path(path: *const c_char) -> Result<&'static Path, ApiError> {
    if path.is_null() {
        return Err(ApiError::new(
            Status::InvalidArgument,
            "the path pointer is null",
        ));
    }
    // SAFETY: the caller guarantees a NUL-terminated string.
    let text = unsafe { CStr::from_ptr(path) }
        .to_str()
        .map_err(|_| ApiError::new(Status::InvalidArgument, "the path is not UTF-8"))?;
    Ok(Path::new(text))
}

/// Maps one profile code to the Machine profile (the header's `ruscv_isa`).
fn isa_profile(code: i32) -> Result<IsaProfile, ApiError> {
    match code {
        0 => Ok(IsaProfile::Rv64ima),
        1 => Ok(IsaProfile::Rv64imac),
        2 => Ok(IsaProfile::Rv64imafd),
        3 => Ok(IsaProfile::Rv64imafdc),
        _ => Err(ApiError::new(
            Status::InvalidArgument,
            format!("unknown ISA profile code {code}"),
        )),
    }
}

/// Maps one Machine lifecycle acknowledgment to its C code.
fn lifecycle_code(lifecycle: crate::machine::Lifecycle) -> i32 {
    use crate::machine::Lifecycle;
    match lifecycle {
        Lifecycle::Running => 0,
        Lifecycle::QuiesceRequested => 1,
        Lifecycle::DrainComplete => 2,
        Lifecycle::TornDown => 3,
    }
}

/// The C facts of one grant.
fn boundary(facts: GrantFacts) -> CBoundary {
    CBoundary {
        budget_exhausted: i32::from(facts.budget_exhausted),
        deadline_reached: i32::from(facts.deadline_reached),
        deadline_blocked: i32::from(facts.deadline_blocked),
        waiting: i32::from(facts.waiting),
        no_progress: i32::from(facts.no_progress),
        uncertain: i32::from(facts.uncertain),
        failed: i32::from(facts.failed),
        lifecycle: lifecycle_code(facts.lifecycle),
        residual_slack_fs: facts.residual_slack_fs,
        candidate_bound_fs: facts.candidate_bound_fs,
        transport_delay_fs: facts.transport_delay_fs,
        turns: facts.turns,
        quantum_ticks: facts.quantum_ticks,
        interrupt_latency_bound_ticks: facts.interrupt_latency_bound_ticks,
        quantum_syncs: facts.quantum_syncs,
        quantum_reached: i32::from(facts.quantum_reached),
    }
}

/// Creates one handle over an optional C RAM target.
///
/// `target` may be null, which selects the native `SimpleMemory` RAM route; a
/// non-null table is copied, and its `user` pointer must stay valid until
/// [`ruscv_machine_destroy`].  `tick_period_fs` is the declared Platform tick
/// period (1 ns is the documented default).  A non-zero `quantum_ticks` in the
/// copied table selects the quantum, which a null table therefore cannot do
/// (quantum D2/D12).
///
/// # Safety
///
/// `target` must be null or point at one readable [`CTarget`] whose callbacks
/// are callable and whose `user` pointer stays valid until destroy.  `out` must
/// be null or point at one writable `*mut ruscv_machine`.
#[no_mangle]
pub unsafe extern "C" fn ruscv_machine_create(
    target: *const CTarget,
    isa: i32,
    tick_period_fs: u64,
    out: *mut *mut RuscvMachine,
) -> i32 {
    if out.is_null() {
        return Status::InvalidArgument as i32;
    }
    if in_callback() {
        return Status::Reentrant as i32;
    }
    // SAFETY: the caller guarantees `out` is writable.
    unsafe { *out = std::ptr::null_mut() };
    let built = (|| -> Result<RuscvMachine, ApiError> {
        let mut config = host::native_config();
        config.isa = isa_profile(isa)?;
        let period = host::tick_period(tick_period_fs)?;
        config.tick_period = period;
        let mut direct_mem = None;
        let mut tlm = None;
        if !target.is_null() {
            // SAFETY: the caller guarantees one readable table.
            let table = unsafe { *target };
            let built = CBlockingTarget::new(table)
                .map_err(|message| ApiError::new(Status::InvalidArgument, message))?;
            host::validate_target(&built, period)?;
            // quantum D12: the quantum is part of the copied table, so it
            // persists across a reset, and a NULL table selects the native
            // route, which cannot select one.
            config.quantum = built.quantum_ticks();
            // A non-NULL query selects direct memory; `NULL` means the target
            // offers nothing and behaves exactly as `dmi` off (direct-memory
            // D12).
            config.dmi = built.offers_direct_memory();
            direct_mem = Some(built.direct_mem_registry().clone());
            config.transport = Transport::Tlm(built.clone());
            tlm = Some(built);
        }
        // quantum D2: the Machine is built later, at load, so create applies
        // the same refusal rules here.
        host::validate_quantum(config.quantum, tlm.as_deref(), period)?;
        Ok(RuscvMachine {
            gate: CallGate {
                creator: std::thread::current().id(),
                active: AtomicBool::new(false),
            },
            direct_mem,
            state: Mutex::new(HandleState {
                poisoned: false,
                config,
                session: None,
                diagnostic: None,
            }),
        })
    })();
    match built {
        Ok(machine) => {
            // SAFETY: the handle is freshly boxed and owned by the caller now.
            unsafe { *out = Box::into_raw(Box::new(machine)) };
            Status::Ok as i32
        }
        Err(error) => error.code(),
    }
}

/// One C device window and the Platform window it names (device D8).
struct CDeviceWindow {
    base: u64,
    size: u64,
}

/// The Platform window one `ruscv_device_window` code names.
///
/// Only the UART window accepts a host target in this contract; host HTIF
/// service stays deferred, so every other code is an unknown window.
fn c_device_window(code: i32) -> Option<CDeviceWindow> {
    match code {
        target::device_window::UART => Some(CDeviceWindow {
            base: crate::executor::SYSTEM_BUS_UART_BASE,
            size: crate::executor::SYSTEM_BUS_UART_WINDOW as u64,
        }),
        _ => None,
    }
}

/// Stores one copied device table as the handle's window selection.
///
/// Every argument refusal precedes every configuration refusal: the table's
/// shape is an argument, so a NULL table, an unknown window and an incomplete
/// table are [`Status::InvalidArgument`] even on a handle that would also be
/// refused for its state.
fn attach_device(
    state: &mut HandleState,
    window: CDeviceWindow,
    device: CDevice,
) -> Result<(), ApiError> {
    let target = CDeviceTarget::new(device)
        .map_err(|message| ApiError::new(Status::InvalidArgument, message))?;
    // device D8: a device target is composition configuration, chosen after
    // create and before the first load, so no installed generation can have its
    // windows rewired under it.
    if state.session.is_some() {
        return Err(ApiError::new(
            Status::Config,
            "a device target must be attached before the first ELF image is installed",
        ));
    }
    if device.window_base != window.base || device.window_size != window.size {
        return Err(ApiError::new(
            Status::Config,
            format!(
                "the device table names the window at {:#018x} with {} bytes; the selected window \
                 is {:#018x} with {} bytes",
                device.window_base, device.window_size, window.base, window.size
            ),
        ));
    }
    // device D8: a handle created without a `ruscv_target` has no TLM route, and
    // a device window is a TLM-route target.
    if state.config.transport.is_native() {
        return Err(ApiError::new(
            Status::Config,
            "a device target needs the TLM route, and this handle was created with a NULL \
             ruscv_target",
        ));
    }
    // Only the UART window takes a host target, which is the one window this
    // version of the table can name.
    if state.config.devices.uart().is_some() {
        return Err(ApiError::new(
            Status::Config,
            "a device target is already attached to the UART window",
        ));
    }
    state.config.devices = DeviceTargets::with_uart(target);
    Ok(())
}

/// Attaches one host device target to the selected window (device D8).
///
/// `window` is `RUSCV_DEVICE_UART`.  The call must come after
/// [`ruscv_machine_create`] and before the first [`ruscv_machine_load_elf`]: a
/// device target is composition configuration, chosen once per handle and
/// copied, so it survives [`ruscv_machine_reset`] and the target is reset at
/// every Platform build.
///
/// It is refused with [`Status::InvalidArgument`] for a NULL handle, NULL table,
/// unknown window or incomplete table, and with [`Status::Config`] for a
/// `window_base` or `window_size` that is not the selected window's, a second
/// attach for the same window, an attach after the first load, or a handle
/// created with a NULL `ruscv_target`, which has no TLM route.
///
/// # Safety
///
/// `device` must be null or point at one readable [`CDevice`] whose callbacks
/// are callable and whose `user` pointer stays valid until
/// [`ruscv_machine_destroy`], and `machine` must be a handle from
/// [`ruscv_machine_create`] used on the creating thread.
#[no_mangle]
pub unsafe extern "C" fn ruscv_machine_attach_device(
    machine: *mut RuscvMachine,
    window: i32,
    device: *const CDevice,
) -> i32 {
    if device.is_null() {
        return Status::InvalidArgument as i32;
    }
    let Some(window) = c_device_window(window) else {
        return Status::InvalidArgument as i32;
    };
    // SAFETY: the caller guarantees one readable table.
    let device = unsafe { *device };
    // SAFETY: forwarded caller guarantees.
    unsafe {
        match with_handle(machine, |state| attach_device(state, window, device)) {
            Ok(()) => Status::Ok as i32,
            Err(error) => error.code(),
        }
    }
}

/// Parses and installs one ELF image, replacing any previous one.
///
/// # Safety
///
/// `machine` must be a handle from [`ruscv_machine_create`] and `path` a
/// NUL-terminated UTF-8 path on the creating thread.
#[no_mangle]
pub unsafe extern "C" fn ruscv_machine_load_elf(
    machine: *mut RuscvMachine,
    path: *const c_char,
) -> i32 {
    // SAFETY: forwarded caller guarantees.
    unsafe {
        match with_handle(machine, |state| {
            let path = c_path(path)?;
            let data = std::fs::read(path).map_err(|error| {
                ApiError::new(
                    Status::Io,
                    format!("cannot read {}: {error}", path.display()),
                )
            })?;
            let image = LoadImage::parse(&data).map_err(|error| {
                ApiError::new(Status::Elf, format!("cannot parse the ELF: {error}"))
            })?;
            let mut session = Session::new(state.config.clone(), Arc::new(image))?;
            if let Some(previous) = state.session.as_mut() {
                session.adopt_logger(previous.take_logger());
            }
            state.session = Some(session);
            state.diagnostic = None;
            Ok(())
        }) {
            Ok(()) => Status::Ok as i32,
            Err(error) => error.code(),
        }
    }
}

/// Routes the commit log to `path` for every later grant.
///
/// # Safety
///
/// `machine` must be a valid handle and `path` a NUL-terminated UTF-8 path.
#[no_mangle]
pub unsafe extern "C" fn ruscv_machine_set_commit_log(
    machine: *mut RuscvMachine,
    path: *const c_char,
) -> i32 {
    // SAFETY: forwarded caller guarantees.
    unsafe {
        call(machine, |session| {
            let path = c_path(path)?;
            session.set_commit_log(path)
        })
    }
}

/// Runs one bounded grant and writes its result to `out`.
///
/// `start_fs` is the caller's authoritative start time, `max_end_fs` its
/// inclusive horizon and `turn_budget` the started-slot budget
/// (`RUSCV_TURNS_UNBOUNDED` for none).  A classified run failure returns
/// [`Status::Ok`] and is visible through [`ruscv_machine_result`]; an invalid
/// grant returns its own status.
///
/// # Safety
///
/// `machine` must be a valid handle and `out` null or writable.
#[no_mangle]
pub unsafe extern "C" fn ruscv_machine_run_grant(
    machine: *mut RuscvMachine,
    start_fs: u64,
    max_end_fs: u64,
    turn_budget: u64,
    out: *mut CGrant,
) -> i32 {
    if out.is_null() {
        return Status::InvalidArgument as i32;
    }
    // SAFETY: the caller guarantees `out` is writable.
    unsafe { *out = CGrant::default() };
    // SAFETY: forwarded caller guarantees.
    unsafe {
        match with_session(machine, |session| {
            session.run_grant(start_fs, max_end_fs, turn_budget)?;
            let facts = session.facts();
            Ok(CGrant {
                consumed_fs: facts.consumed_fs,
                committed_start_fs: facts.committed_start_fs,
                committed_end_fs: facts.committed_end_fs,
                boundary_reached: i32::from(facts.boundary_reached()),
                facts: boundary(facts),
                host_elapsed_fs: facts.host_elapsed_fs,
            })
        }) {
            Ok(grant) => {
                *out = grant;
                Status::Ok as i32
            }
            Err(error) => error.code(),
        }
    }
}

/// Reads the facts of the most recent grant.
///
/// # Safety
///
/// `machine` must be a valid handle and `out` null or writable.
#[no_mangle]
pub unsafe extern "C" fn ruscv_machine_boundary(
    machine: *mut RuscvMachine,
    out: *mut CBoundary,
) -> i32 {
    if out.is_null() {
        return Status::InvalidArgument as i32;
    }
    // SAFETY: the caller guarantees `out` is writable.
    unsafe { *out = CBoundary::default() };
    // SAFETY: forwarded caller guarantees.
    unsafe {
        match with_session(machine, |session| Ok(boundary(session.facts()))) {
            Ok(facts) => {
                *out = facts;
                Status::Ok as i32
            }
            Err(error) => error.code(),
        }
    }
}

/// Reads the run's classification.
///
/// # Safety
///
/// `machine` must be a valid handle and `out` null or writable.
#[no_mangle]
pub unsafe extern "C" fn ruscv_machine_result(
    machine: *mut RuscvMachine,
    out: *mut CResult,
) -> i32 {
    if out.is_null() {
        return Status::InvalidArgument as i32;
    }
    // SAFETY: the caller guarantees `out` is writable.
    unsafe { *out = CResult::default() };
    // SAFETY: forwarded caller guarantees.
    unsafe {
        match with_session(machine, |session| {
            Ok(CResult {
                state: session.state().code(),
                exit_code: session.exit_code(),
                turns: session.facts().turns,
                pc: session.pc(),
            })
        }) {
            Ok(result) => {
                *out = result;
                Status::Ok as i32
            }
            Err(error) => error.code(),
        }
    }
}

/// Reinstalls the image on a fresh, acknowledged transport domain.
///
/// This is the recovery path after an unknown completion: the new composition
/// obtains the target's transport-reset acknowledgement and zero-fills the RAM
/// window before any new grant (dev-plan D6).
///
/// # Safety
///
/// `machine` must be a valid handle used on the creating thread.
#[no_mangle]
pub unsafe extern "C" fn ruscv_machine_reset(machine: *mut RuscvMachine) -> i32 {
    // SAFETY: forwarded caller guarantee.
    unsafe { call(machine, Session::reset) }
}

/// Invalidates every cached direct-memory grant that overlaps the inclusive
/// physical range `start..=end` (direct-memory D8/D12).
///
/// This is the C ABI's one deliberate exception to re-entry rejection and to
/// poison rejection.  It is accepted inside a target callback and on a handle
/// poisoned by an earlier callback panic, because it touches only the
/// [`BlockingDirectMemRegistry`] owned by the C target wrapper and no run state.
/// Every other entry point keeps rejecting a callback and a poisoned handle.
///
/// The registry is created with the target wrapper in [`ruscv_machine_create`]
/// and persists across Machine replacement on reset, so an invalidation cannot
/// deadlock on the run-state lock (which a synchronous callback already holds)
/// and cannot fail because that lock is poisoned.  An inverted range is treated
/// as an invalidation of the whole address space: keeping a grant through a
/// malformed request could outlive the storage behind it.  A handle with no C
/// target, or a target with a `NULL` query, has no grant to remove, so the call
/// succeeds.
///
/// `invalidate` is safe inside `b_transport`: the adapter's direct-memory lock
/// is never held across a target call, and a returning invalidation proves no
/// copy through a removed pointer is still in flight (direct-memory D8).
///
/// # Safety
///
/// `machine` must be a valid handle used on the creating thread.
#[no_mangle]
pub unsafe extern "C" fn ruscv_machine_invalidate_direct_mem(
    machine: *mut RuscvMachine,
    start: u64,
    end: u64,
) -> i32 {
    // Deliberately no callback or `state` lock check here: this entry is the
    // documented exception that keeps it usable inside a callback and on a
    // poisoned handle (direct-memory D12).
    let Some(handle) = (unsafe { machine.as_ref() }) else {
        return Status::InvalidArgument as i32;
    };
    if std::thread::current().id() != handle.gate.creator {
        return Status::NotCreatorThread as i32;
    }
    if let Some(registry) = &handle.direct_mem {
        registry.invalidate(start, end);
    }
    Status::Ok as i32
}

/// Copies the most recent diagnostic message into `buffer`, NUL-terminated and
/// truncated to fit.  Returns the message length before truncation.
///
/// # Safety
///
/// `buffer` must be null or point at `length` writable bytes.
#[no_mangle]
pub unsafe extern "C" fn ruscv_machine_diagnostic(
    machine: *mut RuscvMachine,
    buffer: *mut c_char,
    length: usize,
) -> i32 {
    // SAFETY: forwarded caller guarantee.
    unsafe {
        match with_handle(machine, |state| {
            if buffer.is_null() && length != 0 {
                return Err(ApiError::new(
                    Status::InvalidArgument,
                    "the diagnostic buffer is null",
                ));
            }
            let message = state
                .session
                .as_ref()
                .and_then(Session::diagnostic)
                .or(state.diagnostic.as_deref())
                .unwrap_or("");
            if !buffer.is_null() && length != 0 {
                let bytes = message.as_bytes();
                let copy = bytes.len().min(length - 1);
                // SAFETY: the caller guarantees `length` writable bytes.
                let target = std::slice::from_raw_parts_mut(buffer.cast::<u8>(), length);
                target[..copy].copy_from_slice(&bytes[..copy]);
                target[copy] = 0;
            }
            Ok(message.len().min(i32::MAX as usize) as i32)
        }) {
            Ok(written) => written,
            Err(error) => error.code(),
        }
    }
}

/// Provokes one Rust panic inside the ABI.
///
/// This is a self-test entry point: it proves that the boundary catches a Rust
/// panic and poisons the handle instead of unwinding into the caller.  It is
/// exported in every `capi` build and is not part of the hosting interface;
/// call it only on a throwaway handle.
///
/// # Safety
///
/// `machine` must be a valid handle used on the creating thread.
#[no_mangle]
pub unsafe extern "C" fn ruscv_machine_self_test_panic(machine: *mut RuscvMachine) -> i32 {
    // SAFETY: forwarded caller guarantee.
    unsafe {
        let outcome = with_handle(machine, |_| -> Result<(), ApiError> {
            panic!("ruscv_sim C ABI panic self-test")
        });
        outcome
            .err()
            .map_or(Status::Ok as i32, |error| error.code())
    }
}

/// Destroys one handle and releases its image and Machine.
///
/// Returns [`Status::NotCreatorThread`] without destroying anything when it is
/// called from another thread, and [`Status::Reentrant`] when it is called from
/// inside a callback.
///
/// # Safety
///
/// `machine` must be null or a handle from [`ruscv_machine_create`] that has not
/// been destroyed.
#[no_mangle]
pub unsafe extern "C" fn ruscv_machine_destroy(machine: *mut RuscvMachine) -> i32 {
    if machine.is_null() {
        return Status::Ok as i32;
    }
    if in_callback() {
        return Status::Reentrant as i32;
    }
    // SAFETY: the caller guarantees one live handle.
    let handle = unsafe { &*machine };
    if std::thread::current().id() != handle.gate.creator {
        return Status::NotCreatorThread as i32;
    }
    if handle.gate.active.load(Ordering::SeqCst) {
        return Status::Reentrant as i32;
    }
    // SAFETY: the pointer came from `Box::into_raw` in `ruscv_machine_create`
    // and ownership is returned exactly once, because every call is either
    // serialized on the creating thread or rejected above.
    drop(unsafe { Box::from_raw(machine) });
    Status::Ok as i32
}

/// The unbounded grant budget the header documents.
pub const UNBOUNDED_TURNS: u64 = u64::MAX;
