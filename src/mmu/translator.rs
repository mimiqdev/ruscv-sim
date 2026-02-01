//! Address translation engine

use super::{AccessType, MmuConfig, MmuError, PrivilegeMode, Satp, Tlb, TranslationMode};

/// Translation request
#[derive(Debug, Clone, Copy)]
pub struct TranslationRequest {
    pub vaddr: u64,
    pub access_type: AccessType,
    pub privilege: PrivilegeMode,
    pub satp: u64,
    pub mstatus: u64,
}

/// Translation result
#[derive(Debug, Clone, Copy)]
pub struct TranslationResult {
    pub paddr: u64,
    pub pte_addr: Option<u64>,
    pub pte_value: Option<u64>,
}

/// Address translator
pub struct AddressTranslator {
    config: MmuConfig,
}

impl AddressTranslator {
    pub fn new(config: MmuConfig) -> Self {
        Self { config }
    }

    pub fn translate(
        &self,
        request: TranslationRequest,
        tlb: &Tlb,
    ) -> Result<u64, MmuError> {
        let satp = Satp(request.satp);
        
        // Check translation mode
        match satp.mode() {
            TranslationMode::Bare => {
                // No translation
                Ok(request.vaddr)
            }
            TranslationMode::Sv39 => {
                self.translate_sv39(request, satp)
            }
            TranslationMode::Sv48 => {
                if self.config.enable_sv48 {
                    Err(MmuError::UnsupportedMode(TranslationMode::Sv48))
                } else {
                    Err(MmuError::UnsupportedMode(TranslationMode::Sv48))
                }
            }
            _ => Err(MmuError::UnsupportedMode(satp.mode())),
        }
    }

    fn translate_sv39(&self, request: TranslationRequest, satp: Satp) -> Result<u64, MmuError> {
        let vaddr = request.vaddr;
        
        // Check sign extension (bits 63:39 must be all 0 or all 1)
        let sign_bits = vaddr >> 39;
        if sign_bits != 0 && sign_bits != 0x1F_FFFF {
            return Err(MmuError::InvalidVirtualAddress(vaddr));
        }
        
        // For now, return passthrough (TODO: implement full page table walk)
        // This is a placeholder implementation
        Ok(vaddr)
    }

    /// Extract VPN fields from virtual address
    pub fn extract_vpn(vaddr: u64, level: usize) -> u64 {
        const VPN_WIDTH: u32 = 9;
        const PAGE_OFFSET: u32 = 12;
        
        let shift = PAGE_OFFSET + (level as u32) * VPN_WIDTH;
        (vaddr >> shift) & ((1 << VPN_WIDTH) - 1)
    }

    /// Get page offset
    pub fn page_offset(vaddr: u64) -> u64 {
        vaddr & ((1 << 12) - 1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bare_mode() {
        let config = MmuConfig::default();
        let translator = AddressTranslator::new(config);
        let tlb = Tlb::new(64, 4);
        
        let request = TranslationRequest {
            vaddr: 0x8000_0000,
            access_type: AccessType::Read,
            privilege: PrivilegeMode::Machine,
            satp: 0, // Bare mode
            mstatus: 0,
        };
        
        let result = translator.translate(request, &tlb);
        assert_eq!(result.unwrap(), 0x8000_0000);
    }

    #[test]
    fn test_extract_vpn() {
        // VPN[2] = bits 30-38
        assert_eq!(AddressTranslator::extract_vpn(0x0000_0040_0000, 2), 1);
        // VPN[1] = bits 21-29
        assert_eq!(AddressTranslator::extract_vpn(0x0000_0020_0000, 1), 1);
        // VPN[0] = bits 12-20
        assert_eq!(AddressTranslator::extract_vpn(0x0000_0000_1000, 0), 1);
    }

    #[test]
    fn test_page_offset() {
        assert_eq!(AddressTranslator::page_offset(0x1234), 0x234);
        assert_eq!(AddressTranslator::page_offset(0xABCD_EF12), 0xF12);
    }
}
