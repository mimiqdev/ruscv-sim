//! Internal subscribed typed-access capture. It forwards exactly one existing
//! helper call, without extra memory reads or a second execution engine.
use super::observation::{MemoryEffect, MemoryJournal};
use crate::memory::{MemoryError, MemoryInterface};
use crate::physical::PhysicalWidth;

pub(super) struct ObservedMemory<'a> {
    inner: &'a mut dyn MemoryInterface,
    journal: Option<&'a MemoryJournal>,
    base: u64,
}

impl<'a> ObservedMemory<'a> {
    pub(super) fn new(
        inner: &'a mut dyn MemoryInterface,
        journal: Option<&'a MemoryJournal>,
        base: u64,
    ) -> Self {
        Self {
            inner,
            journal,
            base,
        }
    }

    fn record(&self, address: u64, width: PhysicalWidth, value: u64, write: bool) {
        if let Some(journal) = self.journal {
            let mut bytes = [0; 8];
            bytes[..width.bytes()].copy_from_slice(&value.to_le_bytes()[..width.bytes()]);
            journal.borrow_mut().push(MemoryEffect {
                guest_address: address,
                issued_address: address.checked_sub(self.base),
                width,
                read: (!write).then_some(bytes),
                write: write.then_some(bytes),
                atomic: None,
            });
        }
    }
}

macro_rules! read {
    ($method:ident, $ty:ty, $width:ident) => {
        fn $method(&self, addr: u64) -> Result<$ty, MemoryError> {
            let value = self.inner.$method(addr)?;
            self.record(addr, PhysicalWidth::$width, value as u64, false);
            Ok(value)
        }
    };
}
macro_rules! write {
    ($method:ident, $ty:ty, $width:ident) => {
        fn $method(&mut self, addr: u64, value: $ty) -> Result<(), MemoryError> {
            self.inner.$method(addr, value)?;
            self.record(addr, PhysicalWidth::$width, value as u64, true);
            Ok(())
        }
    };
}

impl MemoryInterface for ObservedMemory<'_> {
    read!(read_dword, u64, Doubleword);
    read!(read_word, u32, Word);
    read!(read_half, u16, Halfword);
    read!(read_byte, u8, Byte);
    read!(read_word_zext, u64, Word);
    read!(read_half_zext, u64, Halfword);
    read!(read_byte_zext, u64, Byte);
    read!(read_word_sext, u64, Word);
    read!(read_half_sext, u64, Halfword);
    read!(read_byte_sext, u64, Byte);
    write!(write_dword, u64, Doubleword);
    write!(write_word, u32, Word);
    write!(write_half, u16, Halfword);
    write!(write_byte, u8, Byte);
    fn size(&self) -> usize {
        self.inner.size()
    }
}
