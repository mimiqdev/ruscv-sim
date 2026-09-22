//! Memory Interface Module
//!
//! Defines a generic interface for memory access and a simple implementation.
//! `SimpleMemory` also carries the storage-level committed-write bookkeeping
//! and the atomic critical-section primitives required by dev-plan §5.2 M1
//! and §5.5 (approved P2a-precise).

use std::collections::BTreeMap;
use std::sync::RwLock;
use thiserror::Error;

/// Memory errors
#[derive(Error, Debug)]
pub enum MemoryError {
    #[error("Invalid memory address: 0x{0:016x}")]
    InvalidAddress(u64),
    #[error("Misaligned access: addr 0x{0:016x}, requires {1}-byte alignment")]
    Misaligned(u64, u32),
    #[error("Memory access out of bounds")]
    OutOfBounds,
    /// The target/backend rejected the request because the host-side adapter
    /// could not complete it.  This is distinct from a guest-visible target
    /// rejection and must not be guessed from an error string.
    #[error("Memory backend failure: {0}")]
    Backend(String),
    /// The physical-access adapter violated its transport/protocol contract.
    #[error("Memory protocol failure: {0}")]
    Protocol(String),
    /// The physical-access adapter cannot establish whether the target effect
    /// completed.  This is terminal simulator state, never a guest trap.
    #[error("Memory completion is unknown: {0}")]
    Unknown(String),
}

/// Memory interface trait (supports RV64I with 64-bit addresses)
pub trait MemoryInterface {
    /// Read double word (8 bytes) - RV64I
    fn read_dword(&self, addr: u64) -> Result<u64, MemoryError>;
    /// Read word (4 bytes)
    fn read_word(&self, addr: u64) -> Result<u32, MemoryError>;
    /// Read half word (2 bytes)
    fn read_half(&self, addr: u64) -> Result<u16, MemoryError>;
    /// Read byte (1 byte)
    fn read_byte(&self, addr: u64) -> Result<u8, MemoryError>;
    /// Read word (zero-extended to 64 bits)
    fn read_word_zext(&self, addr: u64) -> Result<u64, MemoryError>;
    /// Read half word (zero-extended to 64 bits)
    fn read_half_zext(&self, addr: u64) -> Result<u64, MemoryError>;
    /// Read byte (zero-extended to 64 bits)
    fn read_byte_zext(&self, addr: u64) -> Result<u64, MemoryError>;
    /// Read word (sign-extended to 64 bits)
    fn read_word_sext(&self, addr: u64) -> Result<u64, MemoryError>;
    /// Read half word (sign-extended to 64 bits)
    fn read_half_sext(&self, addr: u64) -> Result<u64, MemoryError>;
    /// Read byte (sign-extended to 64 bits)
    fn read_byte_sext(&self, addr: u64) -> Result<u64, MemoryError>;

    /// Write double word (8 bytes) - RV64I
    fn write_dword(&mut self, addr: u64, value: u64) -> Result<(), MemoryError>;
    /// Write word (4 bytes)
    fn write_word(&mut self, addr: u64, value: u32) -> Result<(), MemoryError>;
    /// Write half word (2 bytes)
    fn write_half(&mut self, addr: u64, value: u16) -> Result<(), MemoryError>;
    /// Write byte (1 byte)
    fn write_byte(&mut self, addr: u64, value: u8) -> Result<(), MemoryError>;

    /// Get memory size
    fn size(&self) -> usize;
}

/// The committed-write version recorded for bytes no write has covered yet.
///
/// The version clock starts above this sentinel, so version zero on a byte
/// means exactly "no committed write has covered this byte".
const NEVER_COMMITTED_VERSION: u64 = 0;

/// Storage-level committed-write bookkeeping (dev-plan §5.2 M1, §5.5 P2a-precise).
///
/// Every write that commits bytes records the last committed-write version of
/// each covered byte as a disjoint, sorted interval map keyed by span start.
/// A load-reserved captures the exact per-byte versions of its span inside
/// the same critical section as its read; a store-conditional re-checks them
/// inside its own critical section, so a conditional store fails iff a
/// committed write overlapped the reserved span.  This is the approved
/// P2a-precise bookkeeping: a standalone coarse or global counter is
/// deliberately not used, because it would fail a store-conditional whose
/// reservation was never overlapped.
///
/// The structure lives under the same `RwLock` as the bytes themselves, so a
/// commit and its bookkeeping bump are one lock-held sequence with no
/// observable gap, and reads never take a second lock while a critical
/// section holds the first.
#[derive(Debug, Default)]
struct CommittedWriteBookkeeping {
    /// Disjoint sorted byte intervals `start -> (end_exclusive, version)`.
    /// Every interval records the version of the most recent committed write
    /// covering it; intervals never overlap and never touch with equal
    /// versions coalesced across commits (versions differ per commit, so
    /// adjacency carries no merge meaning).
    versions: BTreeMap<u64, (u64, u64)>,
    /// Monotonic committed-write clock.  `NEVER_COMMITTED_VERSION` is
    /// reserved for never-written bytes; the first commit issues version 1.
    next_version: u64,
}

impl CommittedWriteBookkeeping {
    /// Returns the last committed-write version covering one byte offset, or
    /// `NEVER_COMMITTED_VERSION` when no committed write has covered it.
    fn byte_version(&self, offset: u64) -> u64 {
        if let Some((_, &(end, version))) = self.versions.range(..=offset).next_back() {
            if offset < end {
                return version;
            }
        }
        NEVER_COMMITTED_VERSION
    }

    /// Records one committed byte span under a fresh version.
    ///
    /// An empty or wrapping span commits no bytes and bumps nothing.  The
    /// interval map is split at the span boundaries first, so the covered
    /// intervals can be dropped without stranding a partial overlap.
    fn commit(&mut self, offset: u64, len: usize) {
        if len == 0 {
            return;
        }
        let Some(end) = offset.checked_add(len as u64) else {
            return;
        };
        self.next_version += 1;
        let version = self.next_version;

        // Split the interval containing the span start so nothing crosses it.
        if let Some((&start, &(existing_end, existing_version))) =
            self.versions.range(..=offset).next_back()
        {
            if start < offset && offset < existing_end {
                self.versions.insert(end, (existing_end, existing_version));
                self.versions.insert(offset, (end, version));
                self.versions.insert(start, (offset, existing_version));
            }
        }
        // Split the interval containing the exclusive span end so nothing
        // crosses it.
        if let Some((&start, &(existing_end, existing_version))) =
            self.versions.range(..=end).next_back()
        {
            if start < end && end < existing_end {
                self.versions.insert(end, (existing_end, existing_version));
                self.versions.insert(start, (end, existing_version));
            }
        }
        // Drop every interval fully covered by the new span, then insert the
        // fresh record.  The splits above guarantee no interval crosses the
        // boundaries, so the retain cannot strand a partial overlap.
        self.versions
            .retain(|&start, &mut (existing_end, _)| !(start >= offset && existing_end <= end));
        self.versions.insert(offset, (end, version));
    }

    /// Returns the exact per-byte versions of `offset..offset + len` as
    /// opaque little-endian version bytes (eight bytes per covered byte).
    fn snapshot(&self, offset: u64, len: usize) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(len * 8);
        for i in 0..len {
            bytes.extend_from_slice(&self.byte_version(offset + i as u64).to_le_bytes());
        }
        bytes
    }

    /// Returns `Ok(true)` when the span's current per-byte versions differ
    /// from the snapshot — the exact committed-overlap check — or `Err` when
    /// the snapshot cannot describe the span.
    fn span_overwritten(
        &self,
        offset: u64,
        len: usize,
        snapshot: &[u8],
    ) -> Result<bool, MemoryError> {
        if snapshot.is_empty() || snapshot.len() != len * 8 {
            return Err(MemoryError::Protocol(
                "committed-write snapshot does not match the reserved span".into(),
            ));
        }
        for i in 0..len {
            let recorded = u64::from_le_bytes(
                snapshot[i * 8..i * 8 + 8]
                    .try_into()
                    .expect("snapshot is a multiple of eight bytes"),
            );
            if recorded != self.byte_version(offset + i as u64) {
                return Ok(true);
            }
        }
        Ok(false)
    }
}

/// The bytes and bookkeeping of one storage object under a single lock.
///
/// Keeping both fields inside one `RwLock` makes every committed write and
/// its bookkeeping bump one lock-held sequence: the bump is applied after
/// the bytes commit and before the lock is released, and an atomic critical
/// section holds the lock across its entire read → check/transform → write.
#[derive(Debug)]
struct MemoryStorage {
    /// Memory bytes.
    data: Vec<u8>,
    /// Committed-write bookkeeping covering the same byte offsets.
    bookkeeping: CommittedWriteBookkeeping,
}

/// The result of an atomic load-reserved critical section: the exact old span
/// bytes plus the committed-write snapshot of the covered span.
///
/// `old_bytes` is zero-padded beyond the span width; callers slice off the
/// first `width` bytes.  `snapshot` is opaque to the Hart and must be echoed
/// verbatim into a store-conditional covering the same span.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadReservedOutcome {
    /// Old span bytes, zero-padded to eight bytes.
    pub old_bytes: [u8; 8],
    /// The committed-write version snapshot of the span.
    pub snapshot: Vec<u8>,
}

/// Simple memory implementation
pub struct SimpleMemory {
    /// Memory bytes and committed-write bookkeeping (one locking domain).
    storage: RwLock<MemoryStorage>,
    /// Memory size
    size: usize,
}

/// Tests a nonempty span without constructing an exclusive endpoint.
///
/// Regions extending past u64::MAX expose only their addressable prefix.
/// The final byte is valid, but an access itself must never wrap through zero.
pub(crate) fn contains_range(base: u64, size: usize, addr: u64, width: usize) -> bool {
    let Some(last_delta) = width.checked_sub(1).and_then(|n| u64::try_from(n).ok()) else {
        return false;
    };
    if addr.checked_add(last_delta).is_none() {
        return false;
    }
    addr.checked_sub(base)
        .and_then(|offset| usize::try_from(offset).ok())
        .and_then(|offset| size.checked_sub(offset))
        .is_some_and(|remaining| width <= remaining)
}

impl SimpleMemory {
    /// Creates a new simple memory block.
    pub fn new(size: usize) -> Self {
        Self {
            storage: RwLock::new(MemoryStorage {
                data: vec![0; size],
                bookkeeping: CommittedWriteBookkeeping::default(),
            }),
            size,
        }
    }

    /// Initializes memory from existing data.
    pub fn from_data(data: Vec<u8>) -> Self {
        let size = data.len();
        Self {
            storage: RwLock::new(MemoryStorage {
                data,
                bookkeeping: CommittedWriteBookkeeping::default(),
            }),
            size,
        }
    }

    // Bounds precede alignment; retain the original guest offset in errors.
    fn checked_index(&self, addr: u64, width: usize) -> Result<usize, MemoryError> {
        if !contains_range(0, self.size, addr, width) {
            return Err(MemoryError::InvalidAddress(addr));
        }
        if !addr.is_multiple_of(width as u64) {
            return Err(MemoryError::Misaligned(addr, width as u32));
        }
        usize::try_from(addr).map_err(|_| MemoryError::InvalidAddress(addr))
    }

    /// Loads program data (little-endian).
    ///
    /// The data is loaded starting at a relative offset of 0.
    /// The `_base_addr` parameter is ignored and kept only for API compatibility.
    ///
    /// # BREAKING CHANGE
    ///
    /// This function previously used `base_addr` to determine the write offset.
    /// It now writes to the start of the memory block, ignoring the base address.
    /// This change was made to simplify memory loading, as the `SystemBus` now
    /// handles address mapping.
    ///
    /// The bytes that commit are recorded in committed-write bookkeeping
    /// (dev-plan §5.5 W6): a reservation overlapping a freshly loaded image
    /// must observe the image write.
    pub fn load_program(&self, data: &[u8], _base_addr: u64) {
        let mut storage = self.storage.write().unwrap();
        for (i, &byte) in data.iter().enumerate() {
            if i < self.size {
                storage.data[i] = byte;
            }
        }
        let committed = data.len().min(self.size);
        storage.bookkeeping.commit(0, committed);
    }

    /// Read one contiguous raw byte span without applying typed-access
    /// alignment rules.
    ///
    /// This is used by the A7 native physical adapter.  The legacy
    /// [`MemoryInterface`] methods above deliberately retain their existing
    /// width/alignment behavior; this helper only supplies the raw-byte target
    /// operation needed by the new boundary.
    pub fn read_bytes(&self, addr: u64, width: usize) -> Result<Vec<u8>, MemoryError> {
        if !contains_range(0, self.size, addr, width) {
            return Err(MemoryError::InvalidAddress(addr));
        }
        let mut bytes = vec![0; width];
        self.read_bytes_into(addr, &mut bytes)?;
        Ok(bytes)
    }

    /// Reads one contiguous raw span into caller-owned storage without applying
    /// typed-access alignment rules.
    pub fn read_bytes_into(&self, addr: u64, output: &mut [u8]) -> Result<(), MemoryError> {
        if !contains_range(0, self.size, addr, output.len()) {
            return Err(MemoryError::InvalidAddress(addr));
        }
        let index = usize::try_from(addr).map_err(|_| MemoryError::InvalidAddress(addr))?;
        let end = index
            .checked_add(output.len())
            .ok_or(MemoryError::InvalidAddress(addr))?;
        let storage = self.storage.read().unwrap();
        output.copy_from_slice(&storage.data[index..end]);
        Ok(())
    }

    /// Write one contiguous raw byte span without applying typed-access
    /// alignment rules.
    ///
    /// Bounds are validated before the single slice copy, so a rejected native
    /// transaction cannot expose a byte-prefix write.  Empty host inspection
    /// semantics remain owned by the existing executor helpers and are not
    /// changed by this target operation.
    pub fn write_bytes(&mut self, addr: u64, bytes: &[u8]) -> Result<(), MemoryError> {
        if !contains_range(0, self.size, addr, bytes.len()) {
            return Err(MemoryError::InvalidAddress(addr));
        }
        let index = usize::try_from(addr).map_err(|_| MemoryError::InvalidAddress(addr))?;
        let end = index
            .checked_add(bytes.len())
            .ok_or(MemoryError::InvalidAddress(addr))?;
        let mut storage = self.storage.write().unwrap();
        storage.data[index..end].copy_from_slice(bytes);
        storage.bookkeeping.commit(index as u64, bytes.len());
        Ok(())
    }

    /// Executes one atomic read-modify-write critical section over a span
    /// (dev-plan §5.2 M1): read the exact span, apply the caller-supplied
    /// pure transform, write the result, and return the exact old bytes.
    ///
    /// The complete span and operand are validated before any mutation, so a
    /// rejected section writes nothing.  The transform is opaque to the
    /// storage object — the Hart owns all ISA semantics; this method only
    /// applies it to the old and operand bytes.  The whole section holds the
    /// storage lock once, so no granted reader can observe a value between
    /// the old and the committed result, and the committed write bumps
    /// bookkeeping exactly once.
    ///
    /// `width` must be at most eight bytes and `operand` must be exactly
    /// `width` bytes; violations are [`MemoryError::Protocol`] failures.
    pub fn atomic_rmw(
        &mut self,
        offset: u64,
        width: usize,
        operand: &[u8],
        transform: impl Fn(&[u8], &[u8]) -> [u8; 8],
    ) -> Result<[u8; 8], MemoryError> {
        if width == 0 || width > 8 {
            return Err(MemoryError::Protocol(
                "atomic span width must be between one and eight bytes".into(),
            ));
        }
        if operand.len() != width {
            return Err(MemoryError::Protocol(
                "atomic operand does not match the span width".into(),
            ));
        }
        if !contains_range(0, self.size, offset, width) {
            return Err(MemoryError::InvalidAddress(offset));
        }
        let index = usize::try_from(offset).map_err(|_| MemoryError::InvalidAddress(offset))?;
        let end = index
            .checked_add(width)
            .ok_or(MemoryError::InvalidAddress(offset))?;

        let mut storage = self.storage.write().unwrap();
        let mut old = [0u8; 8];
        old[..width].copy_from_slice(&storage.data[index..end]);
        let new = transform(&old[..width], operand);
        storage.data[index..end].copy_from_slice(&new[..width]);
        storage.bookkeeping.commit(offset, width);
        Ok(old)
    }

    /// Executes one atomic load-reserved critical section (dev-plan §5.2 M1):
    /// read the exact span and capture the committed-write snapshot of the
    /// covered bytes in the same lock hold.
    ///
    /// The returned snapshot is opaque to the Hart; it must be echoed
    /// verbatim into the store-conditional that targets this span.  The span
    /// must be at most eight bytes so the snapshot fits the envelope's inline
    /// representation.
    pub fn atomic_load_reserved(
        &self,
        offset: u64,
        width: usize,
    ) -> Result<LoadReservedOutcome, MemoryError> {
        if width == 0 || width > 8 {
            return Err(MemoryError::Protocol(
                "atomic span width must be between one and eight bytes".into(),
            ));
        }
        if !contains_range(0, self.size, offset, width) {
            return Err(MemoryError::InvalidAddress(offset));
        }
        let index = usize::try_from(offset).map_err(|_| MemoryError::InvalidAddress(offset))?;
        let end = index
            .checked_add(width)
            .ok_or(MemoryError::InvalidAddress(offset))?;

        let storage = self.storage.read().unwrap();
        let mut old = [0u8; 8];
        old[..width].copy_from_slice(&storage.data[index..end]);
        let snapshot = storage.bookkeeping.snapshot(offset, width);
        Ok(LoadReservedOutcome {
            old_bytes: old,
            snapshot,
        })
    }

    /// Executes one atomic store-conditional critical section (dev-plan §5.2
    /// M1): re-check the echoed reservation snapshot inside the same lock
    /// hold and either commit the single write (bumping bookkeeping) or
    /// commit nothing.
    ///
    /// Returns `Ok(true)` when the conditional write committed and
    /// `Ok(false)` when the snapshot showed a committed write overlapping the
    /// reserved span — conditional failure is a completed check, not an
    /// error.  `reserved_offset`/`reserved_width` describe the Hart's
    /// reserved span (which covers the write span); a snapshot that cannot
    /// describe that span is a [`MemoryError::Protocol`] failure, never a
    /// silent conditional outcome.
    pub fn atomic_store_conditional(
        &mut self,
        offset: u64,
        width: usize,
        payload: &[u8],
        reserved_offset: u64,
        reserved_width: usize,
        snapshot: &[u8],
    ) -> Result<bool, MemoryError> {
        if width == 0 || width > 8 {
            return Err(MemoryError::Protocol(
                "atomic span width must be between one and eight bytes".into(),
            ));
        }
        if payload.len() != width {
            return Err(MemoryError::Protocol(
                "conditional-store payload does not match the span width".into(),
            ));
        }
        if !contains_range(0, self.size, offset, width) {
            return Err(MemoryError::InvalidAddress(offset));
        }
        if !contains_range(0, self.size, reserved_offset, reserved_width) {
            return Err(MemoryError::Protocol(
                "reservation context describes a span outside this storage".into(),
            ));
        }
        let index = usize::try_from(offset).map_err(|_| MemoryError::InvalidAddress(offset))?;
        let end = index
            .checked_add(width)
            .ok_or(MemoryError::InvalidAddress(offset))?;

        let mut storage = self.storage.write().unwrap();
        if storage
            .bookkeeping
            .span_overwritten(reserved_offset, reserved_width, snapshot)?
        {
            return Ok(false);
        }
        storage.data[index..end].copy_from_slice(payload);
        storage.bookkeeping.commit(offset, width);
        Ok(true)
    }

    /// Alias for [`Self::read_bytes`] for native raw-target callers.
    pub fn read_raw(&self, addr: u64, width: usize) -> Result<Vec<u8>, MemoryError> {
        self.read_bytes(addr, width)
    }

    /// Alias for [`Self::write_bytes`] for native raw-target callers.
    pub fn write_raw(&mut self, addr: u64, bytes: &[u8]) -> Result<(), MemoryError> {
        self.write_bytes(addr, bytes)
    }
}

impl MemoryInterface for SimpleMemory {
    fn read_dword(&self, addr: u64) -> Result<u64, MemoryError> {
        let addr = self.checked_index(addr, 8)?;

        let storage = self.storage.read().unwrap();
        Ok(u64::from_le_bytes([
            storage.data[addr],
            storage.data[addr + 1],
            storage.data[addr + 2],
            storage.data[addr + 3],
            storage.data[addr + 4],
            storage.data[addr + 5],
            storage.data[addr + 6],
            storage.data[addr + 7],
        ]))
    }

    fn read_word(&self, addr: u64) -> Result<u32, MemoryError> {
        let addr = self.checked_index(addr, 4)?;

        let storage = self.storage.read().unwrap();
        Ok(u32::from_le_bytes([
            storage.data[addr],
            storage.data[addr + 1],
            storage.data[addr + 2],
            storage.data[addr + 3],
        ]))
    }

    fn read_half(&self, addr: u64) -> Result<u16, MemoryError> {
        let addr = self.checked_index(addr, 2)?;

        let storage = self.storage.read().unwrap();
        Ok(u16::from_le_bytes([
            storage.data[addr],
            storage.data[addr + 1],
        ]))
    }

    fn read_byte(&self, addr: u64) -> Result<u8, MemoryError> {
        let addr = self.checked_index(addr, 1)?;
        let storage = self.storage.read().unwrap();
        Ok(storage.data[addr])
    }

    fn read_word_zext(&self, addr: u64) -> Result<u64, MemoryError> {
        Ok(self.read_word(addr)? as u64)
    }

    fn read_half_zext(&self, addr: u64) -> Result<u64, MemoryError> {
        Ok(self.read_half(addr)? as u64)
    }

    fn read_byte_zext(&self, addr: u64) -> Result<u64, MemoryError> {
        Ok(self.read_byte(addr)? as u64)
    }

    fn read_word_sext(&self, addr: u64) -> Result<u64, MemoryError> {
        let val = self.read_word(addr)?;
        Ok((val as i32) as i64 as u64)
    }

    fn read_half_sext(&self, addr: u64) -> Result<u64, MemoryError> {
        let val = self.read_half(addr)?;
        Ok((val as i16) as i64 as u64)
    }

    fn read_byte_sext(&self, addr: u64) -> Result<u64, MemoryError> {
        let val = self.read_byte(addr)?;
        Ok((val as i8) as i64 as u64)
    }

    fn write_dword(&mut self, addr: u64, value: u64) -> Result<(), MemoryError> {
        let addr = self.checked_index(addr, 8)?;

        let mut storage = self.storage.write().unwrap();
        let bytes = value.to_le_bytes();
        storage.data[addr..addr + 8].copy_from_slice(&bytes);
        storage.bookkeeping.commit(addr as u64, 8);
        Ok(())
    }

    fn write_word(&mut self, addr: u64, value: u32) -> Result<(), MemoryError> {
        let addr = self.checked_index(addr, 4)?;

        let mut storage = self.storage.write().unwrap();
        let bytes = value.to_le_bytes();
        storage.data[addr..addr + 4].copy_from_slice(&bytes);
        storage.bookkeeping.commit(addr as u64, 4);
        Ok(())
    }

    fn write_half(&mut self, addr: u64, value: u16) -> Result<(), MemoryError> {
        let addr = self.checked_index(addr, 2)?;

        let mut storage = self.storage.write().unwrap();
        let bytes = value.to_le_bytes();
        storage.data[addr..addr + 2].copy_from_slice(&bytes);
        storage.bookkeeping.commit(addr as u64, 2);
        Ok(())
    }

    fn write_byte(&mut self, addr: u64, value: u8) -> Result<(), MemoryError> {
        let addr = self.checked_index(addr, 1)?;
        let mut storage = self.storage.write().unwrap();
        storage.data[addr] = value;
        storage.bookkeeping.commit(addr as u64, 1);
        Ok(())
    }

    fn size(&self) -> usize {
        self.size
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_memory_read_write() {
        let mut mem = SimpleMemory::new(1024);

        // Write and read word
        mem.write_word(0x100, 0x12345678).unwrap();
        assert_eq!(mem.read_word(0x100).unwrap(), 0x12345678);

        // Write and read half word
        mem.write_half(0x200, 0xABCD).unwrap();
        assert_eq!(mem.read_half(0x200).unwrap(), 0xABCD);

        // Write and read byte
        mem.write_byte(0x300, 0x42).unwrap();
        assert_eq!(mem.read_byte(0x300).unwrap(), 0x42);
    }

    #[test]
    fn test_memory_misaligned() {
        let mut mem = SimpleMemory::new(1024);

        // Misaligned word access
        assert!(mem.read_word(0x101).is_err());
        assert!(mem.write_word(0x101, 0).is_err());

        // Misaligned half word access
        assert!(mem.read_half(0x101).is_err());
        assert!(mem.write_half(0x101, 0).is_err());
    }

    #[test]
    fn test_load_program() {
        let mem = SimpleMemory::new(0x2000); // 8KB, enough for 0x1000 address
        let program = vec![0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08];

        mem.load_program(&program, 0x1000); // base_addr is ignored, uses relative offset

        assert_eq!(mem.read_byte(0).unwrap(), 0x01);
        assert_eq!(mem.read_byte(7).unwrap(), 0x08);
        assert_eq!(mem.read_word(0).unwrap(), 0x04030201); // little-endian
    }
}
