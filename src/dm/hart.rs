//! Hart operations the Debug Module is allowed to perform.
//!
//! The module does not import `Machine`. U3 will call this trait while it
//! holds the installed hart. `step_outcome` stays off the trait: a
//! `haltreq` write must not run a hart turn inside the DM (Debug Spec 1.0
//! §3.5, halt is sampled at the next instruction boundary).

use crate::core::{DebugMemFault, RiscvCore};
use crate::csr::CsrError;

/// One hart as seen by the Debug Module.
pub trait DebugHart {
    fn set_haltreq(&mut self, haltreq: bool);
    fn haltreq(&self) -> bool;
    fn in_debug_mode(&self) -> bool;
    fn dcsr(&self) -> u64;
    fn write_dcsr(&mut self, value: u64) -> u64;
    fn dpc(&self) -> u64;
    fn write_dpc(&mut self, value: u64) -> u64;
    fn dscratch0(&self) -> u64;
    fn write_dscratch0(&mut self, value: u64);
    fn dscratch1(&self) -> u64;
    fn write_dscratch1(&mut self, value: u64);
    fn debug_resume(&mut self) -> bool;
    fn read_gpr(&self, index: u8) -> u64;
    fn write_gpr(&mut self, index: u8, value: u64);
    fn read_csr_machine(&self, addr: u16) -> Result<u64, CsrError>;
    fn write_csr_machine(&mut self, addr: u16, value: u64) -> Result<(), CsrError>;
    fn debug_physical_load(&mut self, addr: u64, size: u32) -> Result<u64, DebugMemFault>;
    fn debug_physical_store(
        &mut self,
        addr: u64,
        size: u32,
        value: u64,
    ) -> Result<(), DebugMemFault>;
}

impl DebugHart for RiscvCore {
    fn set_haltreq(&mut self, haltreq: bool) {
        RiscvCore::set_haltreq(self, haltreq);
    }

    fn haltreq(&self) -> bool {
        RiscvCore::haltreq(self)
    }

    fn in_debug_mode(&self) -> bool {
        RiscvCore::in_debug_mode(self)
    }

    fn dcsr(&self) -> u64 {
        RiscvCore::dcsr(self)
    }

    fn write_dcsr(&mut self, value: u64) -> u64 {
        RiscvCore::write_dcsr(self, value)
    }

    fn dpc(&self) -> u64 {
        RiscvCore::dpc(self)
    }

    fn write_dpc(&mut self, value: u64) -> u64 {
        RiscvCore::write_dpc(self, value)
    }

    fn dscratch0(&self) -> u64 {
        RiscvCore::dscratch0(self)
    }

    fn write_dscratch0(&mut self, value: u64) {
        RiscvCore::write_dscratch0(self, value);
    }

    fn dscratch1(&self) -> u64 {
        RiscvCore::dscratch1(self)
    }

    fn write_dscratch1(&mut self, value: u64) {
        RiscvCore::write_dscratch1(self, value);
    }

    fn debug_resume(&mut self) -> bool {
        RiscvCore::debug_resume(self)
    }

    fn read_gpr(&self, index: u8) -> u64 {
        if index == 0 {
            0
        } else {
            self.state().regs[usize::from(index)]
        }
    }

    fn write_gpr(&mut self, index: u8, value: u64) {
        if index != 0 {
            self.state_mut().regs[usize::from(index)] = value;
        }
    }

    fn read_csr_machine(&self, addr: u16) -> Result<u64, CsrError> {
        self.state().csr.read_as_machine(addr)
    }

    fn write_csr_machine(&mut self, addr: u16, value: u64) -> Result<(), CsrError> {
        self.state_mut().csr.write_as_machine(addr, value)
    }

    fn debug_physical_load(&mut self, addr: u64, size: u32) -> Result<u64, DebugMemFault> {
        RiscvCore::debug_physical_load(self, addr, size)
    }

    fn debug_physical_store(
        &mut self,
        addr: u64,
        size: u32,
        value: u64,
    ) -> Result<(), DebugMemFault> {
        RiscvCore::debug_physical_store(self, addr, size, value)
    }
}
