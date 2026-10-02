//! Shared image/address metadata utilities, independent of Runner policy/errors.
use crate::elf::SignatureInfo;

#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub(crate) struct PlacementError(String);

/// Storage address form, not MMU translation.
#[derive(Debug, Clone, Copy)]
pub(crate) enum AddressForm {
    Bus,
    Flat { base_addr: u64, memory_size: u64 },
}
impl AddressForm {
    pub(crate) fn resolve(
        self,
        guest_addr: u64,
        len: u64,
        what: &str,
    ) -> Result<u64, PlacementError> {
        match self {
            Self::Bus => Ok(guest_addr),
            Self::Flat {
                base_addr,
                memory_size,
            } => {
                let offset = guest_addr.checked_sub(base_addr).ok_or_else(|| {
                    PlacementError(format!(
                        "{what} address 0x{guest_addr:016x} is below image base 0x{base_addr:016x}"
                    ))
                })?;
                let end = offset.checked_add(len).ok_or_else(|| {
                    PlacementError(format!(
                        "{what} address 0x{guest_addr:016x} overlaps the end of the address space"
                    ))
                })?;
                if end > memory_size {
                    return Err(PlacementError(format!(
                        "{what} address 0x{guest_addr:016x} maps to flat offset 0x{offset:016x}, outside the {memory_size:#x}-byte image memory"
                    )));
                }
                Ok(offset)
            }
        }
    }
    /// Native routes RAM at the image base; flat adapts guest addresses once.
    pub(crate) fn core_translation_base(self) -> u64 {
        match self {
            Self::Bus => 0,
            Self::Flat { base_addr, .. } => base_addr,
        }
    }
}

/// One metadata conversion shared by compatibility Runner and Machine/Platform.
#[derive(Debug, Clone, Default)]
pub(crate) struct ImagePlacement {
    base_addr: u64,
    memory_size: u64,
    tohost: Option<u64>,
    signature: Option<SignatureInfo>,
}
impl ImagePlacement {
    pub(crate) fn new(
        base_addr: u64,
        memory_size: usize,
        tohost: Option<u64>,
        signature: Option<SignatureInfo>,
    ) -> Self {
        Self {
            base_addr,
            memory_size: memory_size as u64,
            tohost,
            signature,
        }
    }
    pub(crate) fn address_form(&self) -> AddressForm {
        AddressForm::Flat {
            base_addr: self.base_addr,
            memory_size: self.memory_size,
        }
    }
    pub(crate) fn tohost(&self, form: AddressForm) -> Result<Option<u64>, PlacementError> {
        self.tohost
            .map(|addr| form.resolve(addr, 8, "ELF tohost"))
            .transpose()
    }
    pub(crate) fn signature_address(
        &self,
        info: &SignatureInfo,
        form: AddressForm,
    ) -> Result<u64, PlacementError> {
        form.resolve(info.vaddr, info.size, "ELF signature")
    }
    pub(crate) fn signature_info(&self) -> Option<&SignatureInfo> {
        self.signature.as_ref()
    }
    pub(crate) fn tohost_guest(&self) -> Option<u64> {
        self.tohost
    }
}
