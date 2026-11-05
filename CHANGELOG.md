# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

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
