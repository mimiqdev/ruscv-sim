//! Public memory-capability poison is a host failure on the SAME physical port.
//! No fallback transaction, ISA interpretation, architectural snapshot or retry.
use super::{Installed, PlatformKind, SharedMemory};
use crate::core::{SharedDataAccess, SharedPhysicalAccess};
use crate::memory::MemoryInterface;
use crate::physical::*;
use std::sync::{Arc, Mutex, MutexGuard};

type HeldMemory<'a> = Option<MutexGuard<'a, dyn MemoryInterface + Send + Sync + 'static>>;
fn hold(memory: &SharedMemory, serial: bool) -> Result<HeldMemory<'_>, ()> {
    if serial {
        memory.lock().map(Some).map_err(|_| ())
    } else if memory.is_poisoned() {
        Err(())
    } else {
        Ok(None)
    }
}

fn failure(request: PhysicalRequestDescriptor, context: &str) -> PhysicalAccessError {
    PhysicalAccessError::BackendFailure(PhysicalBackendFailure {
        request,
        context: context.into(),
    })
}
fn atomic_failure(request: AtomicRequestDescriptor, context: &str) -> AtomicAccessError {
    AtomicAccessError::BackendFailure(AtomicBackendFailure {
        request,
        context: context.into(),
    })
}
struct Fetch {
    memory: SharedMemory,
    port: SharedPhysicalAccess,
    serial: bool,
}
impl PhysicalAccess for Fetch {
    fn access(&mut self, request: PhysicalRequest<'_>) -> PhysicalAccessResult {
        let _guard = hold(&self.memory, self.serial)
            .map_err(|_| failure(request.descriptor(), "poisoned host memory capability"))?;
        self.port
            .lock()
            .map_err(|_| failure(request.descriptor(), "poisoned physical port"))?
            .access(request)
    }
}
struct Data {
    memory: SharedMemory,
    port: SharedDataAccess,
    serial: bool,
}
impl PhysicalAccess for Data {
    fn access(&mut self, request: PhysicalRequest<'_>) -> PhysicalAccessResult {
        let _guard = hold(&self.memory, self.serial)
            .map_err(|_| failure(request.descriptor(), "poisoned host memory capability"))?;
        self.port
            .lock()
            .map_err(|_| failure(request.descriptor(), "poisoned physical port"))?
            .access(request)
    }
}
impl AtomicAccess for Data {
    fn access_atomic(&mut self, request: AtomicRequest<'_>) -> AtomicAccessResult {
        let _guard = hold(&self.memory, self.serial)
            .map_err(|_| atomic_failure(request.descriptor(), "poisoned host memory capability"))?;
        self.port
            .lock()
            .map_err(|_| atomic_failure(request.descriptor(), "poisoned physical port"))?
            .access_atomic(request)
    }
}
pub(super) fn attach(installed: &mut Installed, memory: SharedMemory, kind: PlatformKind) {
    // Preserve flat memory() mutex serialization; native callbacks must retain
    // their independent RAM writer path while the device/bus request is active.
    let serial = kind == PlatformKind::Flat;
    installed.core.set_physical_access(
        Arc::new(Mutex::new(Fetch {
            memory: memory.clone(),
            port: installed.platform.fetch.clone(),
            serial,
        })),
        Arc::new(Mutex::new(Data {
            memory,
            port: installed.platform.data.clone(),
            serial,
        })),
    );
}
