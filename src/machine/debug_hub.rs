//! Machine-owned Debug Module and the queue of DMI transactions.
//!
//! The hart owner applies a transaction at a Hart boundary. A call that
//! arrives while an instruction is in progress stays queued until that
//! boundary. `ndmreset` is a bit on the module; the owner reloads the hart.

use super::MachineError;
use crate::dm::DebugModule;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::{Condvar, Mutex};

pub(crate) struct DmiCall {
    pub addr: u8,
    pub write: Option<u32>,
    pub reply: Sender<Result<u32, MachineError>>,
}

pub(crate) struct DebugHub {
    pub dm: Mutex<DebugModule>,
    queue: Mutex<VecDeque<DmiCall>>,
    wake: Condvar,
    pub(crate) in_instruction: AtomicBool,
    attached: AtomicBool,
    closed: AtomicBool,
    porters: AtomicUsize,
    completed_turns: AtomicU64,
}

impl DebugHub {
    pub(crate) fn new() -> Self {
        Self {
            dm: Mutex::new(DebugModule::new()),
            queue: Mutex::new(VecDeque::new()),
            wake: Condvar::new(),
            in_instruction: AtomicBool::new(false),
            attached: AtomicBool::new(false),
            closed: AtomicBool::new(false),
            porters: AtomicUsize::new(0),
            completed_turns: AtomicU64::new(0),
        }
    }

    pub(crate) fn attach(&self) {
        self.attached.store(true, Ordering::Release);
        self.acquire_port();
    }

    pub(crate) fn attached(&self) -> bool {
        self.attached.load(Ordering::Acquire)
    }

    pub(crate) fn acquire_port(&self) {
        self.porters.fetch_add(1, Ordering::AcqRel);
    }

    /// Returns true when the last debugger port has been dropped.
    pub(crate) fn release_port(&self) -> bool {
        self.porters.fetch_sub(1, Ordering::AcqRel) == 1
    }

    #[cfg(test)]
    pub(crate) fn set_in_instruction(&self, active: bool) {
        self.in_instruction.store(active, Ordering::Release);
    }

    pub(crate) fn in_instruction(&self) -> bool {
        self.in_instruction.load(Ordering::Acquire)
    }

    pub(crate) fn open_session(&self) {
        self.closed.store(false, Ordering::Release);
    }

    pub(crate) fn close(&self) {
        let mut queue = match self.queue.lock() {
            Ok(queue) => queue,
            Err(poisoned) => poisoned.into_inner(),
        };
        self.closed.store(true, Ordering::Release);
        for call in queue.drain(..) {
            let _ = call.reply.send(Err(MachineError::Debug(
                "debug run ended before the transaction was admitted".into(),
            )));
        }
        drop(queue);
        self.wake.notify_all();
    }

    pub(crate) fn closed(&self) -> bool {
        self.closed.load(Ordering::Acquire)
    }

    pub(crate) fn publish_turns(&self, turns: u64) {
        self.completed_turns.store(turns, Ordering::Release);
    }

    pub(crate) fn completed_turns(&self) -> u64 {
        self.completed_turns.load(Ordering::Acquire)
    }

    pub(crate) fn post(&self, addr: u8, write: Option<u32>) -> Result<u32, MachineError> {
        let (reply, wait) = mpsc::channel();
        {
            let mut queue = self.queue.lock().map_err(|_| MachineError::Poisoned)?;
            if self.closed() {
                return Err(MachineError::Debug(
                    "debug run ended before the transaction was admitted".into(),
                ));
            }
            queue.push_back(DmiCall { addr, write, reply });
        }
        self.wake.notify_one();
        wait.recv().map_err(|_| {
            MachineError::Debug("debug run ended before the transaction was admitted".into())
        })?
    }

    pub(crate) fn wake(&self) -> &Condvar {
        &self.wake
    }

    pub(crate) fn queue(&self) -> &Mutex<VecDeque<DmiCall>> {
        &self.queue
    }

    #[cfg(test)]
    pub(crate) fn queue_len(&self) -> usize {
        self.queue.lock().map(|queue| queue.len()).unwrap_or(0)
    }
}

/// Clears the in-instruction flag when the turn returns or unwinds.
pub(crate) struct InInstruction<'a> {
    flag: &'a AtomicBool,
}

impl<'a> InInstruction<'a> {
    pub(crate) fn enter(flag: &'a AtomicBool) -> Self {
        flag.store(true, Ordering::Release);
        Self { flag }
    }
}

impl Drop for InInstruction<'_> {
    fn drop(&mut self) {
        self.flag.store(false, Ordering::Release);
    }
}
