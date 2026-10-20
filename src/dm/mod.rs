//! DMI-level Debug Module.
//!
//! Debug Specification 1.0, tag `5695c0a`, §3.1 and §3.14. This module
//! answers register reads and writes. It does not shift JTAG, and it does
//! not own `Machine`.

/// One hart's Debug Module, addressed by a 7-bit DMI register number.
pub struct DebugModule<H> {
    hart: H,
}

impl<H> DebugModule<H> {
    /// Power-up state. `dmactive` is 0 until a debugger sets it (§3.13).
    pub fn new(hart: H) -> Self {
        Self { hart }
    }

    /// Read one DMI register. The address is the low 7 bits (§3.1, §3.14).
    pub fn dmi_read(&mut self, _addr: u8) -> u32 {
        0
    }

    /// Write one DMI register. The address is the low 7 bits (§3.14).
    pub fn dmi_write(&mut self, _addr: u8, _value: u32) {}

    pub fn hart(&self) -> &H {
        &self.hart
    }

    pub fn hart_mut(&mut self) -> &mut H {
        &mut self.hart
    }
}
