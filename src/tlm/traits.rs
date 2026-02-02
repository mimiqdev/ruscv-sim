//! TLM2.0 接口 Traits
//!
//! 定义 TLM2.0 发起者(Initiator)和目标(Target)接口

use super::{TlmGenericPayload, TlmPhase, ScTime, TlmError, TlmSyncEnum};

/// TLM2.0 发起者接口 (Initiator)
/// 
/// 发起者负责发起 TLM 事务请求。实现此 trait 的组件可以主动发起读写操作。
/// 
/// # 示例
/// ```
/// use ruscv_sim::tlm::{TlmInitiator, TlmGenericPayload, TlmCommand, ScTime, TlmError};
/// 
/// struct MyInitiator;
/// 
/// impl TlmInitiator for MyInitiator {
///     fn b_transport(&self, trans: &mut TlmGenericPayload, delay: &mut ScTime) -> Result<(), TlmError> {
///         // 实现阻塞传输
///         Ok(())
///     }
/// }
/// ```
pub trait TlmInitiator: Send + Sync {
    /// 阻塞传输 (Blocking Transport)
    /// 
    /// 执行阻塞式的读写操作，直到操作完成才返回。
    /// 
    /// # 参数
    /// - `trans`: 事务载荷，包含命令、地址、数据等信息
    /// - `delay`: 时间延迟，用于模拟传输时间
    /// 
    /// # 返回值
    /// - `Ok(())`: 传输成功
    /// - `Err(TlmError)`: 传输失败
    fn b_transport(
        &self,
        trans: &mut TlmGenericPayload,
        delay: &mut ScTime,
    ) -> Result<(), TlmError>;

    /// 非阻塞前向传输 (Non-blocking Forward Transport)
    /// 
    /// 执行非阻塞的传输操作，立即返回状态。
    /// 
    /// # 参数
    /// - `trans`: 事务载荷
    /// - `phase`: 传输相位
    /// - `delay`: 时间延迟
    /// 
    /// # 返回值
    /// - `Ok(TlmSyncEnum)`: 同步状态
    /// - `Err(TlmError)`: 传输错误
    fn nb_transport_fw(
        &self,
        trans: &mut TlmGenericPayload,
        phase: &mut TlmPhase,
        delay: &mut ScTime,
    ) -> Result<TlmSyncEnum, TlmError> {
        // 默认实现：不支持非阻塞传输
        let _ = (trans, phase, delay);
        Err(TlmError::NotImplemented)
    }

    /// 获取直接内存接口 (DMI)
    /// 
    /// 如果支持，返回直接内存访问接口以提高性能。
    /// 
    /// # 参数
    /// - `trans`: 事务载荷，用于确定访问区域
    /// 
    /// # 返回值
    /// - `Some(DmiData)`: DMI 接口数据
    /// - `None`: 不支持 DMI
    fn get_direct_mem_ptr(&self, trans: &TlmGenericPayload) -> Option<DmiData> {
        let _ = trans;
        None
    }

    /// 传输调试信息
    /// 
    /// 用于调试目的，不实际执行传输。
    /// 
    /// # 参数
    /// - `trans`: 事务载荷
    /// 
    /// # 返回值
    /// - 传输成功时返回 true
    fn transport_dbg(&self, trans: &mut TlmGenericPayload) -> bool {
        let _ = trans;
        false
    }
}

/// TLM2.0 目标接口 (Target)
/// 
/// 目标负责响应 TLM 事务请求。实现此 trait 的组件可以接收和处理读写操作。
/// 
/// # 示例
/// ```
/// use ruscv_sim::tlm::{TlmTarget, TlmGenericPayload, ScTime, TlmError, AddressRange};
/// 
/// struct MyTarget;
/// 
/// impl TlmTarget for MyTarget {
///     fn b_transport(&mut self, trans: &mut TlmGenericPayload, delay: &mut ScTime) -> Result<(), TlmError> {
///         // 处理阻塞传输请求
///         Ok(())
///     }
///     
///     fn get_address_ranges(&self) -> Vec<AddressRange> {
///         vec![]
///     }
/// }
/// ```
pub trait TlmTarget: Send + Sync {
    /// 阻塞传输回调
    /// 
    /// 接收并处理来自发起者的阻塞传输请求。
    /// 
    /// # 参数
    /// - `trans`: 事务载荷
    /// - `delay`: 时间延迟
    /// 
    /// # 返回值
    /// - `Ok(())`: 处理成功
    /// - `Err(TlmError)`: 处理失败
    fn b_transport(
        &mut self,
        trans: &mut TlmGenericPayload,
        delay: &mut ScTime,
    ) -> Result<(), TlmError>;

    /// 非阻塞后向传输 (Non-blocking Backward Transport)
    /// 
    /// 用于目标向发起者返回响应。
    /// 
    /// # 参数
    /// - `trans`: 事务载荷
    /// - `phase`: 传输相位
    /// - `delay`: 时间延迟
    /// 
    /// # 返回值
    /// - `Ok(TlmSyncEnum)`: 同步状态
    /// - `Err(TlmError)`: 传输错误
    fn nb_transport_bw(
        &self,
        trans: &mut TlmGenericPayload,
        phase: &mut TlmPhase,
        delay: &mut ScTime,
    ) -> Result<TlmSyncEnum, TlmError> {
        let _ = (trans, phase, delay);
        Err(TlmError::NotImplemented)
    }

    /// 获取支持的地址范围
    /// 
    /// 返回此目标支持的地址范围列表。
    fn get_address_ranges(&self) -> Vec<AddressRange>;

    /// 检查地址是否在此目标范围内
    /// 
    /// # 参数
    /// - `address`: 要检查的地址
    /// 
    /// # 返回值
    /// - `true`: 地址在范围内
    /// - `false`: 地址不在范围内
    fn contains_address(&self, address: u64) -> bool {
        self.get_address_ranges()
            .iter()
            .any(|range| range.contains(address))
    }

    /// 获取直接内存接口 (DMI)
    /// 
    /// 如果支持，返回直接内存访问接口以提高性能。
    /// 
    /// # 参数
    /// - `trans`: 事务载荷，用于确定访问区域
    /// 
    /// # 返回值
    /// - `Some(DmiData)`: DMI 接口数据
    /// - `None`: 不支持 DMI
    fn get_direct_mem_ptr(&self, _trans: &TlmGenericPayload) -> Option<DmiData> {
        None
    }
}

/// DMI (Direct Memory Interface) 数据
/// 
/// 用于直接内存访问的信息结构
#[derive(Debug, Clone)]
pub struct DmiData {
    /// DMI 指针
    pub dmi_ptr: *mut u8,
    /// DMI 大小
    pub dmi_size: usize,
    /// 访问权限（读/写）
    pub access_rights: DmiAccessRights,
    /// 读延迟
    pub read_latency: ScTime,
    /// 写延迟
    pub write_latency: ScTime,
    /// 起始地址
    pub start_address: u64,
    /// 结束地址
    pub end_address: u64,
}

impl DmiData {
    /// 创建新的 DMI 数据
    pub fn new(
        dmi_ptr: *mut u8,
        dmi_size: usize,
        start_address: u64,
        end_address: u64,
    ) -> Self {
        Self {
            dmi_ptr,
            dmi_size,
            access_rights: DmiAccessRights::ReadWrite,
            read_latency: ScTime::from_nanoseconds(0),
            write_latency: ScTime::from_nanoseconds(0),
            start_address,
            end_address,
        }
    }

    /// 检查地址是否在 DMI 范围内
    pub fn contains(&self, address: u64) -> bool {
        address >= self.start_address && address <= self.end_address
    }

    /// 检查是否允许读访问
    pub fn allows_read(&self) -> bool {
        matches!(
            self.access_rights,
            DmiAccessRights::ReadOnly | DmiAccessRights::ReadWrite
        )
    }

    /// 检查是否允许写访问
    pub fn allows_write(&self) -> bool {
        matches!(
            self.access_rights,
            DmiAccessRights::WriteOnly | DmiAccessRights::ReadWrite
        )
    }
}

// 为 DmiData 实现安全 trait（因为是裸指针，需要手动保证安全）
unsafe impl Send for DmiData {}
unsafe impl Sync for DmiData {}

/// DMI 访问权限
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DmiAccessRights {
    /// 禁止访问
    NoAccess,
    /// 只读访问
    ReadOnly,
    /// 只写访问
    WriteOnly,
    /// 读写访问
    ReadWrite,
}

/// 地址范围
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AddressRange {
    /// 起始地址（包含）
    pub start: u64,
    /// 结束地址（包含）
    pub end: u64,
}

impl AddressRange {
    /// 创建新的地址范围
    /// 
    /// # 参数
    /// - `start`: 起始地址
    /// - `end`: 结束地址（包含）
    /// 
    /// # 示例
    /// ```
    /// use ruscv_sim::tlm::AddressRange;
    /// 
    /// let range = AddressRange::new(0x1000, 0x1FFF);
    /// assert!(range.contains(0x1500));
    /// assert!(!range.contains(0x2000));
    /// ```
    pub fn new(start: u64, end: u64) -> Self {
        Self { start, end }
    }

    /// 检查地址是否在范围内
    pub fn contains(&self, address: u64) -> bool {
        address >= self.start && address <= self.end
    }

    /// 获取范围大小
    pub fn size(&self) -> u64 {
        self.end.saturating_sub(self.start).saturating_add(1)
    }

    /// 检查两个范围是否重叠
    pub fn overlaps(&self, other: &AddressRange) -> bool {
        self.start <= other.end && other.start <= self.end
    }
}

/// 通用 TLM 接口（简化版）
/// 
/// 提供简化的读写接口，用于不需要完整 TLM2.0 功能的场景
pub trait TlmInterface: Send + Sync {
    /// 读操作（阻塞）
    /// 
    /// # 参数
    /// - `addr`: 读取地址
    /// - `size`: 读取字节数
    /// 
    /// # 返回值
    /// - `Ok(Vec<u8>)`: 读取的数据
    /// - `Err(TlmError)`: 读取失败
    fn read(&self, addr: u64, size: usize) -> Result<Vec<u8>, TlmError>;

    /// 写操作（阻塞）
    /// 
    /// # 参数
    /// - `addr`: 写入地址
    /// - `data`: 要写入的数据
    /// 
    /// # 返回值
    /// - `Ok(())`: 写入成功
    /// - `Err(TlmError)`: 写入失败
    fn write(&mut self, addr: u64, data: &[u8]) -> Result<(), TlmError>;

    /// 获取延迟
    fn get_delay(&self) -> ScTime;

    /// 设置延迟
    fn set_delay(&mut self, delay: ScTime);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tlm::{TlmCommand, TlmResponseStatus};

    struct TestInitiator;
    impl TlmInitiator for TestInitiator {
        fn b_transport(
            &self,
            trans: &mut TlmGenericPayload,
            _delay: &mut ScTime,
        ) -> Result<(), TlmError> {
            trans.set_response_status(TlmResponseStatus::Ok);
            Ok(())
        }
    }

    struct TestTarget {
        memory: Vec<u8>,
        base_addr: u64,
    }

    impl TestTarget {
        fn new(size: usize, base_addr: u64) -> Self {
            Self {
                memory: vec![0; size],
                base_addr,
            }
        }
    }

    impl TlmTarget for TestTarget {
        fn b_transport(
            &mut self,
            trans: &mut TlmGenericPayload,
            _delay: &mut ScTime,
        ) -> Result<(), TlmError> {
            let addr = trans.address() - self.base_addr;
            
            match trans.command() {
                TlmCommand::Read => {
                    for i in 0..trans.data_length() {
                        trans.data_mut()[i] = self.memory[addr as usize + i];
                    }
                }
                TlmCommand::Write => {
                    for i in 0..trans.data_length() {
                        self.memory[addr as usize + i] = trans.data()[i];
                    }
                }
            }
            
            trans.set_response_status(TlmResponseStatus::Ok);
            Ok(())
        }

        fn get_address_ranges(&self) -> Vec<AddressRange> {
            vec![AddressRange::new(
                self.base_addr,
                self.base_addr + self.memory.len() as u64 - 1,
            )]
        }
    }

    #[test]
    fn test_initiator_b_transport() {
        let initiator = TestInitiator;
        let mut trans = TlmGenericPayload::new(TlmCommand::Read, 0x1000, 4);
        let mut delay = ScTime::zero();

        assert!(initiator.b_transport(&mut trans, &mut delay).is_ok());
        assert!(trans.is_response_ok());
    }

    #[test]
    fn test_target_b_transport() {
        let mut target = TestTarget::new(1024, 0x1000);
        
        // 测试写操作
        let mut write_trans = TlmGenericPayload::with_data(
            TlmCommand::Write,
            0x1000,
            vec![0x01, 0x02, 0x03, 0x04],
        );
        let mut delay = ScTime::zero();
        
        assert!(target.b_transport(&mut write_trans, &mut delay).is_ok());

        // 测试读操作
        let mut read_trans = TlmGenericPayload::new(TlmCommand::Read, 0x1000, 4);
        assert!(target.b_transport(&mut read_trans, &mut delay).is_ok());
        assert_eq!(read_trans.data(), &[0x01, 0x02, 0x03, 0x04]);
    }

    #[test]
    fn test_address_range() {
        let range = AddressRange::new(0x1000, 0x1FFF);
        
        assert!(range.contains(0x1000));
        assert!(range.contains(0x1FFF));
        assert!(range.contains(0x1500));
        assert!(!range.contains(0x0FFF));
        assert!(!range.contains(0x2000));
        
        assert_eq!(range.size(), 0x1000); // 4096 bytes
    }

    #[test]
    fn test_address_range_overlap() {
        let range1 = AddressRange::new(0x1000, 0x1FFF);
        let range2 = AddressRange::new(0x1800, 0x2800);
        let range3 = AddressRange::new(0x2000, 0x3000);
        
        assert!(range1.overlaps(&range2));
        assert!(range2.overlaps(&range1));
        assert!(!range1.overlaps(&range3));
        assert!(!range3.overlaps(&range1));
    }

    #[test]
    fn test_dmi_data() {
        let mut data = vec![0u8; 1024];
        let dmi = DmiData::new(
            data.as_mut_ptr(),
            data.len(),
            0x1000,
            0x13FF,
        );
        
        assert!(dmi.contains(0x1000));
        assert!(dmi.contains(0x13FF));
        assert!(!dmi.contains(0x1400));
        
        assert!(dmi.allows_read());
        assert!(dmi.allows_write());
    }

    #[test]
    fn test_target_address_check() {
        let target = TestTarget::new(1024, 0x1000);
        
        assert!(target.contains_address(0x1000));
        assert!(target.contains_address(0x13FF));
        assert!(!target.contains_address(0x0FFF));
        assert!(!target.contains_address(0x1400));
    }
}
