# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- **Quantum U1: 本地时间与阈值（issue #201）**
  - `MachineConfig` 新增 `pub(crate)` 字段 `quantum`（Platform tick；`MachineConfig::new` 选择 0），该新增字段
    使外部代码无法再以穷举结构体字面量构造 `MachineConfig`。U2 完成前该字段保持 crate 私有，因此没有公开
    调用方可以选择尚未完整同步的 quantum。非零选择要求 `Transport::Tlm`：与 `Transport::Native` 的组合是既有
    的 `MachineError::UnsupportedTransport`，该变体的消息加宽为命名 quantum 情形；
    `(Q + t + 1) × tick_period` 无法以 u64 飞秒表示时，构建以 `MachineError::Placement` 拒绝，`Q` = 0 不运行
    该检查
  - `SystemBus` 在 CLINT 旁、同一把总线锁下保存本地偏移 `O` 与累计同步计数；`L = C + O`（回绕加法）。
    每个 turn 起点与 `publish_platform_time` 通过 `csr.set_time` 呈现 `L`，中断线仍由 `C` 派生。
    一次 sync 把 `C` 设为 `L`、清零 `O`，只在移动了 `C` 时计数；它不做 target 调用、不计延迟、不改任何
    架构或延迟计数器
  - 唯一累积路径（D4）：turn 的 `advance = delay.ticks + completed_turn` 在 quantum 开启时进入 `O` 而非
    `C`；无延迟 tick 的 turn 回绕，带延迟 tick 的 turn 做检查，检查失败沿用今天的 simulator failure 并
    保留已完成事实，该 turn 的 advance 既不进入 `C` 也不进入 `O`。阈值（D5）在推进后先测 `O >= Q`，
    达阈值即在边界 sync；`Q` = 1 每个边界同步，`Q` = 0 完全走原路径
  - U1 拥有的强制同步点：F3（等待中在 `settle_wait` 之前同步，唤醒与 parity 一致）、F5（single step
    边界）、F6（失败 turn 含已知延迟的同步）、F10（零延迟 turn 使 `L` 回绕后同步，MTIP 不会滞后为高）
  - `BoundaryFacts` 与 `ExecutionResult` 新增 `quantum: Option<QuantumFacts>`（`ticks`、
    `interrupt_latency_bound_ticks`、`reduced_accuracy`、`syncs`、`reached`）；`ControlBoundary` 上报
    `reached` 为 false。新增 `QuantumFacts` 类型，`ExecutionResult` 的新字段破坏外部穷举结构体字面量。
    `print_result` 仅在该事实存在时输出一行 `Quantum:`，`Q` = 0 的 stdout 保持逐字节相同
  - 新增 12 个 Machine 级测试覆盖合同 §4 U1 的验收项；U1 拥有的 19 条负控制（20 条测试行）注册进
    `scripts/debug/stage6-mutations`（`quantum-u1-*`，含四个逐点控制、逐边界计数、D5 与强制同步重合、
    D10 误报、延迟重复、同步偏差与计数器记账），每条都由已提交测试在变异下失败、还原后通过

- **Quantum U2: 强制同步与延迟上界（issue #202）**
  - `MachineConfig` 的 `quantum` 字段改为公开（`MachineConfig::new` 仍选择 0），合同要求该字段在 U2 的同步点
    完成后再公开，因此现在公开调用方可以选择 quantum；其余公开签名不变，`run --quantum`、guest suite、C ABI
    与 SystemC smoke 仍分别属于 U3、U4、U5
  - `SystemBus` 在 CLINT 旁、同一把总线锁下保存锚点 `E`（quantum D3）：install 时 `E` = `C`，每次 sync、空闲
    跳转、guest 或 debug 的 CLINT `mtime` 写、crate 内 `set_platform_mtime`、以及 `Q` = 0 时
    `advance_mtime`/`advance_mtime_by` 的直接推进都重设它；flush 不重设，所以 `L - E` 在 flush 后仍是完整的
    累积偏移（D11）。真正的一次边界 sync 只在移动了 `C`、或该 turn 内有 flush 移动了 `C` 时计入一次；两次都
    没移动的边界是 no-op，不计入
  - F1：`SystemBus::native_transact_from` 的 Hart 设备窗口（CLINT、PLIC、gateway、UART、HTIF，含设备拒绝的
    访问）与 `native_transact_atomic` 的设备分支在同一把总线锁下、设备看到访问之前 flush，并标记该 turn；本
    turn 的延迟仍在 pending，因此设备看到 parity 的 `mtime`。被标记的 turn 即使 flush 时 `O` 为零也在边界
    sync，sync 清除标记；debug 工作经 Hart 端口产生的标记在 `begin_transport_turn` 清除，`Platform`
    `begin_transport_turn` 因此返回 `Result<(), MachineError>`。未映射地址不是设备，`DebugInbound` 属 F4
  - F2：新增 `OwnedMachine::sync_after_tohost(&mut self, turn: &mut MachineTurn)`，在 Runner 的 `observe_ram`
    与 `Session::observe_exit` 读取到非零 RAM `tohost` 之后、共享退出规则解码之前 sync，并刷新
    `turn.boundary.quantum.syncs`；零值轮询不是 sync
  - F4：`DebugHalted`（两种形式）在 turn 结束 sync；`apply_dmi` 的排队批次、`advance_debug_command` 的忙碌
    命令与 `service_sba` 取到暂存请求时，都在 `sample_debug_inputs` 之前 sync，所以调试器与 SBA 看到 parity
    时间，debug 工作不进入 `O`
  - F7：`OwnedMachine::step_request` 的 `last_slot` sync 并刷新边界计数；`cli_result` 在报告事实之前经总线锁
    完成一次 sync，使最终结果携带 `C` = `L`，总线锁中毒时以 `; ` 追加到运行错误
  - F8：`acknowledge_drain` 在采样前提交偏移（仅总线锁更新，省略 Hart CSR 重发布），按 D3 计数并重设 `E`；
    F9 不变：reset 不运行 sync，新 Platform 以 `C` = `O` = `E` = 0 开始，不跨代继承偏移
  - `Installed` 新增 `#[cfg(test)]` 时间账本（D9）：记录每个 turn 的已计费 tick、完成 tick 与每次空闲跳转
    距离，`mtime` 写作为 rebase，测试以 `OwnedMachine::platform_mtime` 在 tick 上核对守恒恒等式
  - 新增 19 个 Machine 级、1 个 C ABI `Session` 级与 1 个 Runner 级测试覆盖合同 §4 U2 的验收项，包括量子中途的 CLINT `mtime` 读返回
    parity 值、`mtimecmp` 等于 `C` 加该 turn `advance` 的下一个 turn 呈现 MTIP、运行中与停机下的 SBA
    `mtime` 读、guest 下调 `mtime`、SBA 与 abstract command 的 debug `mtime` 写后的 `E` = `C` = `L` 与标记
    清除、flush-only 失败边界按新提交时间重新发布中断线、`last_slot` 与 Runner 返回的计数、drain、
    `OwnedMachine` 与 `Session` 两条新代路径无偏移，以及
    `quantum_timer_spin`（手编 guest 经 CLINT 装载 `mtimecmp` 并置 MTIE 自旋）：`Q` = 0 与 `Q` = 1 的 PC、
    `time` 与 MTIP 逐 turn 等于 native 路由，`Q` = 16 的识别延迟按 guest 的 F1 同步相位与 `Q` 算出并逐值
    相等（6 tick，在 D8 内），运行经 F7 的 `last_slot` 同步收尾，最终 `platform_mtime` 满足 D9

- **Quantum U4: C ABI（issue #204）**
  - `BlockingTransaction` 新增只读请求访问器 `local_offset() -> Femtoseconds`（quantum D11）：Hart 事务的
    `L - E`（回绕减法）加上该 turn 已计入的延迟 tick，再乘以 tick 周期。以 `E` 而非 `O` 计量，所以 F1 flush
    把 `O` 移入 `C` 后该值仍是完整累积偏移；`Q` = 0、`DebugInbound` 与 debug transport 一律为零。它是时序
    元数据，地址、字节、状态与权限从不依赖它，`set_response_delay` 的含义与 D4 的记账路径都不变
  - 同一把总线锁下的偏移经 `RamSlot` 传给适配器：`SystemBus::native_transact_from` 的 Hart RAM 分支与
    `native_transact_atomic` 传入 `turn_local_base()`，适配器加上本 turn 已计费的 tick 后写入事务。
    `BlockingTransaction::set_local_offset` 为 crate 内可见，target 无法通过复制载荷改写它
  - `include/ruscv_sim.h`：`ruscv_target` 追加 `uint64_t quantum_ticks`（零为关闭；`NULL` target 选择 native
    路由，因而无法选择 quantum；table 在 create 时复制，值跨 `ruscv_machine_reset` 保持）、`ruscv_payload`
    追加 `uint64_t local_offset_fs` 请求字段、`ruscv_boundary` 追加 `quantum_ticks`、
    `interrupt_latency_bound_ticks`、`quantum_syncs` 与 `int32_t quantum_reached`。三处布局 pin 同步更新
    （`the_direct_memory_fields_are_final_in_their_structs`、`tests/capi/smoke.c` 的布局检查、header 的
    Layout rule 与 final field 文本）：`ruscv_target` 48 → 56 字节，`ruscv_payload` 248 → 256 字节
  - `ruscv_machine_create` 在 `host::validate_target` 旁调用新的 `host::validate_quantum`，在 Machine 于 load
    时构建之前执行同一套 D2 规则，被拒绝的值返回 `RUSCV_ERROR_CONFIG`
  - `GrantFacts` 新增四个 `ruscv_boundary` 量子字段与 `quantum_sync`；`boundary()` 照抄这四个字段，
    `boundary_reached()` 包含 `quantum_sync`。`quantum_syncs` 是 D10 的累计 `syncs`，在 grant 返回同步之后
    读取；load 或 reset 后的第一次 grant 之前仍是 `GrantFacts::default()` 的零值
  - grant 返回路径：新增 `Session::grant_end_fs` 作为唯一的 grant 结束时间来源，先同步再读 `C`，所以
    `committed_end_fs` 与 `consumed_fs` 都是同步后的值；`Q` 非零时一次被计数的 turn 边界 sync 结束 grant
    （`boundary_reached` 为 1），空闲跳转在虚拟截止设置时先于唤醒 turn 结束 grant（`waiting` 为 1、
    `deadline_blocked` 为 0）。`Installed::turn` 在 `reached_virtual_deadline` 与 `wake_turn_fits` 之后加该
    判定；Runner 不设虚拟截止，仍把跳转与唤醒放在同一个 `turn()` 内
  - grant 的 horizon、`deadline_blocked` 与 `residual_slack_fs` 测试改用 `L`（quantum D8/D12）：每次 grant
    返回都先同步，所以调用方的已提交时钟结束时等于 `L`，按 `C` 授予 turn 会让该时钟越过 horizon。新增测试
    证明非零 `O` 下档位内的 grant 满足 `committed_end_fs <= max_end_fs`，且 `deadline_blocked` 在与 parity
    路由同一 turn 上报告
  - 新增 10 个 C ABI 级 Rust 测试（含手编的 `wfi` 唤醒 guest 与经 CLINT 常驻 PTE 的 Sv39 walk guest）与
    plain-C smoke 的对应用例：`Q` = 0/1/4 的事实与声明上界、`local_offset_fs` 的 turn 序列（关闭与 `Q` = 1
    时为零）、`committed_end_fs` 恰为 `committed_start_fs + consumed_fs`、阈值与强制同步重合、强制同步单独
    结束 grant、空闲跳转结束 grant 且唤醒 turn 首个事务的本地时间等于跳转目标、Sv39 walk 在 flush 后发布
    `L - E` 加已计费 tick、每事务延迟只计一次、非零 `O` 的 unknown completion 之后 reset 恢复所选 quantum、
    `RUSCV_ERROR_CONFIG`，以及无 quantum 的 `NULL` target 仍走 native
  - U4 拥有的两条负控制（`quantum-u4-f7-grant-return`、`quantum-u4-horizon-uses-committed`）注册进
    `scripts/debug/stage6-mutations`，每条都由已提交测试在变异下失败、还原后通过
- **Quantum U3: CLI 与 guest parity（issue #203）**
  - `run --quantum <ticks>` 把 `MachineConfig.quantum` 接到公共入口：选项可选，未给出时不设置 quantum；
    给出时按 D2 要求 `--transport tlm`，无论取值（含 `--quantum 0`）不带该传输都是 CLI 错误并以状态 2
    退出。`--quantum` 与 `--dmi`、`--rbb-port` 组合仍走同一条 `load_and_run_file_with_config`；`--verbose`
    另打印所选 tick。帮助文本列出该选项及其 TLM 要求。选中 quantum 时结果块多一行 `Quantum:`（U1 已加入
    的 D10 报告），`Q` = 0 的 stdout 逐字节不变
  - 新增 `tests/quantum_cli.rs`：状态 2 拒绝（含 `--quantum 0`）、`Q` = 0 与 `Q` = 1 的报告除 `Quantum:`
    行外与 native 逐字节相同（`Q` = 1 的上界为 0、无 `reduced accuracy`），以及 `--transport tlm --dmi
    --quantum 1024` 的 RAM-only 运行报告 `syncs` 低于 `Cycles` 且带 D8 上界
  - `scripts/run_elf_tests.sh` 对每个 guest 增加 `--transport tlm --quantum 1` 与 `--transport tlm
    --quantum 1024` 两遍（各用其声明的 profile 与同一 `--max-cycles`）：parity set 两遍的 commit log 均须与
    native 逐字节相同，其余 guest 保持退出码，且汇总在任一遍覆盖不足时失败。`hello` 另跑一次
    `--transport tlm --dmi --quantum 1024`，要求 `Quantum:` 行的 `syncs` 严格低于 `Cycles:` 并带
    `latency bound 1023 ticks`；没有 guest 需要按名字列出的时间派生例外
  - 新增 guest `tests/bare-metal-riscv-test/rv64i/quantum_u3_timer.S`：经 CLINT 装载 `mtimecmp` 并置 MTIE
    自旋，使 MTIP 只由时间到达 compare 引起；陷入处理记录 `mcause`、`mepc`、识别次数与 D8 延迟，未接入调试器
    时自行以 tohost 退出，因此它也作为普通 suite guest 在五遍上运行
  - `scripts/debug_attach.sh` 新增一条 `--transport tlm --quantum 1024` 实时 attach 会话（
    `scripts/debug/quantum-u3-session.gdb`）：强制 system-bus 访问，在初始停机与量子中途的 spin 停机各读一次
    `mtime`（OpenOCD 日志须有 `mtime` 的 system-bus 读），resume 经过定时器陷入后读取 guest 记录的事实。
    会话断言 `mcause` 为 machine timer、`mepc` 为自旋点、识别恰一次、延迟在 `Q - 1 + t` 内，并不断言某个精确
    延迟（D6：调试器的 F4 同步，包括 DMI poll 引起的同步，会移动阈值同步的相位）。这些事实检查放在
    `scripts/debug/quantum_session_facts.sh`：超出 bash 有符号 64-bit 的 64-bit 值（例如相减得到的
    `0xffffffffffffffff`）被拒绝，而不是读成负的小数后通过 `> 上界` 检查
  - `scripts/test_quantum_session_facts.sh` 直接以构造的事实文件覆盖上述检查的接受与拒绝路径（含上述回绕
    延迟的负控制，缺失或空字段按非法值拒绝而不是读成 0），无需 OpenOCD、GDB 或 RISC-V 工具链；CI 的
    guest-script 步骤运行它。`run_elf_tests.sh` 的 `--dmi --quantum 1024` 行要求 `Quantum:` 与 `Cycles:` 各
    恰好一行、两个字段都是不超过 18 位的十进制数，并在解析或比较失败时判该行失败，而不是让 shell 错误落成
    通过条件；`quantum_u3_timer.elf` 同时是 suite 的必需成员，缺失时 suite 以状态 2 失败而不是静默缩小
  - 新增变异控制 `quantum-u3-cli-requires-tlm`（`src/main.rs`）：去掉 CLI 的 `--quantum` 传输检查后，
    `tests/quantum_cli.rs::the_quantum_option_requires_the_tlm_transport` 必须以非状态 2 的结果失败，还原后
    通过

- **Quantum U5: SystemC smoke（issue #205）**
  - `integration/systemc` 的 `RamTarget::b_transport` 不再把 `delay` 赋为 `SC_ZERO_TIME`，而是原样保留该参数：
    它到达时是 Hart 的本地偏移（`ruscv_payload.local_offset_fs`，精确飞秒），这个零延迟模型不添加任何延迟，
    所以 quantum 开启与关闭的 committed time 逐值相同（quantum D13）
  - initiator 把 `payload.local_offset_fs` 作为 `b_transport` 的 `delay` 实参传入，返回时只上报 `delay - offset`
    作为新增延迟；返回的 `delay` 低于 offset 时无法表示为新增延迟，且 target 可能已提交写，因此经既有
    `mark_unknown_completion` 上报并设置 `outcome_unknown`（unknown completion，而不是普通
    `TLM_INCOMPLETE_RESPONSE`）。target 表新增的 `quantum_ticks` 由 `--quantum <ticks>` 选择（零为关闭）
  - `systemc_smoke` 新增 `--quantum <ticks>` 与合同 §4 的两个故障选项：`--fault-keep-offset`（initiator 把
    完整 `delay` 当作新增延迟上报，超出 target 声明的零上限）与 `--fault-decrease-delay`（`RamTarget` 把
    `delay` 降到收到的 offset 之下）；两者都要求 `--quantum`，都不会静默落成通过。每次 grant 后打印内核时钟
    与 `committed_end_fs` 的比较次数，失败结果另打印边界的 `uncertain`，使两条故障断言各自指向一个具名分类
  - `scripts/test_systemc_smoke.sh` 对两个 guest 各增加 `--quantum 1` 与 `--quantum 1024`、带与不带 `--dmi`
    的四遍：每遍退出码与 commit log 与 native 逐字节一致，内核时钟比较次数恰等于 grant 数，且 `--quantum
    1024` 的 grant 数严格低于同一 guest、同一 `--dmi` 状态的 `--quantum 1`（`atomic_tohost_exit` 1 对 4、
    `fd_fma` 1 对 132）。既有各遍与早停控制不变；现有 grant 上限（10000）已足够覆盖 `--quantum 1` 的逐 turn
    grant，因此未改动
  - 两个故障控制各在 `--quantum 1024` 下运行，并要求 `result state=failed` 加各自具名诊断：
    `--fault-keep-offset` 须命中 `above its declared per-transaction maximum of 0 fs` 且 `uncertain=0`
    （host failure），`--fault-decrease-delay` 须命中 `could not establish the outcome of` 且 `uncertain=1`
    （unknown completion）。变异 runner 只构建 Rust，所以按 D14 这两条是 smoke 二进制的选项、不是变异行；
    其失败证据由上述断言在每次 smoke 运行中重新产生
  - `docs/integration/systemc-tlm.md` 按 quantum D14 成为本 host 偏移、grant、sync 与上界规则的规范说明：
    `b_transport` 的 delay 实参、`delay - offset` 上报规则与低于 offset 的 unknown completion、grant 返回的
    F7 同步与时钟等式、horizon 使用本地时间（零 overshoot）、两个故障控制及其断言；开放项只余非阻塞传输。
    `integration/systemc/README.md` 同步列出新选项

- **Direct memory U1: 授权与读取路径（issue #188）**
  - `MachineConfig` 新增公开 `dmi` 字段（`MachineConfig::new` 选择 `false`，破坏结构体字面量构造）；
    `dmi` 与 `Transport::Native` 的组合是既有的 `MachineError::UnsupportedTransport`，该变体的消息加宽为
    同时命名 `Flat` + `tlm` 与本组合；`Flat` 规则不变。`run --dmi` 暴露该选择，缺少 `--transport tlm` 时是
    CLI 错误（状态 2）。`load_and_run_file_with_config` 按字段生效，`RiscVSimulator` 与其余 `load_and_run*`
    facade 仍走 native
  - `src/blocking` 新增 TLM Direct Memory Interface 词表：`BlockingDirectMemAccess`、
    `BlockingDirectMemQuery`、`unsafe fn` 构造的 `BlockingDirectMemGrant`（D10 义务写在 `# Safety` 中）、
    `BlockingDirectMemResponse`（granted/denied/failed）、持 adapter 弱引用的
    `BlockingDirectMemInvalidator`、公开 `BlockingDirectMemRegistry`（追加而不替换注册、可 prune 已释放的
    adapter、含 invalidate-all），以及 `BlockingTransaction` 上的 IEEE 命名 `dmi_allowed` 响应 hint。
    `BlockingTarget` 新增 `get_direct_mem_ptr` 与 `register_direct_mem_invalidator`，两者都有默认实现且不提供
    任何能力，既有实现者无需改动
  - `TlmRamAdapter` 在既有 RAM slot 内实现直接内存读路径：hint 触发的单次查询、grant 形状校验（空指针、
    倒置或溢出的区间、无授权、不含被查询地址、读或写延迟超过 target 声明的事务最大值均为被查询访问的
    protocol failure）、8 个 grant 的有界 region 缓存（新 grant 替换重叠项、最旧项被逐出）、Hart 取指与 load
    经由指针的 relaxed 逐字节原子拷贝。拷贝与查找在同一把专门 direct-memory 锁内完成，该锁不跨 target 调用，
    因此 `b_transport` 内同步到达的 invalidation 是安全的；查询开始后到达的 invalidation 会丢弃待安装的
    grant。读取延迟经既有的一处延迟记账路径（同一换算、同一 debug 不计时规则、同一边界总量），不进
    `mcycle`/`minstret`。store、原子包络与 `DebugInbound` 仍走 `b_transport`，写路径、D7 门控与 A/D 标记属
    U2
  - `ZeroDelayRam` 的 chunk 字节改为 `AtomicU8`（`bytes_at` 签名不变），在完成的非原子事务上设置 hint，
    为包含被查询地址的单个 4 KiB chunk 授予只读零延迟 grant，在 storage 锁内先 invalidate 全部注册再丢弃
    chunk，并在 transport reset 之间保留 committed-write 版本时钟（版本不重复）
  - 新增 17 个适配器级与 6 个 Machine 级测试覆盖合同 §4 U1 的验收项；合同 §4 的 14 条 U1 负控制注册进
    `scripts/debug/stage6-mutations`（`direct-mem-u1-*`），每条都指向必须失败的已提交测试

- **Direct memory U2: 写路径与原子操作（issue #189）**
  - `TlmRamAdapter` 在同一个 region 缓存内实现直接写路径：整段落在一个可写缓存 grant 内、且写挂起门关闭的
    Hart store 经由 host 指针完成（relaxed 逐字节原子 `AtomicU8` 存储），不再调用 `b_transport`；跨 region
    边界或缺少写权限的 store 仍是一次 blocking 事务，因此指向只读 region 的 store 到达 target，其响应上的
    hint 以 `DMI_ACCESS_WRITE` 查询，target 可改授可读写 grant 并替换旧 region。直接 store 的写入延迟走既有
    的一处延迟记账路径（同一换算、同一 debug 不计时规则、同一边界总量），不计入 `mcycle`/`minstret`，并在
    返回前计入适配器的 committed-write 记录
  - 实现 direct-memory D7 的写挂起门：guest LR 打开它，此后每个 store 都取 `b_transport`，使 target 记录该
    写入；只有结论性的 guest SC（成功或条件失败）关闭它，faulted 或未决的 SC 保持打开（Hart 仍持有预留），
    transport reset 关闭它。LR、SC、每个 AMO 与每一段 A/D 包络始终各是一次 `b_transport`，从不经由指针
  - `AtomicRequest` 新增 `pub(crate)` 页表更新标记，`src/core/translation.rs` 与 `src/mmu/sv39.rs` 两条
    walker 路由的 A/D LR 与 SC 都设置它，使 walk 私有的预留既不打开也不关闭写挂起门
  - `ZeroDelayRam` 改为授予包含被查询地址的单个 4 KiB chunk 可读写零延迟 grant；chunk 字节本就是
    `AtomicU8`，直接 store 与 `b_transport`、debug transport 看到同一份数据，invalidate-before-discard 与
    非重复版本时钟不变
  - 新增 11 个适配器级与 9 个 Machine 级测试覆盖合同 §4 U2 的验收项，另有两个既有测试扩展到写路径
    （direct-memory 锁屏障与 chunk grant 形状），包括 U1 延后的完整形式（guest 在直接 store 之后取得预留、
    跨失败的 recomposition 保留它，此时 SC 结果与 `dmi` 关闭时相同）；合同 §4 的 U2 负控制注册进
    `scripts/debug/stage6-mutations`（`direct-mem-u2-*`），每条都指向必须失败的已提交测试

- **Direct memory U3: 交互与 guest parity（issue #190）**
  - `scripts/run_elf_tests.sh` 对每个 guest 追加第三次运行：在 `--transport tlm --dmi` 下使用其声明的
    profile 与同一 cycle 上限，并像另外两遍一样分类失败（失败即缺陷，绝不跳过）。parity set（U4 的五个
    guest 加上本单元新增的两个）的第三次 commit log（`<guest>.direct.log`）必须与 native 逐字节相同，
    摘要同时报告两种 parity 比较的覆盖数
  - 新增两个必选 guest：`direct_mem_smc_fence_i.S` 在自己的指令流上打补丁（store → `FENCE.I` → 执行被替换
    的指令），`direct_mem_sv39_pt_store.S` 用直接 store 改写叶 PTE、执行 `sfence.vma`、再通过重映射的 VA
    读取；两者都参与 fresh suite 的三遍运行
  - `tests/direct_mem_u3_guests.rs` 用记录全部 `b_transport` 的 `ZeroDelayRam` 包装跑这两个镜像：开启直接
    内存时，代码页上的 store 与 `FENCE.I` 之后对被补丁字的取指、页表 store 与 `sfence.vma` 之后的 walk
    都不产生任何事务（每个 4 KiB chunk 只允许冷访问那一次）；关闭直接内存的同一镜像必须记录这些访问本身，
    因此该证明既不会因“未走该路径”而空过，也不依赖调度
  - `tests/tlm_u4_guest_parity.rs` 扩展为每 guest 三遍，断言第三遍确实带 `--dmi`，并注入仅在直接内存那一遍
    发生的空白差异（`DIRECT_DIVERGES`），证明该差异会被 suite 检出且不波及 TLM 那一遍；脚本按合同比较
    native 与第三遍，而在 suite 已先要求 TLM 与 native 一致的前提下，这与比较 TLM 等价，故没有 fixture
    能区分这两种比较目标
  - `scripts/debug_attach.sh` 增加一条 `run --transport tlm --dmi` 会话，强制 OpenOCD 只用 system bus：
    读取 `s6_break` 处的字、写入 EBREAK、resume 命中、用 raw system-bus 访问回读植入字、恢复原字并再次
    执行它直到 tohost 退出。读回必须绕过 GDB 自己的软件断点影子字节，插入与移除必须是恰好两次 OpenOCD
    system-bus 写
  - `machine::tests::sba_access_stays_counted_and_each_route_sees_the_other` 证明直接内存开启时 SBA 读与写
    各仍是一次计数事务、直接 load 与 store 读取对方写入的字节、host view（debug transport）看到直接 store；
    native 结果、A10 与既有 named 结果不变
  - 合同 §4 的 U3 负控制注册进 `scripts/debug/stage6-mutations`（`direct-mem-u3-*`）：取指绕开直接路径、
    直接 store 未落到 target、walk 读到过期 PTE、第三遍漏掉 `--dmi`，每条都指向必须失败的已提交测试

- **Direct memory U4: C ABI（issue #191）**
  - `ruscv_target` 末尾新增可空 `get_direct_mem_ptr`：`NULL` 表示 target 不提供直接内存，行为与 `dmi` 关闭
    完全一致；非 `NULL` 选择直接内存，`ruscv_machine_create` 的签名因此不变。`ruscv_payload` 末尾新增
    `dmi_allowed` 响应 hint。新增 `ruscv_direct_mem`、`RUSCV_DMI_ACCESS_*` 与
    granted／denied／failed 三种答案；`FAILED` 是 D3 的包含异常情形（未知完成并毒化 transport），不是
    denial。头文件在 `ruscv_direct_mem` 上完整陈述 D10 义务：指针寿命、别名、逐字节原子访问与
    invalidate-before-free 顺序
  - 新增 `ruscv_machine_invalidate_direct_mem(machine, start, end)`：C ABI 唯一允许在 target 回调内、以及
    在被毒化句柄上调用的入口，仍限创建线程。它只访问 Rust target wrapper 从 create 起持有、位于 handle
    run-state 锁之外、并跨 reset 的 Machine 替换保留的 `BlockingDirectMemRegistry`，因此不会在回调内
    死锁，也不会因句柄毒化而失败；其它入口仍拒绝回调与毒化句柄。target 的 `transport_reset` 在调用 C
    reset 之前 invalidate 全部注册（D10/D13）
  - 头文件记录布局规则：已发布结构体没有 size 或 version 字段，新字段只追加在末尾
    （`get_direct_mem_ptr`、`dmi_allowed`），既有字段偏移不变；用旧头文件编译的宿主必须重新编译
  - `scripts/test_capi_smoke.sh` 的普通 C smoke 与 Rust 测试覆盖 grant、NULL 回调、denial、grant 之间的
    invalidation 与回调内 invalidation、跨线程 invalidation 拒绝、唯一的再入例外、reset，以及直接内存
    开启时的未知完成与 reset 后 grant 恢复；新增两个 `direct-mem-u4-*` 负控制

- **Direct memory U5: SystemC smoke（issue #192）**
  - `integration/systemc` 的 `RamTarget` 与 initiator 获得直接内存：chunk 字节改为
    `std::atomic<uint8_t>`（`b_transport`、`transport_dbg` 与授权指针访问同一份字节，满足头文件的
    D10 逐字节原子要求），完成的非原子事务设置 `dmi_allowed` hint，`register_get_direct_mem_ptr`
    将包含被查询地址的单个 4 KiB chunk 以可读写、零延迟 grant 出；`transport_reset` 先经
    `invalidate_direct_mem_ptr` invalidate 全部注册再丢弃 chunk，并在 reset 间保留 committed-write
    版本时钟。initiator 的 backward `invalidate_direct_mem_ptr` 转发到
    `ruscv_machine_invalidate_direct_mem`——C ABI 唯一允许在回调内调用的入口
  - `systemc_smoke` 新增 `--dmi`（target 表提供 `get_direct_mem_ptr`，否则为 `NULL`、与 `dmi` 关闭一致）
    与 `--dmi-invalidate-every <grants>`（要求 `--dmi`，第 N 个 grant 后由 target 发起全空间
    invalidation）；退出时报告 `b_transport` 计数与转发的 invalidation 计数
  - `scripts/test_systemc_smoke.sh` 对两个 guest 各加一遍 `--dmi`：退出码与 commit log 与 native
    逐字节一致，且 `b_transport` 计数必须严格低于同一 guest 的非直接遍（从未使用 grant 指针的
    运行会失败）；再对 `atomic_tohost_exit` 运行 `--dmi-invalidate-every 2`，要求 commit log 不变、
    计划计数恰等于 grants/2 且转发的计数不低于它。既有各遍与早停控制不变。C++ 检查不在 Rust mutation runner
    覆盖内，其失败证据记录在 PR 中，未新增控制

- **Direct memory U6: 累积验证与收尾（issue #193）**
  - 合同 §4 的负控制审计：十五条全部已在 mutation runner 注册为 33 个 `direct-mem-u1-*`/`u2-*`/`u3-*`/
    `u4-*` 单元，每条指向必须失败的已提交测试；审计未发现缺失的控制，未新增控制。U5 的 C++ 侧检查不在该
    runner 可补丁范围内，其失败证据与脚本断言的对应关系记录在对应单元 PR 中，由
    `scripts/test_systemc_smoke.sh` 的断言本身（`b_transport` 计数必须严格更低、invalidation 计数必须
    匹配）逐次执行证明
  - CI 覆盖确认（读 `.github/workflows/ci.yml` 核实，非假设）：`Quality and tests` 运行完整 Rust 质量门
    （fmt、check、clippy、test、doc）、三遍 guest 套件（native、`--transport tlm`、`--transport tlm --dmi`）、
    C ABI smoke、A10 producer pin 套件与 mutation runner；`Debug attach` 运行 tool script（含
    `ruscv-direct-mem` 会话）；`SystemC transport smoke` 运行 pinned SystemC 作业的 `--dmi` 与
    `--dmi-invalidate-every` 遍。无新 CI 作业，开发镜像不变
  - `tools/a10/public-v1.json` 及其 fixtures 与里程碑前逐字节相同（`tools/a10/` 自 `17633cc` 起无改动），
    A10 只在 native 路由上运行，Stage 3 performance-certification 缓期不变
  - D14 文档收尾：`docs/integration/systemc-tlm.md` 对直接内存的 grant、permission、latency、invalidation、
    lifetime、version-clock persistence across reset 与 FFI 规则成为规范说明（SystemC 侧），quantum 与
    同步策略仍为该页开放项；README 的 `--dmi` 行与 SystemC/`--help` 文本已随各单元更新，本单元核对全部
    陈述与源码一致；验证文档列出第三遍 guest 套件与 96 单元 mutation runner
  - 修复 `direct-mem-u1-copy-without-the-lock` 的输出交错 flake：被变异的源使测试在释放前 panic，挂起在
    拷贝钩子里的辅助线程随后因通道关闭而 panic，其分离线程的 panic 输出与 runner 匹配的
    `test result:` 汇总行竞争。钩子改为在通道关闭时安静返回（`let _ = ...recv()`），辅助线程不再产生
    竞争输出；测试与控制行为不变

- **TLM U7: 累积验证与收尾（issue #179）**
  - 合同 §4 的十一条负控制全部在 mutation runner（`scripts/debug/stage6-mutations`）注册，并逐条指向必须
    失败的已提交测试：默认 transport 不是 native（`tlm-u1-default-not-native`）、
    `TLM_GENERIC_ERROR_RESPONSE` 映射为 guest 可见故障（`tlm-u1-generic-error-guest-fault`）、
    不完整或被 target 修改的 payload 被当作完成（`tlm-u1-incomplete-or-modified-accepted`）、
    一次访问被拆分或合并（`tlm-u1-split-access`，同一个事务日志相等断言同时拒绝缺失的事务）、
    `tohost` 轮询与镜像安装走 `b_transport`（`tlm-u1-poll-b-transport`、`tlm-u1-install-b-transport`）、
    AMO 被模拟为独立读和写（`tlm-u2-amo-read-then-write`）、正的 sub-tick 延迟被舍入为零
    （`tlm-u3-subtick-rounds-to-zero`）、transport 延迟计入 `mcycle`/`minstret`
    （`tlm-u3-delay-counted-into-counters`）、未知完成后未 reset 仍发起访问
    （`tlm-u1-access-after-unknown`）、回调再入 C ABI 被接受（`tlm-u5-callback-reentry-accepted`）、
    TLM 状态或终止结果前的 host stop 被报告为通过的 ruscv-sim 结果
    （`tlm-u5-transport-status-passes`、`tlm-u5-boundary-passes`）。`sc_stop` 由该 Rust stand-in
    加上 SystemC 作业的早停变体（`--stop-after-grants 1`，必须不通过）共同覆盖。审计未发现缺失的控制，
    未新增控制
  - CI 覆盖合同要求的全部检查：`Quality and tests` 运行完整 Rust 质量门（fmt、check、clippy、test、doc）、
    带 TLM 第二遍的 guest 套件、C ABI smoke（`scripts/test_capi_smoke.sh`）、A10 producer pin 套件与
    mutation runner（`python3 scripts/verify_stage6_mutations.py --source-ref HEAD`）；`Debug attach`
    运行 tool script（`scripts/debug_attach.sh --require-spike`）；`SystemC transport smoke` 运行 pinned
    SystemC 作业
  - `tools/a10/public-v1.json` 及其 fixtures 与里程碑前逐字节相同（`tools/a10/` 自轮换合并 `579d128`
    起无改动），A10 只在 native 路由上运行，Stage 3 performance-certification 缓期不变
  - D12 文档收尾：`docs/integration/systemc-tlm.md` 成为所选 blocking 映射、FFI 所有权与线程、错误映射、
    时间单位、再入与停止行为、构建与版本 pin 的规范说明，DMI 与 quantum／同步策略仍为开放项；README 新增
    `MachineConfig` 变更、C ABI 与 pinned SystemC 3.0.2 宿主的说明；验证文档列出 TLM 第二遍、C ABI smoke
    与 SystemC 作业
  - 2026-11-05 在合同授权下对 D2 做有界修订：声明的 tick 周期以 `MachineConfig.tick_period`（默认 1 ns，
    来自 D7 与 ADR-0004 §5.2）列为第四个新增公开变更，与 `transport`、`BoundaryFacts` 的
    `transport_delay_femtoseconds`、`Flat` + `tlm` 的 `MachineError::UnsupportedTransport` 并列，
    四者均由此处命名；“all three” 改为 “all four”。合同其余正文不变，合同未归档或轮换
  - 为让 D2 的 “no other public signature changes” 按字面成立，U2 加在既有 `AmoTransform` 与
    `AmoOperation` 上的三个方法（`operation()`、`kind()`、`from_kind()`）由 `pub` 收窄为 `pub(crate)`；
    调用点全部在 crate 内，运行时行为不变，`AmoKind` 等新词表类型保持公开

- **TLM U6: pinned SystemC 3.0.2 smoke（issue #178）**
  - `Dockerfile` 新增 Accellera SystemC 3.0.2 构建：从 `accellera-official/systemc` 取
    `70b0fc8e4a74acc677b0fc73cea08f940c2115d5`（合同 pin），配置前用 `git rev-parse HEAD`
    校验检出的提交并用 `SYSTEMC_3_0_2` 版本宏校验内核，以 C++17 构建静态库连同 TLM-2.0 头文件
    安装到 `/opt/systemc`（`SYSTEMC_HOME`）；pin 被移动或标注不符时镜像构建失败。镜像基线校验
    新增一个真实 SystemC/TLM 编译并运行的程序，缺可用的内核时镜像构建失败
  - 新增 `integration/systemc/` CMake 工程：零延迟 blocking RAM target 模块带
    `simple_target_socket`，实现 `b_transport`、`transport_dbg`、transport reset 与原子扩展
    （一次事务内应用 RMW、LR 与 SC，SC 按 target 自己的逐字节 committed-write 版本快照判定），
    以及把 C ABI 回调映射为 TLM 事务的 initiator socket：一个访问一个事务、streaming width 等于
    长度、无 byte enable、只接受七种标准响应状态，target 修改过的 address/command/length 回声给
    ruscv-sim 以保留其协议判定，原子扩展请求与完成与标注延迟原样按数据跨 ABI
  - initiator 的 `SC_THREAD` 通过 C ABI 以有界 grant 驱动 Machine：每次 grant 从当前
    `sc_time_stamp()` 起算，`wait()` 恰好返回的消费量（内核只按该量提交全局时间，ADR-0004
    §4.4），在 `ruscv_machine_result` 报告终止结果后、或在 host 判定失败后 `sc_stop`（早停控制
    故意在结果前停下且不得通过）；kernel 时钟与 Machine 游标不一致、零消费的 grant 边界、
    turn 上限或 grant 守卫超限都是 host 失败。时间分辨率设为 1 fs，使标注
    延迟的飞秒映射精确；`--max-turns`（默认 100000，等价于公共 CLI 的 `--max-cycles`）
    把每个 grant 的预算夹到剩余 turn 额度，永不终止的 guest（例如退出信号被路由破坏后进入
    死循环）以诊断失败而不是让 kernel 空转
  - 新增 `scripts/test_systemc_smoke.sh` 与 CI 作业 `SystemC transport smoke`：在该镜像内构建
    静态库与 SystemC 宿主，`atomic_tohost_exit`（`rv64imafdc`）与 `fd_fma`（`rv64imafd`）
    的退出码与 `--log-commits` 输出必须与 native 公共 CLI 完全一致；空日志、非零退出、无终止
    结果或 RAM target 未服务任何 blocking 事务都失败。两次运行使用同一 turn 上限，且每次运行
    都有 wall-clock `timeout`，因此 guest 死循环或宿主缺陷都不会挂住 CI。现有 CI 作业不变，
    `cargo build/test --all-features` 仍不需要 C++ 工具链或 SystemC
  - 早停变体（`--stop-after-grants 1 --turn-budget 1`）必须在终止结果前停下且不得通过：它以
    非零状态退出、报告 host stop before a terminal ruscv-sim result，且不产生 guest 退出码
  - `docs/integration/systemc-tlm.md` 记录本次实现的映射（构建与版本 pin、FFI 所有权与线程、
    错误映射、时间单位、再入与停止行为）；DMI 生命周期与 quantum/同步仍为开放项。Linux x86-64
    only

- **TLM U5: C ABI facade（issue #177）**
  - 新增 `capi` cargo feature、`src/capi` 与手写头文件 `include/ruscv_sim.h`；crate type 与默认/
    `--all-features` 构建不变，静态库用 `cargo rustc --lib --features capi --crate-type staticlib`
    构建，`cargo build/test --all-features` 不需要 C++ 工具链
  - 不透明句柄：create（ISA profile、tick 周期、可选 C RAM target）、load ELF、set commit log、
    run grant、read boundary facts、read result、reset、destroy。grant 携带权威起点、最大结束时间与
    turn 预算，返回本次消费的建模时间（完成的 turn、idle 跳进与 transport 延迟）、边界事实以及是否
    触及 grant 边界；早于 Machine 已提交时间或早于调用方自己已提交时间的起点按 ADR-0004 §4.4 拒绝
    （内核只按 grant 返回量提交全局时间，因此其时钟同样不能回退），reset 重启纪元
  - 声明的 tick 周期进入组合：`MachineConfig` 新增 `tick_period` 字段（默认 1 ns），Platform 用它
    构造可选择的 RAM 槽位，因此 target 的标注延迟与声明的单事务最大值按同一周期换算，`mtime` 的 tick
    与返回的消费时间一致；C ABI 由 create 声明该周期
  - 轮询或清除 RAM 驻留 `tohost` 时的 debug transport 失败按 host failure 分类（保留已完成的
    transition 与已消费时间），不再被当作静默缺失或被接受为 guest 退出
  - 回调再入按线程判定：回调运行期间任何 handle 的 C ABI 调用（含 create/destroy）都以
    `RUSCV_ERROR_REENTRANT` 拒绝，而不只是同一个 handle；超出 64 字节的 LR 快照不再被截断接受，
    而是留给适配器判为协议失败
  - 终止的 guest 退出不再丢弃同时成立的边界事实（最后一个 turn 预算槽与含端截止时间），
    分数 tick 的剩余视界报告为 `deadline_blocked` 与真实飞秒剩余 slack
  - `ruscv_machine_reset` 对普通（含 failed）运行使用组合自身的 fresh reset（quiesce、清零、
    重新安装），只有未知完成才按 D6 在 target 确认 transport reset 后构建新的 transport 域；
    失败的组合永不恢复、重连或强制 drain
  - grant 级 deadline 阻断（ADR-0004 §7.2）：启动任何 turn 前先形成该 grant step 的保守完成上界。
    一个 step 可能完成等待中的 turn 及其唤醒 turn，因此上界为两个完成 turn tick 加上各自最坏情况
    事务数乘以 target 声明的单事务最大延迟；放不下的 step 不启动，并以 `deadline_blocked`、剩余
    slack 与被拒绝的上界报告
  - 含端截止时间处不启动普通 turn：等待结算（idle jump）落在截止时间上时，同一 exchange 在该
    时刻停止并报告 `DeadlineReached`，不再执行唤醒 turn；因此定时器恰好到期的 grant 不会越过
    其视界（ADR-0004 §7.2）
  - C RAM target 是 `b_transport`、`transport_dbg`、transport reset 回调表加声明的单事务最大延迟与
    user 数据；payload 的原始字节缓冲由 Rust 拥有且只在一次回调内有效，原子扩展的请求与完成（RMW、
    LR 快照、SC 结果）同样跨 ABI 编码为数据
  - 只在创建线程接受调用；回调只在 ruscv-sim 调用内同步执行，从回调再入 C ABI 返回
    `RUSCV_ERROR_REENTRANT`。Rust panic 在边界被捕获并毒化句柄（`RUSCV_ERROR_PANIC`，之后
    `RUSCV_ERROR_POISONED`），错误一律为整数状态码。TLM 状态、回调返回值或 host stop 都不会
    变成 ruscv-sim 的终态结果；未知完成毒化 transport，只有 target 确认 transport reset 后的
    reset 才重新安装镜像并继续
  - `Quality and tests` 作业新增普通 C 编译并链接的 smoke（`scripts/test_capi_smoke.sh`），覆盖
    生命周期、profile 选择、commit log、grant 时间字段与过早起点拒绝、跨线程与再入拒绝、panic
    毒化、不通过的 TLM 状态与未知完成后的 reset；缺少 C 编译器时脚本直接失败，不算作跳过
  - 负控制：回调再入 C ABI 被接受；TLM 状态（终态结果之前）被报告为通过的 ruscv-sim 结果

- **TLM U4: guest-suite parity（issue #176）**
  - `scripts/run_elf_tests.sh` 对每个 guest 以声明的 profile 和相同 cycle 上限再运行一次 `--transport tlm`，并用与 native 相同的成功分类；仅在 TLM 路径失败或超时的 guest 是真实失败，不是 skip
  - parity set（`hello`、`s5_timer`、`s5_external`、`atomic_tohost_exit`、`fd_fma`）在两次 pass 都写 `--log-commits`，由 `cmp` 逐字节比较；日志缺失/为空或内容不同都使 suite 失败，汇总行 `TLM parity logs compared: 5/5` 是验收证据的一部分
  - 新增 `tests/tlm_u4_guest_parity.rs`：以 fake simulator 驱动真实的 suite 脚本，证明每个 guest 都以同一 profile 跑两次、第二遍使用 TLM，且 parity 日志确实被比较；另一个公共 CLI 用例对 synthetic guest 比较 native/TLM commit log
  - 负控制（`scripts/debug/stage6-mutations`）：TLM RAM 路由改变写入数据会被逐字节比较捕获；第二遍被改回 native transport 会被 runner 检查捕获
  - native 结果、A10 oracle 与 ACT4 选择不变

- **TLM U3: transport delay and time（issue #175）**
  - 标注延迟按声明的 Platform tick 周期（默认 1 ns）从飞秒换算：能整除时精确换算，否则正延迟向上取整，零仍为零；正的非整 tick 延迟不会消失。零 tick 周期（畸形）与换算/累计溢出是 adapter 失败，而不是被丢弃、为负或回绕的时钟更新
  - 每个 guest turn 的保守完成上界（ADR-0004 §7.2）按 target 声明的单事务最大延迟乘以最坏情况事务数计算（两个取指 parcel、Sv39 页表与 A/D 访问、一次数据或原子访问，共 18）；单个响应超过声明最大值按 §5.3 overshoot 报告为 simulator failure，诊断中给出 overshoot 与上界
  - 延迟按 ADR-0004 §5.3 的结果消费：成功访问与 guest 可见 target fault 消耗完整的已标注延迟，已知 host 失败消耗已标注的延迟，协议失败与未知完成不把标注当作已流逝时间；已消耗的延迟不会因后续 fault 或返回原因被撤销
  - 完成的 turn 让 `mtime` 前进一个 turn tick 加换算后的延迟 tick，因此定时器比较能看到延迟；失败的 turn 只消耗已知延迟，不增加 turn tick。两者都不改变 `mcycle`/`minstret`
  - SBA 与 Debug Mode（含 program-buffer 执行）的访问延迟同样换算和校验，但不计入 Platform 时间，保持 2026-10-28 与 2026-10-30 ADR-0004 修订；`BoundaryFacts::transport_delay_femtoseconds` 现在报告已计入的精确延迟总量，native 与零延迟 target 保持 0
  - 原子（LR/SC/AMO）响应与普通数据访问遵循同一延迟换算、保守上界与计入规则：一次原子事务的标注延迟在 guest turn 中计入 `mtime`，超限同样按 §5.3 overshoot 失败并保留已知延迟
  - in-tree `ZeroDelayRam` 与 SystemC target 仍标注零延迟，parity 运行时间等价；延迟测试使用 test-only 的延迟 target。新增负控制：正的 sub-tick 延迟被舍入为零、transport delay 计入 `mcycle` 或 `minstret`

- **TLM U2: 原子 payload 扩展（issue #174）**
  - LR、SC 与全部 AMO 经一次 blocking `b_transport` 事务携带 ruscv-sim 原子扩展：操作、
    宽度、`aq`/`rl` 与 SC 的保留上下文（保留区间与 committed-write 快照）均作为数据编码，
    Hart 的 AMO transform 函数指针不跨传输
  - in-tree `ZeroDelayRam` 在同一锁临界区内按与 `hart_amods` 一致的运算表执行 RMW、LR、SC，
    返回旧值、快照或条件结果，并维护与 native RAM 相同的逐字节 committed-write 记账；
    适配器记录完成的写入、AMO 与成功 SC
  - 不接受扩展的 target 以 `TLM_COMMAND_ERROR_RESPONSE` 拒绝，按 native 原因映射为 LR 的
    load access fault 与 SC/AMO 的 store access fault；返回 `TLM_OK_RESPONSE` 却没有原子完成
    的 target 是协议失败
  - 对全部 9 种 AMO 的 W/D 宽度、signed/unsigned min/max、W 符号扩展以及 LR/SC 的
    成功、失败、覆盖与无保留结果做 native 对等测试；计数 target 证明每个原子（含 Sv39
    A/D 的 LR/SC）恰好一个事务
  - 负控制：AMO 被模拟为独立读和写会被捕获（`scripts/debug/stage6-mutations`）

- **TLM U1: Rust TLM RAM 路由（issue #173）**
  - 新增独立的 blocking-transport 适配词表（`src/blocking`，与 `src/tlm` 无关）：IEEE 1666
    七种响应状态、以 `TLM_INCOMPLETE_RESPONSE` 起始的 payload、ruscv-sim 扩展（既有
    `PhysicalAccessKind` 与 Hart/debug-inbound 发起者）、含 `b_transport`、`transport_dbg`、
    transport reset 与声明式单事务最大延迟的 target 接口，以及带检查算术的飞秒延迟
  - Platform 的单一 guest-RAM 槽位可选择原生 `SimpleMemory`（默认，行为不变）或调用方提供的
    TLM RAM target；类型化路径、物理与原子后端、DM inbound、镜像安装和全部 host 视图
    （`tohost` 轮询、签名提取、`read_mem`/`write_mem`、`Machine::memory()`）都经过该槽位。
    安装、host 视图与每次 transport reset 后的 RAM 窗口清零使用 `transport_dbg`，不计为
    事务；未知完成毒化 transport，直到 target 确认 transport reset。轮询或清除 RAM 驻留
    `tohost` 时若 debug transport 失败，运行结果保留 guest 退出码并报告 host 失败
    （`Status: FAILED`）；普通未映射地址保持既有的低优先级观察行为
  - `MachineConfig` 新增公开 `transport` 字段（`MachineConfig::new` 选择 native；这破坏了
    `MachineConfig` 的 struct literal 构造），`Transport::Tlm` 共享 `Arc` target 以保持
    `Clone`。`Flat` + `tlm` 返回新增的 `MachineError::UnsupportedTransport`。
    `BoundaryFacts` 新增 `transport_delay_femtoseconds`（native 与零延迟 target 为 0；
    U3 负责转换、计入与边界事实总量）
  - 新增公开入口 `load_and_run_file_with_config`，在给定 `MachineConfig` 下运行 ELF；CLI 新增
    `run --transport native|tlm`，`tlm` 使用 in-tree 零延迟 Rust RAM target（`ZeroDelayRam`）
  - 原子操作在 U2 之前以原生 access-fault 原因失败关闭：不发事务、不做 read-then-write 模拟；
    非零标注延迟在 U3 之前按适配器失败拒绝，不静默丢弃
  - 负控制（`scripts/debug/stage6-mutations`）：默认 transport 不是 native、
    `TLM_GENERIC_ERROR_RESPONSE` 映射为 guest 可见故障、不完整或被 target 修改的 payload
    被当作完成、一次访问被拆分或合并、`tohost` 轮询或镜像安装走 `b_transport`、未知完成后
    未 reset 仍发起访问

- **GC U1: 默认 profile 切换为 `rv64imafdc`（issue #162）**
  - `IsaProfile::default()`、`MachineConfig::new`、`run --isa` 的默认值以及 native
    （`load_and_run`、`load_and_run_file` 及其 debug 变体）与 flat（`RiscVSimulator::new`）
    facade 均选择 `rv64imafdc`（`misa` `0x800000000014112D`，IALIGN=16，FP 状态使能）；
    需要旧默认的调用方必须显式选择 `rv64ima`
  - 原始 `RiscvCore` 兼容构造器（`new`、`new_with_memory`、`new_with_physical_access`、
    `new_with_physical_ports`）显式保持 `rv64ima`，且构造完成即具备与 profile 一致的
    CSR 状态（`misa`、`epc_ialign32`、FP 使能），不再依赖首次 `reset()`
  - `tests/gc_u1_default_profile.rs` 通过真实 CLI、`MachineConfig::new` 与两个 facade 证明新默认；
    `zca_u2_hart_profile` 的 profile 相关断言改显式 `rv64ima` 绑定（flat facade 与 CLI 各一处断言默认本身的用例已按合同 D3 命名并改为显式选择）
  - 新增两条负控制：`gc-default-stays-ima`（Machine 默认仍为 `rv64ima`）与
    `gc-cli-default-diverges`（CLI 默认与 `MachineConfig::new` 分叉）；既有
    `zca-default-misa-c`、`fd-default-misa-f` 控制保留

- **F/D U7: 累积 CI 验证与负控制（issue #151）**
  - 契约要求的十三条 F/D 负控制全部按现有 mutation-runner 模式注册（`scripts/debug/stage6-mutations`），每条指向必须失败的已提交测试：舍入到格式而非整数、忽略静态 `rm`、RMM 视为 RNE、FMA 两次舍入、`f0` 硬连线为零、未 box 的单精度读取、FS Off 仍接受 FP 指令、FPR 写入不置 Dirty、FEQ 对 quiet NaN 置 NV、FCVT.WU.S 负值不饱和到 0、C.FLD 在 `rv64imac`/`rv64imafd` 下被接受、`frm` 为 5–7 时接受动态 `rm`、默认 profile 宣告 F
  - `rv64imafd` 下低两位不是 `0b11` 的非法编码，`mtval` 改为该 16-bit parcel 零扩展，与 pinned Spike 读取首个 parcel 的行为及 archived Zca D2 一致；`rv64ima`/`rv64imac` 保持原值，`fd_zcd_absent` guest 使用非零高半字验证该规则
- **F/D U4: 调试器访问 FP 寄存器与 CSR（issue #148）**
  - FP profile 下 DM 抽象 Access Register 支持 `f0`–`f31`（regno `0x1020`–`0x103f`，`aarsize` 2 与 3）：32 位写入按 NaN-box 存储，32 位读取返回原始低 32 位（D5/D10）
  - `fflags`/`frm`/`fcsr`（CSR `0x001`–`0x003`）经抽象命令读写，不受 `mstatus.FS` 限制；按 pinned Spike，抽象访问本身不改变 FS（相对地，guest 或程序缓冲区里的 FP-CSR 指令仍按 D6 置 Dirty）
  - 程序缓冲区中的 FP 指令遵循常规 D6 FS 规则：FS On 正常退休，FS Off 以 `cmderr=3` 拒绝
  - DM `misa` 读取反映四个 profile；`rv64ima`/`rv64imac` 下 FPR 与 FP-CSR 抽象访问仍为 `cmderr=3`，既有 abstract、program-buffer 与 SBA 行为不变
- **F/D U3b: Zcd 展开与 Spike 格式 FP commit log（issue #152）**
  - 压缩译码器新增 profile 维度（`ZcaProfile::{Zca,Zcd}`）：`rv64imafdc` 下 C.FLD/C.FSD/C.FLDSP/C.FSDSP 按 C.LD/C.SD 与 C.LDSP/C.SDSP 的 8 字节缩放偏移展开为 FLD/FSD，经 U3a 路径执行，并保留 FS-Off 非法指令陷阱；`rv64imac` 下仍为 FP-illegal，`rv64imafd` 为 IALIGN=32 且无 Zcd。`CompressedDecoder::new()` 仍是 Zca-only 译码器
  - Zca 参考生成器新增 `--profile {zca,zcd}` 维度，`tools/zca/zca-reference-v2.tsv` 固定 Zcd 分类；v1 文件逐字节不变，其穷举测试保持有效，新增 v2 穷举测试与 v1/v2 差异测试
  - FP profile 下 commit log 按 pinned Spike `commit_log_print_insn` 格式与 `(index << 4) | kind` 排序打印 FPR 写入（` f<rd>  <FLEN hex>`）与别名 FP-CSR 写入（` c1_fflags`、` c2_frm`；pinned Spike 的 `fcsr` 是复合 CSR，写它记录两个底层 CSR，自身不入日志，等值写入同样记录）；`rv64ima`/`rv64imac` 的日志文本逐字节不变
  - 新增负控制 “C.FLD accepted under rv64imac or rv64imafd”（`scripts/debug/stage6-mutations`）

- **F/D U3a: FP 译码与公共路径执行（issue #147）**
  - 32-bit 译码器新增 R4 opcode（MADD/MSUB/NMSUB/NMADD）与显式 `fmt`/`rm` 译码，覆盖全部 OP-FP、LOAD-FP、STORE-FP 形式；未分配或超出 profile 的 FP 编码按非法指令陷入，不再产生 `SimulatorFailure`
  - FP profile 下全部 F/D 指令经 `src/isa/fd` 在公共 Hart 路径上执行，算术、舍入、异常标志与 NaN 结果来自 U2 的 `src/fpu/arith`；静态与动态舍入均生效，保留的静态 `rm`（5/6）或 `frm` 为 5–7 的动态舍入按非法指令陷入
  - `mstatus.FS=Off` 时全部 F/D 指令（含 FLW/FLD/FSW/FSD、FMV.X.*、FEQ/FLT/FLE、FCLASS、FCVT 到整数）按非法指令陷入；FS-Dirty 遵循 D6：FPR 写入、FP-CSR 写入形式与非零标志累积置 Dirty，FSW/FSD 与不产生异常的 GPR-only FP 指令不改变 FS
  - FLW NaN-box；FSW 与 FMV.X.W 使用原始低 32 位；算术 NaN 结果为规范 NaN，FSGNJ*、FMV 与加载/存储保留 payload（D4/D5）
  - FLW/FLD/FSW/FSD 继续经既有 raw `DataRead`/`DataWrite` 物理路径，保留对齐、Sv39 与访问故障行为
  - 指令退休事实携带 FPR 与 `fcsr` 效果；`rv64ima`/`rv64imac` 的 commit log 文本不变
  - 默认 `rv64ima`、`rv64imac` 行为逐位不变，包括 `s4_fp_absent`

- **F/D U1: FP profile、FPR/`fcsr` 状态与 `mstatus.FS`（issue #145）**
  - 新增 `rv64imafd`（IALIGN=32、`misa` `0x8000000000141129`）与 `rv64imafdc`（IALIGN=16、`misa` `0x800000000014112D`）profile；`run --isa`、native（`load_and_run_with_isa`）和 flat（`RiscVSimulator::new_with_isa`）facade 接受四个 profile，能力检查显式化（`has_f`/`has_d`/`has_fp`）
  - FP profile 下 32 个 FPR 都是普通 64-bit 寄存器（`f0` 不再硬连线）；32-bit 写入 NaN-box；计算型单精度读取对未 box 的值返回规范 NaN `0x7fc00000`，FSW/FMV.X.W/32-bit 调试读取保留低 32 位（D5）
  - `fflags`/`frm`/`fcsr`（0x001–0x003）别名同一个 `fcsr`；保留位读 0、写入忽略；`mstatus.FS=Off` 时三者均按非法指令陷入
  - `mstatus.FS`（含 `sstatus` 视图）在 FP profile 下可写，`SD` 读为 `FS==Dirty`，复位为 Off；FP CSR 写入成功即置 FS=Dirty（CSRRW/CSRRWI 总是写，CSRRS/CSRRC 的 `rs1=x0` 与 CSRRSI/CSRRCI 的零立即数不写）
  - 复位恢复 FPR、`fcsr` 与 FS；`rv64ima`/`rv64imac` 的 FS/SD 仍硬连 0，0x001–0x003 仍非法
  - 本条 U1 只交付 profile 与 FP 状态；F/D/Zcd 指令执行、FP commit-log 格式与 DM 抽象 FPR/FP-CSR 访问由后续 U3a/U3b/U4 单元交付
  - 默认 `rv64ima` 行为逐位不变

- **Zca U3: Debug Mode、单步与程序缓冲区集成（issue #134）**
  - `dpc` 合法性按 profile 掩码：`rv64imac` 入口与写入仅清 bit 0，`rv64ima` 保持 bits 1:0 掩码（D8/D5）
  - C.EBREAK 按 EBREAK 处理：`ebreakm/s/u` 重定向进入 Debug Mode（`dcsr.cause=1`，`dpc` 为自身地址），否则产生 Breakpoint 异常
  - 单步跨过一条 2 或 4 字节指令，包括 `pc % 4 == 2`
  - `rv64imac` 下两个程序缓冲区字构成连续的 8 字节 parcel 流，随后是隐含 EBREAK：混合长度、C.EBREAK 正常终止；C.J/C.JR/C.JALR/C.BEQZ/C.BNEZ 与越过 `progbuf1` 的 32 位指令按 `cmderr=3` 拒绝
  - DM 抽象寄存器读取的 `misa` 反映所选 profile；`rv64ima` 下缓冲区中的压缩 parcel 仍按非法指令报 `cmderr=3`，`progbufsize`、`datacount`、`impebreak` 不变
  - 默认 `rv64ima` 的 abstract、program-buffer 与 SBA 行为逐位不变

- **Zca U2: Hart ISA profile 与 parcel 取指（issue #133）**
  - `run --isa <rv64ima|rv64imac>` 选择 Machine ISA profile；native（`load_and_run_with_isa`）与 flat（`RiscVSimulator::new_with_isa`）facade 支持同一选择，不从 ELF `e_flags` 推断
  - `rv64imac` profile：16-bit parcel 取指（第二 parcel 仅在首 parcel 低两位为 `0b11` 时取）、跨页 straddle 的逐 parcel 翻译与 D4 fault 优先级/tval 规则、IALIGN=16（链接/落点为 `pc + length`，`xepc`/`dpc` 写仅清 bit 0，MRET/SRET 返回 2 字节对齐目标）、只读 `misa` `0x8000000000141105`
  - commit log 对 16-bit 指令按 pinned Spike 格式输出 4 位十六进制
  - 默认 `rv64ima` 行为逐位不变

- **M4: 调试支持**
  - GDB RSP 服务器实现
    - 完整的 Remote Serial Protocol 支持
    - 断点/观察点/寄存器/内存访问
  - CLI 调试界面
    - 交互式命令行调试工具
    - 支持源码级调试
  - 断点管理器
    - 软件断点 (breakpoint)
    - 硬件断点 (hardware breakpoint)
  - 观察点管理器
    - 读观察点 (read watchpoint)
    - 写观察点 (write watchpoint)
    - 访问观察点 (access watchpoint)
  - 版本控制 git hooks
    - commit 前自动 fmt + check
    - push 前自动 clippy

### Changed

- **Quantum U4：C ABI 需要重新编译**
  - `ruscv_target`、`ruscv_payload` 与 `ruscv_boundary` 只追加字段，既有字段的偏移与大小不变，但结构体
    大小改变（`ruscv_target` 48 → 56、`ruscv_payload` 248 → 256、`ruscv_boundary` 64 → 96 字节）。本 ABI 与
    上一版 header 不二进制兼容：针对旧 header 编译的宿主必须在链接本版本前用新 header 重新编译
  - CI 把 debug mutation controls 从 "Quality and tests" 拆成独立的 "Debug mutation controls" job（同样的
    Rust 与 RISC-V 工具链、同样的触发条件与 90 分钟上限），因为该步骤在 U2 的 head 上占用了质量 job 1 小时
    15 分钟预算的大部分；质量 job 的其余步骤不变

- **Sprint 11: TLM2.0 + 外设**
  - TLM2.0 抽象层完整实现
    - TlmPhase: BEGIN_REQ, END_REQ, BEGIN_RESP, END_RESP 四阶段协议
    - TlmResponseStatus: 完整的响应状态枚举和错误分类
    - TlmCommand: Read/Write 命令类型
    - ScTime: SystemC 风格的时间管理（皮秒精度）
    - TlmGenericPayload: 通用事务载荷，支持字节使能、流式传输、DMI
    - TlmPayloadBuilder: Builder 模式构造载荷
    - TlmInitiator/TlmTarget: 发起者和目标接口 trait
    - TlmBus: 多设备互联总线，支持固定优先级/轮询/LRU 仲裁
    - TlmBusBridge: 总线桥接器，支持地址转换
    - TlmSimpleMemory: 简单内存实现
    - DmiData: 直接内存接口支持
  - 外设模型实现
    - CLINT (Core Local Interruptor): mtime/mtimecmp 定时器，MSIP 软件中断
    - PLIC (Platform-Level Interrupt Controller): 最多 1024 个中断源，优先级仲裁
    - UART 16550: 标准串口控制器，支持 FIFO、中断、波特率配置
  - 平台配置
    - PlatformConfig: 预定义 SiFive HiFive1 和 QEMU Virt 平台配置
  - 测试套件
    - TLM 基础类型测试: 10 个
    - TLM Payload 测试: 10 个
    - TLM 总线测试: 10 个
    - 外设集成测试: 20+ 个
    - 总计 90+ 个测试

### Fixed

- Debug abstract-memory operations now participate in Machine admission. An unresolved physical completion blocks reset, resume, and further abstract commands while keeping DMI error inspection available.
- Hart 0 halt requests survive Hart reconstruction and selection changes. Explicit request clear and Debug Module reset cancel the persistent request.

### Removed

- **Stage 6 U6:** the unused GDB Remote Serial Protocol debugger under `src/debug/` and its crate re-exports (`DebugTarget`, `GdbServer`, breakpoint and watchpoint managers, and the debug CLI). OpenOCD `remote_bitbang` on `run --rbb-port` is unchanged.

### Planned

- **Sprint 8.5: 推广 C 指令模块化模式到所有指令集**
  - 分析当前 C 指令的组织结构（src/isa/rv64c/）
  - 设计重构方案：将 RV64I/RV64M/RV64A/RV64F/RV64D 按功能拆分成多个小文件
  - 参考 C 指令的组织模式，保持 API 兼容性
  - 规划文档: `docs/sprint-8.5-plan.md`
  - 预计改动：新增 23 个文件，修改 3 个文件，删除 24 个文件
  - 时间估算：7-11 小时

### Added

- RV64I base instruction set support (64-bit integer operations)
  - 64-bit register file (x0-x31 as 64-bit registers)
  - 64-bit arithmetic instructions: ADDW, SUBW, SLLW, SRLW, SRAW
  - 64-bit immediate instructions: ADDIW, SLLIW, SRLIW, SRAIW
  - 64-bit load/store: LD, SD, LWU (zero-extending word load)
- RV64M multiplication and division extension
  - 64-bit multiply: MUL, MULH, MULHU, MULHSU
  - 64-bit divide: DIV, DIVU, REM, REMU
  - Proper overflow handling per RISC-V spec (i64::MIN / -1 returns i64::MIN)
- RV64A atomic operation extension
  - Load-reserved/Store-conditional: LR.D, SC.D
  - Atomic memory operations: AMOADD.D, AMOSWAP.D, AMOAND.D, AMOOR.D, AMOXOR.D,
    AMOMAX.D, AMOMIN.D, AMOMAXU.D, AMOMINU.D
- RV64F single-precision floating-point extension
  - 32-bit floating-point register operations
  - IEEE 754-2008 compliant arithmetic
  - NaN boxing/unboxing for upper 32 bits of 64-bit registers
- RV64D double-precision floating-point extension
  - 64-bit floating-point register operations
  - Full IEEE 754-2008 compliant arithmetic
- CSR (Control and Status Register) framework
  - Machine mode CSRs: mstatus, misa, medeleg, mideleg, mie, mtvec, mcounteren,
    mscratch, mepc, mcause, mtval, mip, mhartid
  - Supervisor mode CSRs: sstatus, sie, stvec, scounteren, sscratch, sepc,
    scause, stval, sip, satp
  - Virtualization mode CSRs: vsstatus, vsie, vstvec, vsscratch, vsepc,
    vscause, vstval, vsip, vsatp
  - CSR instruction support: CSRRW, CSRRS, CSRRC, CSRRWI, CSRRSI, CSRRCI
- Privilege mode support
  - User (U), Supervisor (S), and Machine (M) modes
  - Privilege mode transitions and protection
  - Trap handling framework with MRET/SRET instructions

### Changed

- Migrated from RV32I to RV64I as the base architecture
- All integer registers upgraded from 32-bit to 64-bit
- Memory addressing upgraded to support 64-bit virtual address space
- MSTATUS register mask updated for RV64 (0x8000_0003_000D_FFEA)

### Technical Details

#### Division Overflow Handling
Per RISC-V Spec Volume I, Section 2.4:
- Division by zero returns all ones (-1)
- Overflow case (signed MIN / -1) returns MIN (not undefined behavior)

#### MSTATUS Register Layout (RV64)
Per RISC-V Privileged Spec, Section 3.1.6:
- Bit 63 (SD): State Dirty (read-only)
- Bits 35:34 (SXL): Supervisor XLEN
- Bits 33:32 (UXL): User XLEN
- Bit 22 (TSR): Trap SRET
- Bit 21 (TW): Timeout Wait
- Bit 20 (TVM): Trap Virtual Memory
- Bit 18 (MXR): Make Executable Readable
- Bit 17 (SUM): Supervisor User Memory Access
- Bit 13 (FS): Floating-point State
- Bits 12:11 (MPP): Machine Previous Privilege
- Bit 8 (SPP): Supervisor Previous Privilege
- Bit 7 (MPIE): Machine Previous Interrupt Enable
- Bit 5 (SPIE): Supervisor Previous Interrupt Enable
- Bit 3 (MIE): Machine Interrupt Enable
- Bit 1 (SIE): Supervisor Interrupt Enable

## [0.1.0] - 2025-01-XX

### Added

- Initial project setup with Rust 2024 Edition
- CI/CD pipeline with GitHub Actions
- Pre-commit and pre-push git hooks
- Basic project structure and documentation
- RV32I base instruction set foundation
