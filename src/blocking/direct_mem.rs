//! TLM Direct Memory Interface vocabulary (direct-memory D3–D5, D8 and D10).
//!
//! "Direct memory" here is the TLM Direct Memory Interface of IEEE 1666.  It is
//! unrelated to the RISC-V Debug Module Interface, which the source calls `dmi`
//! in `crate::dm`, `crate::jtag` and the Machine debug hub.
//!
//! The adapter in [`super::adapter`] is the only simulator-side user of these
//! types: no Hart, ISA, MMU, TLB, decoder or DM code sees a grant or a host
//! pointer, so the direct path stays below the validated physical access
//! boundary (direct-memory D4).  A target offers direct memory by answering a
//! [`BlockingDirectMemQuery`] and by keeping an adapter's
//! [`BlockingDirectMemInvalidator`], which lets it revoke every cached grant
//! before it frees, moves or remaps the storage behind one (direct-memory
//! D8/D10).

use super::Femtoseconds;
use std::fmt;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Arc, Mutex, Weak};

/// The access one direct-memory grant covers (direct-memory D3/D5).
///
/// [`Self::NoAccess`] is IEEE `DMI_ACCESS_NONE`.  A grant that carries it is
/// malformed, never a silent denial, so a responder can report it as such.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BlockingDirectMemAccess {
    /// IEEE `DMI_ACCESS_NONE`: the grant carries no access at all.
    NoAccess,
    /// `DMI_ACCESS_READ`.
    Read,
    /// `DMI_ACCESS_WRITE`.
    Write,
    /// `DMI_ACCESS_READ_WRITE`.
    ReadWrite,
}

impl BlockingDirectMemAccess {
    /// True when a grant with this access permits a load or a fetch.
    pub const fn allows_read(self) -> bool {
        matches!(self, Self::Read | Self::ReadWrite)
    }

    /// True when a grant with this access permits a store.
    pub const fn allows_write(self) -> bool {
        matches!(self, Self::Write | Self::ReadWrite)
    }

    /// True when the grant carries any access.
    pub const fn has_access(self) -> bool {
        !matches!(self, Self::NoAccess)
    }

    /// The access name, for diagnostics.
    pub const fn name(self) -> &'static str {
        match self {
            Self::NoAccess => "no access",
            Self::Read => "read",
            Self::Write => "write",
            Self::ReadWrite => "read-write",
        }
    }
}

/// One direct-memory query for one access (direct-memory D3).
///
/// The adapter issues at most one query per completed Hart-initiated non-atomic
/// transaction, and only after the target's [`super::BlockingTransaction`]
/// `dmi_allowed` hint, so a target never sees a speculative query.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlockingDirectMemQuery {
    address: u64,
    length: usize,
    access: BlockingDirectMemAccess,
}

impl BlockingDirectMemQuery {
    /// Creates one query for `length` bytes at the physical `address`.
    pub const fn new(address: u64, length: usize, access: BlockingDirectMemAccess) -> Self {
        Self {
            address,
            length,
            access,
        }
    }

    /// The queried physical start address.
    pub const fn address(self) -> u64 {
        self.address
    }

    /// The queried byte length, which is the access width.
    pub const fn length(self) -> usize {
        self.length
    }

    /// The queried access direction.
    pub const fn access(self) -> BlockingDirectMemAccess {
        self.access
    }
}

/// One target grant of a host-addressable guest-RAM range (direct-memory
/// D3/D10).
///
/// The inclusive range covers the physically addressed bytes, and `pointer`
/// addresses its first byte.  A grant is a promise, so constructing one is
/// `unsafe`: the adapter validates only its shape, never the promise itself.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct BlockingDirectMemGrant {
    start: u64,
    end: u64,
    pointer: *mut u8,
    access: BlockingDirectMemAccess,
    read_latency: Femtoseconds,
    write_latency: Femtoseconds,
}

impl BlockingDirectMemGrant {
    /// Creates one grant over the inclusive physical range `start..=end`.
    ///
    /// # Safety
    ///
    /// The caller promises all of the following until it has called
    /// [`BlockingDirectMemInvalidator::invalidate`] for a range covering this
    /// one (direct-memory D10):
    ///
    /// * `pointer` addresses `start` and stays valid for the whole inclusive
    ///   range, at that address, until that invalidation;
    /// * it addresses the same bytes [`super::BlockingTarget::b_transport`] and
    ///   [`super::BlockingTarget::transport_dbg`] serve, so a write through any
    ///   of the three is visible to the others at once;
    /// * the range is granted only where `b_transport` would complete with
    ///   [`super::BlockingResponseStatus::Ok`] for `access`, so faults stay
    ///   identical;
    /// * the storage behind it is interior-mutable atomics (a Rust target
    ///   grants no `&mut` alias), because the adapter copies through it with
    ///   relaxed atomic accesses only.
    pub unsafe fn new(
        start: u64,
        end: u64,
        pointer: *mut u8,
        access: BlockingDirectMemAccess,
        read_latency: Femtoseconds,
        write_latency: Femtoseconds,
    ) -> Self {
        Self {
            start,
            end,
            pointer,
            access,
            read_latency,
            write_latency,
        }
    }

    /// The inclusive physical start address.
    pub const fn start(self) -> u64 {
        self.start
    }

    /// The inclusive physical end address.
    pub const fn end(self) -> u64 {
        self.end
    }

    /// The host pointer of the start byte.
    pub const fn pointer(self) -> *mut u8 {
        self.pointer
    }

    /// The granted access.
    pub const fn access(self) -> BlockingDirectMemAccess {
        self.access
    }

    /// The granted read latency, in exact femtoseconds per access.
    pub const fn read_latency(self) -> Femtoseconds {
        self.read_latency
    }

    /// The granted write latency, in exact femtoseconds per access.
    pub const fn write_latency(self) -> Femtoseconds {
        self.write_latency
    }
}

impl fmt::Debug for BlockingDirectMemGrant {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("BlockingDirectMemGrant")
            .field("start", &format_args!("{:#018x}", self.start))
            .field("end", &format_args!("{:#018x}", self.end))
            .field("pointer", &self.pointer)
            .field("access", &self.access)
            .field("read_latency", &self.read_latency)
            .field("write_latency", &self.write_latency)
            .finish()
    }
}

/// A target's answer to one [`BlockingDirectMemQuery`] (direct-memory D3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockingDirectMemResponse {
    /// The grant the adapter may cache.
    Granted(BlockingDirectMemGrant),
    /// No grant for this access: nothing is cached and this is not an error.
    Denied,
    /// The target could not complete the query, as when a C++ exception is
    /// contained at its boundary.  The access becomes an unknown completion and
    /// the transport is poisoned.
    Failed,
}

/// One adapter's revocation handle, kept by a target (direct-memory D8).
///
/// The handle holds the adapter's grant cache weakly, so a released adapter's
/// registration is inert: it owns no storage and keeps nothing alive.  A target
/// keeps every registration it receives and never replaces one with another,
/// because a reset must reach the composition that is still current as well as
/// the one under construction.  [`BlockingDirectMemRegistry`] provides that
/// bookkeeping.
#[derive(Clone)]
pub struct BlockingDirectMemInvalidator {
    regions: Weak<DirectMemRegions>,
}

impl BlockingDirectMemInvalidator {
    /// Creates the handle an adapter registers with its target.
    pub(crate) fn new(regions: &Arc<DirectMemRegions>) -> Self {
        Self {
            regions: Arc::downgrade(regions),
        }
    }

    /// Removes every cached grant that overlaps the inclusive physical range
    /// `start..=end`.
    ///
    /// This may be called at any time from any thread, including synchronously
    /// from inside [`super::BlockingTarget::b_transport`], and it takes only
    /// the adapter's dedicated direct-memory lock.  That lock is never held
    /// across a target call and a direct copy holds it, so when `invalidate`
    /// returns no copy through a removed grant is in flight and none can start
    /// (direct-memory D8).  A malformed range is treated as an invalidation of
    /// the whole address space, because a stale grant is never acceptable.
    pub fn invalidate(&self, start: u64, end: u64) {
        if let Some(regions) = self.regions.upgrade() {
            regions.invalidate(start, end);
        }
    }

    /// True while the adapter that registered this handle is alive.
    pub fn is_live(&self) -> bool {
        self.regions.strong_count() != 0
    }
}

impl fmt::Debug for BlockingDirectMemInvalidator {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("BlockingDirectMemInvalidator")
            .field("live", &self.is_live())
            .finish()
    }
}

/// Every direct-memory registration of one target (direct-memory D8).
///
/// A target that grants direct memory owns one of these and registers every
/// [`BlockingDirectMemInvalidator`] it receives.  `register` appends and never
/// replaces, so a single reset reaches every adapter that once registered,
/// including the composition a recomposition is replacing.  Registrations whose
/// adapter is gone are inert and are pruned on the next mutation.
#[derive(Default)]
pub struct BlockingDirectMemRegistry {
    invalidators: Mutex<Vec<BlockingDirectMemInvalidator>>,
}

impl BlockingDirectMemRegistry {
    /// Creates an empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Keeps one adapter registration without replacing an earlier one.
    pub fn register(&self, invalidator: BlockingDirectMemInvalidator) {
        let mut registrations = self
            .invalidators
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        registrations.retain(BlockingDirectMemInvalidator::is_live);
        registrations.push(invalidator);
    }

    /// Invalidates the inclusive physical range `start..=end` in every live
    /// registration.
    pub fn invalidate(&self, start: u64, end: u64) {
        let registrations: Vec<_> = {
            let mut registrations = self
                .invalidators
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            registrations.retain(BlockingDirectMemInvalidator::is_live);
            registrations.clone()
        };
        for invalidator in registrations {
            invalidator.invalidate(start, end);
        }
    }

    /// Invalidates the whole physical address space in every live registration.
    ///
    /// A target calls this before it discards granted storage.
    pub fn invalidate_all(&self) {
        self.invalidate(0, u64::MAX);
    }

    /// The number of live registrations.
    pub fn registrations(&self) -> usize {
        self.invalidators
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .iter()
            .filter(|invalidator| invalidator.is_live())
            .count()
    }
}

impl fmt::Debug for BlockingDirectMemRegistry {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("BlockingDirectMemRegistry")
            .field("registrations", &self.registrations())
            .finish()
    }
}

/// Most grants one adapter keeps at a time (direct-memory D4).
///
/// Eviction changes transaction counts only: the next access over an evicted
/// range takes `b_transport` and obtains a fresh grant from that response.
pub(crate) const DIRECT_MEM_REGION_LIMIT: usize = 8;

/// One cached grant inside an adapter.
struct Region {
    start: u64,
    end: u64,
    pointer: *mut u8,
    access: BlockingDirectMemAccess,
    read_latency: Femtoseconds,
    write_latency: Femtoseconds,
}

// SAFETY: a region stores a raw address, never a Rust reference.  The grant
// behind it promises that the bytes stay where they are and that the adapter
// copies through them with relaxed atomic accesses only (direct-memory D10), so
// reading that address from any thread is sound while the region is cached.
unsafe impl Send for Region {}
// SAFETY: as above; a shared `&Region` exposes no interior mutability of its own
// and the granted bytes are atomics on the target's side.
unsafe impl Sync for Region {}

impl Region {
    fn new(grant: BlockingDirectMemGrant) -> Self {
        Self {
            start: grant.start(),
            end: grant.end(),
            pointer: grant.pointer(),
            access: grant.access(),
            read_latency: grant.read_latency(),
            write_latency: grant.write_latency(),
        }
    }

    /// True when the whole span `paddr..paddr + width - 1` lies inside.
    fn covers(&self, paddr: u64, width: usize) -> bool {
        let Some(last) = paddr.checked_add(width.saturating_sub(1) as u64) else {
            return false;
        };
        self.start <= paddr && last <= self.end
    }

    /// True when this region overlaps the inclusive range `start..=end`.
    fn overlaps(&self, start: u64, end: u64) -> bool {
        self.start <= end && start <= self.end
    }
}

#[derive(Default)]
struct RegionCache {
    regions: Vec<Region>,
    /// Monotonic count of invalidations.  A grant is discarded when it changed
    /// while the grant's query was in flight (direct-memory D8).
    invalidations: u64,
}

/// An adapter's cached grants and their dedicated lock (direct-memory D8/D9).
///
/// The lock is a leaf: no target call and no other lock is taken while it is
/// held, so an invalidation may itself arrive from inside `b_transport`.
pub(crate) struct DirectMemRegions {
    state: Mutex<RegionCache>,
    /// Test-only pause point inside a direct copy.  It runs while the
    /// direct-memory lock is held, so a test can prove that an invalidation
    /// from another thread waits for an in-flight copy of either direction.
    #[cfg(test)]
    copy_hook: Mutex<Option<Arc<dyn Fn() + Send + Sync>>>,
}

impl DirectMemRegions {
    pub(crate) fn new() -> Arc<Self> {
        Arc::new(Self {
            state: Mutex::new(RegionCache::default()),
            #[cfg(test)]
            copy_hook: Mutex::new(None),
        })
    }

    /// Installs or clears the test-only pause point inside a direct copy.
    #[cfg(test)]
    pub(crate) fn set_copy_hook(&self, hook: Option<Arc<dyn Fn() + Send + Sync>>) {
        *self
            .copy_hook
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = hook;
    }

    /// The handle this adapter registers with its target.
    pub(crate) fn invalidator(self: &Arc<Self>) -> BlockingDirectMemInvalidator {
        BlockingDirectMemInvalidator::new(self)
    }

    /// True while a direct copy holds the direct-memory lock, for tests.
    ///
    /// A `try_lock` failure is the only scheduler-independent way to observe
    /// that the lock is held, so the copy-barrier test uses this instead of a
    /// timing window.
    #[cfg(test)]
    pub(crate) fn lock_held(&self) -> bool {
        self.state.try_lock().is_err()
    }

    /// The current invalidation count, sampled before one query begins.
    pub(crate) fn epoch(&self) -> u64 {
        match self.state.lock() {
            Ok(cache) => cache.invalidations,
            Err(_) => 0,
        }
    }

    /// Removes every cached grant overlapping `start..=end`.
    fn invalidate(&self, start: u64, end: u64) {
        // An inverted range is a caller error, and keeping a grant through it
        // could outlive the storage behind it, so widen to the whole space.
        let (start, end) = if start <= end {
            (start, end)
        } else {
            (0, u64::MAX)
        };
        let mut cache = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        cache.invalidations = cache.invalidations.wrapping_add(1);
        cache.regions.retain(|region| !region.overlaps(start, end));
    }

    /// Removes every cached grant.
    pub(crate) fn clear(&self) {
        let mut cache = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        cache.invalidations = cache.invalidations.wrapping_add(1);
        cache.regions.clear();
    }

    /// Copies one read span through a cached grant.
    ///
    /// Returns the grant's read latency, or `None` when no single cached grant
    /// covers the whole span with read permission, in which case the caller
    /// takes `b_transport` and never splits the access (direct-memory D4).
    /// The lookup and the copy happen in one lock hold, so an invalidation
    /// cannot free the storage between them.
    pub(crate) fn read(&self, paddr: u64, width: usize, bytes: &mut [u8]) -> Option<Femtoseconds> {
        let cache = self.state.lock().ok()?;
        let region = cache
            .regions
            .iter()
            .find(|region| region.covers(paddr, width) && region.access.allows_read())?;
        let offset = paddr - region.start;
        #[cfg(test)]
        {
            let hook = self
                .copy_hook
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .clone();
            if let Some(hook) = hook {
                hook();
            }
        }
        for (index, byte) in bytes.iter_mut().take(width).enumerate() {
            // SAFETY: the grant promises the range stays valid until an
            // overlapping invalidation, and this hold excludes one, so the
            // pointer addresses `width` bytes from `paddr`.  `AtomicU8` has
            // byte layout and alignment, and the relaxed accesses match the
            // target's own (direct-memory D10).
            *byte = unsafe { AtomicU8::from_ptr(region.pointer.add(offset as usize + index)) }
                .load(Ordering::Relaxed);
        }
        Some(region.read_latency)
    }

    /// Copies one store span through a cached grant.
    ///
    /// Returns the grant's write latency, or `None` when no single cached grant
    /// covers the whole span with write permission, in which case the caller
    /// takes `b_transport` and never splits the access (direct-memory D4/D5).
    /// Lookup and copy happen in one lock hold, so an invalidation cannot free
    /// the storage between them (direct-memory D8).
    pub(crate) fn write(&self, paddr: u64, width: usize, bytes: &[u8]) -> Option<Femtoseconds> {
        let cache = self.state.lock().ok()?;
        let region = cache
            .regions
            .iter()
            .find(|region| region.covers(paddr, width) && region.access.allows_write())?;
        let offset = paddr - region.start;
        #[cfg(test)]
        {
            let hook = self
                .copy_hook
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .clone();
            if let Some(hook) = hook {
                hook();
            }
        }
        for (index, byte) in bytes.iter().take(width).enumerate() {
            // SAFETY: the grant promises the range stays valid until an
            // overlapping invalidation, and this hold excludes one, so the
            // pointer addresses `width` writable bytes from `paddr`.  The
            // storage behind it is atomic (direct-memory D10), so a relaxed
            // store through it cannot race the target's own accesses.
            unsafe { AtomicU8::from_ptr(region.pointer.add(offset as usize + index)) }
                .store(*byte, Ordering::Relaxed);
        }
        Some(region.write_latency)
    }

    /// Caches one validated grant, replacing every grant it overlaps.
    ///
    /// A grant is discarded when an invalidation arrived since the epoch
    /// sampled before its query began (direct-memory D8).  The adapter does not
    /// distinguish an overlapping from an unrelated invalidation: a target
    /// invalidates on its own schedule, and the next access re-queries.
    pub(crate) fn install(&self, grant: BlockingDirectMemGrant, epoch: u64) -> bool {
        let mut cache = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if cache.invalidations != epoch {
            return false;
        }
        let region = Region::new(grant);
        cache
            .regions
            .retain(|cached| !cached.overlaps(region.start, region.end));
        if cache.regions.len() >= DIRECT_MEM_REGION_LIMIT {
            cache.regions.remove(0);
        }
        cache.regions.push(region);
        true
    }

    /// Number of cached grants.
    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        match self.state.lock() {
            Ok(cache) => cache.regions.len(),
            Err(_) => 0,
        }
    }
}
