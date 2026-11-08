# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- **Device Delay Annotation U2: C ABI 与纯 C smoke（issue #249）**
  - `include/ruscv_sim.h` 的 payload、设备表与声明上限注释命名已接受的注解与它的边界（delay D7）：
    payload 的 device 段落说明 `delay_fs` 是已完成 Hart 响应的建模设备延迟、至多句柄声明的
    `ruscv_target.max_delay_fs` 且通常为零，在 guest turn 内经 turn 账本计费一次，`DebugInbound`
    仍须为零；`delay_fs` 字段注释写明它是响应的精确飞秒延时；`max_delay_fs` 注释说明它同时约束设备
    响应，覆盖该路线的每一次事务，设备事务也包含在内；`ruscv_device.b_transport` 注释允许设置
    `delay_fs`（超限是运行的 overshoot host failure，不是 guest 故障），其余时序、提示与请求字段仍须
    保持接收值
  - `src/capi/target.rs` 的 `CDeviceTarget` 注释不再写“设备 target 不注解延迟”：它仍无自己的声明
    上限，因为共享的响应路径复制 `delay_fs`、由 guest-RAM slot 的账本按句柄的 `max_delay_fs` 判定
  - 没有结构体、函数、窗口代码或错误码变化，也没有 layout pin 移动：`ruscv_device` 48、
    `ruscv_payload` 280、`ruscv_target` 64 与它们的末字段保持原样，所以本单元不改 ABI，已按本头文件
    编译的宿主无须重新编译
  - 单元级证据（`src/capi/tests.rs`，真实入口 + Rust `extern "C"` 设备表）：RAM 表声明一 tick 时，
    C 设备表注解一 tick 被接受，两个窗口各自的两次设备访问使 grant 的 `transport_delay_fs` 为两 tick、
    `consumed_fs` 为 parity 数额加两 tick，退出码、turn、PC 与发布值都与零注解 parity 相同；超限一飞秒
    以 overshoot host failure 失败且不发布值；`DeviceHarness` 新增声明上限的构造，两个拒绝测试保留
    host-elapsed、hint、changed-request 与 event 分支，其延迟分支移入计费测试
  - `tests/capi/smoke.c` 的测试设备新增 `delay_fs` 旋钮，纯 C 跑通两个窗口的接受计费与超限失败；
    `scripts/test_capi_smoke.sh` 的说明同步
  - mutation 控制：新增 `delay-u2-abi`（C 响应路径的 `delay_fs` 复制），`cases.json` 共 184 个控制，
    全部通过 `patch --fuzz=0 --dry-run`
  - 本单元不改窗口、事件、错误码、turn bound、grant 预留、quantum、direct memory 与非阻塞路径，不做
    SystemC（U3, #250）与文档收尾（U4, #251）

- **Device Delay Annotation U1: Rust 设备延迟计费（issue #248）**
  - `DeviceOutcome` 携带响应的延迟而不是在适配器拒绝（delay D4）：`TlmDeviceAdapter::transact` 校验
    通过后，终止状态（`Ok`、guest 可见故障、`GenericError`）把响应的 `response_delay` 放进 outcome，
    `Incomplete`、校验失败与 unknown completion 携带零。`DebugInbound` 仍要求零延迟，所以同一处
    `validate_response` 只把零延迟规则收窄到 inbound 访问，违规仍是该次访问的协议失败
  - `SystemBus::finish_device` 先计费再发布事件（delay D4/D5）：新增的 `RamSlot::charge_device_delay`
    走既有的 `TlmRamAdapter::account_delay`（换算、向上取整、已声明每事务上限的超限、turn bound、
    pending 数额、generation 总计）并只加一个 `DelayAccess::Device` 标签，没有第二个账本。计费失败
    （超限、不可表示、计数溢出）返回 host failure 且不发布该次访问的 event，因此 UART 字节与 HTIF
    写值都不会到达 sink；计费在超限判定之前累加，失败的 turn 仍消耗已知延迟。只有 turn 窗口打开时
    的 Hart 访问计费，Debug Mode Hart 端口访问被校验但不计费，`host_elapsed` 保持零
  - 树内 UART 与 HTIF target 保持零延迟参照（delay D2），其模块注释写明它们不注解延迟；`device`、
    `htif`、`nb` 等旧引用继续指向已归档合同，新代码引用 `delay D<n>`。模块与 trait 注释不再声明
    “设备 target 不得注解延迟”，改为 Hart 访问可注解、`DebugInbound` 必须为零
  - 单元级证据：`src/blocking/device_tests.rs` 钉住 Hart 响应可携带延迟与 `DebugInbound` 拒绝；
    `src/executor_device_tests.rs` 钉住延迟经 RAM slot 的 turn 账本计费一次、超限不发布且保留已知
    数额，以及宿主 UART／HTIF 延迟下退出码、turn、PC 与逐字节相同的 commit log；`src/machine/tests.rs`
    把两个拒绝测试改写为计费测试（1 tick 精确进 `platform_mtime` 与
    `BoundaryFacts.transport_delay_femtoseconds`，event、计分器与 parity 不变）、树内运行零延迟、
    终止故障/`GenericError` 计费一次、`Incomplete` 与 unknown 不计费、超限失败、SBA 拒绝、
    Debug Mode Hart 端口校验不计费，以及 quantum 1024 下经 F1 flush 后在 turn-end sync 提交；
    `src/capi/tests.rs` 用 `Session` 证明 charge 计入 grant 的 `consumed_fs` 与
    `transport_delay_fs`，且允许 bound 的 horizon 在设备延迟下走到 horizon 而不 `deadline_blocked`
  - mutation 控制：退役 `device-u2-zero-delay`（其规则已被契约取代），新增 `delay-u1-charge`
    （`finish_device` 的计费）、`delay-u1-in-tree`（树内 target 报告延迟）与 `delay-u1-debug-zero`
    （`validate_response` 的 `DebugInbound` 零延迟规则），`cases.json` 共 183 个控制，全部通过
    `patch --fuzz=0 --dry-run`
  - 本单元不改 C ABI（U2, #249）、SystemC（U3, #250）与文档收尾（U4, #251），不改窗口、事件、
    错误码、turn bound、grant 预留、quantum、direct memory 与非阻塞路径，也不改 `mcycle`/`minstret`

- **Host HTIF Service U4: 文档与验收收尾（issue #242）**
  - 文档补全并校正到 U1–U3 的实际行为：`README.md`、`CHANGELOG.md`、
    `docs/integration/systemc-tlm.md`、`docs/verification/README.md` 与 `integration/systemc/README.md`
    统一写出 HTIF 窗口码 `RUSCV_DEVICE_HTIF` 1（`0x4000_8000`/8）、`--htif-target in-tree|host`、三个
    `--fault-htif-*` 选项、运行停止规则（接受写在设备访问内经既有 `config.htif_write` sink 发布一次、
    store 随后退役、同一 turn 上既有退出解码判定 `PlatformExit`；漏报 event 则无退出事实并走到 turn
    bound）与限制
  - 删除 U1–U3 之后过期的说法：README 的“Host HTIF 服务……尚未实现”、`docs/integration/systemc-tlm.md`
    的“no runtime claim here”与“until that contract's units land”、verification 文档的“SystemC host……
    remain requirements”均改为已实现并引用源与测试；该页的开放项去掉 host HTIF 服务、
    保留合同 §5 的其余方向
  - 已知限制如实记录：C ABI 没有 HTIF 写的逐值 sink 入口，所以 SystemC 只通过运行停止、turn bound 与
    被拒写在 target 接受序列中的缺席来观察发布，sink 级发布规则由 Rust 测试
    （`src/blocking/device.rs`、`src/machine/tests.rs`）证明；`HtifTarget` 的读应答固定为零、只渲染
    标准 console 设备的 putchar 命令，且不暴露 fromhost 或 host-to-guest 输入
  - 验收确认：五个 §4 控制（`htif-u1-route`、`htif-u1-reset`、`htif-u1-read-event`、`htif-u1-value`、
    `htif-u2-window`）全部注册，`cases.json` 共 181 个控制且全部通过 `patch --fuzz=0 --dry-run`；
    smoke 以各自断言运行三个 HTIF 故障选项；CI 的 `Quality and tests`、`SystemC transport smoke` 与
    `Debug mutation controls` 覆盖 D9 列出的检查，未新增作业
  - 本单元不改运行时行为、不改 `docs/dev-plan.md`、不归档合同，A10 oracle 与认证豁免不变

- **Host HTIF Service U3: SystemC 宿主 HTIF target 与 smoke（issue #241）**
  - `integration/systemc/htif_target.{h,cpp}` 以普通回调对象实现 `ruscv_device` 的 HTIF 窗口
    （htif D8）：一次已验证的 dword 访问就是一次 `b_transport` 调用，读应答零（htif D3），接受的写
    回 `RUSCV_TLM_OK` 加上恰好一个 `RUSCV_DEVICE_EVENT_HTIF_WRITE`，其值就是 guest 的八个
    小端字节；目标按顺序把每个接受值打印为一行 `htif-value: 0x...`，把标准 console 设备
    （device 1、command 1、payload 字节）的字节渲染成一行 `htif-console: ...`，退出时报告
    selection、事务／读／写／值／`receive` 计数与渲染出的 console 序列。`receive` 从不被调用
    （计数为零），`transport_reset` 丢弃窗口寄存器状态
  - `--htif-target in-tree|host`（默认 `in-tree`）选择端点目标，其他取值是用法错误；`host` 在
    `ruscv_machine_create` 之后、首次 `ruscv_machine_load_elf` 之前用同一窗口索引表 attach
    `RUSCV_DEVICE_HTIF`，可与宿主 UART target 同时存在。三个故障选项各要求 `--htif-target host`，
    in-tree 或缺失时是用法错误：`--fault-htif-drop-event` 只丢掉退出字写的 event（三个 console
    event 仍发布，运行走到 turn bound）、`--fault-htif-wait` 在回调里等一个 Platform tick 并上报
    host elapsed（以 host-elapsed 诊断失败）、`--fault-htif-event-on-fault` 用
    `RUSCV_TLM_ADDRESS_ERROR` 上报退出写的 event（该次访问的协议失败，退出值不发布）
  - `tests/bare-metal-riscv-test/rv64i/htif_u3_console.S` 读取端点、非零则以退出码 1 退出，随后写
    三个 console 字节、标准 syscall 退出字（code 0）并 `j .` 自旋；它加入 guest suite 的 native
    与 TLM 两趟以及 `PARITY_GUESTS`，因此三份 commit log 必须逐字节一致
  - `scripts/test_systemc_smoke.sh` 新增 HTIF 段：in-tree 与 host 在 baseline、`--dmi`、`--nb`、
    `--quantum 1024` 下各跑一趟，host 趟要求 native 退出码、逐字节相同的 commit log、非零 HTIF
    事务、guest 的三个 console 值与退出值序列、`Hi\n` console 序列、与 in-tree parity 趟相同的
    grant／consumed 事实，第二次 host 趟事实相同；in-tree 趟的宿主 HTIF 事务为零；既有两个 guest
    在 `--htif-target host` 下保持原事实且 HTIF 事务为零；三个故障选项各带自己的断言
  - `integration/systemc/README.md` 与 CHANGELOG 记录该选项、目标与故障选项；本单元不改 C ABI
    与 U1 Rust 语义，也不做 U4 的文档收尾（#242）

- **Host HTIF Service U2: C ABI HTIF 窗口（issue #240）**
  - 头文件按 append-only 规则追加 `RUSCV_DEVICE_HTIF` 1（htif D7）：`RUSCV_DEVICE_UART` 0 与所有既有值
    不动，也不新增事件 tag（`RUSCV_DEVICE_EVENT_HTIF_WRITE` 2 已存在）。`ruscv_device` 与
    `ruscv_machine_attach_device` 的注释写出 HTIF 窗口的基址 `0x4000_8000`、大小 8 字节、该窗口的事件
    （读不发布事件，接受的写上报一个 `HTIF_WRITE`，其值取自响应的八个写字节）以及“一个窗口一张表、
    每个窗口只 attach 一次、两个窗口可同时选择”的规则。表、payload 与任何布局 pin 都不变，因此 ABI 与
    设备头文件保持二进制兼容，宿主须按新头文件重编译后才能使用新窗口码
  - `c_device_window` 增加 HTIF 分支，`attach_device` 按窗口保存复制后的表（htif D7）：先 attach UART
    再 attach HTIF（或反序）得到 `DeviceTargets::with_uart_and_htif`，未选择的窗口继续由树内 target
    应答。拒绝码沿用 D6：NULL handle、NULL 表、未知窗口与不完整表是
    `RUSCV_ERROR_INVALID_ARGUMENT`；base／size 不匹配、同一窗口的第二次 attach、晚 attach 与没有
    `ruscv_target`（无 TLM 路由）的句柄是 `RUSCV_ERROR_CONFIG`
  - 端点请求是一次完整八字节 dword。读由 target 自己的寄存器状态回答且不发布事件；接受的写上报恰好
    一个 `HTIF_WRITE`，C 宿主保持请求字节不变即上报 guest 自己的值，运行停止与退出解码仍是既有事件
    路径（htif D4/D5）。`capi` 测试驱动真实入口证明：attach 在首次 load 之前返回 `RUSCV_OK` 且
    target 自己的计数非零；每个拒绝码；payload 的 `device_event`、`delay_fs`、`host_elapsed_fs` 与
    `may_suspend` 全零且宽度为 8；guest 的八个字节经运行退出码与 target 自己的记录可见；HTIF 窗口上的
    `UART_TRANSMIT` tag 与非零注记都是该次访问的协议失败；回调再入被拒而
    `ruscv_machine_invalidate_direct_mem` 仍被接受；同一句柄上的两个窗口各自服务各自的 guest 访问
  - `tests/capi/smoke.c` 以纯 C 覆盖接受的 HTIF attach、一次读与一次写及其事件与值、每个拒绝码，以及
    该窗口上的 UART tag 失败；`scripts/test_capi_smoke.sh` 的说明同步更新
  - `htif-u2-window` mutation 控制钉住 `c_device_window` 的 `RUSCV_DEVICE_HTIF` 分支：去掉该分支后
    attach 不再是 `RUSCV_OK`、target 也不再服务端点。按窗口存储改了 `src/capi/mod.rs` 的 attach 结尾，
    `device-u3-abi-attach` 以同一语义重生成（复制的表不再存到句柄上），181 个控制全部通过
    `patch --fuzz=0` 干跑
  - 本单元不改 U1 的 Rust 语义、不改 SystemC 宿主与 smoke（U3, #241）、不改文档收尾（U4, #242）；
    `--all-features` 仍不需要 C++ 工具链

- **Host HTIF Service U1: Rust 宿主 HTIF target（issue #239）**
  - `DeviceTargets` 增加私有 HTIF 槽（htif D2）：`with_htif(target)` 只选宿主 HTIF，
    `with_uart_and_htif(uart, htif)` 同时选两个窗口，`htif()` 返回所选 target。`DeviceTargets::none()`
    仍让两个窗口都由树内 target 应答，因此没有宿主选择时运行与之前完全一致；两个窗口各自独立，
    宿主 UART 加树内 HTIF、宿主 HTIF 加树内 UART 与两个宿主 target 都可表示
  - `Platform::build` 按固定顺序在每个 install 重置所选 target：先 UART、后 HTIF，两者都在
    `begin_window` 与镜像放置之前（htif D6）。第二个重置被拒时构建走既有 placement 通道失败，
    已经重置过的第一个 target 不回滚，之前的关联保持不变；`fresh_reset`、`reload_for_ndmreset` 与
    `ruscv_machine_reset` 都走这条路径，target 在重置中 panic 同样按拒绝处理
  - `validate_response` 增加 D10 的两条规则（device D4/htif D10）：`Read` 上报设备事件在两个窗口都是
    该次访问的协议失败；HTIF 窗口上报的 `HtifWrite` 值必须等于请求的八个数据字节，否则同样失败。
    两条规则都在 outcome-unknown 之后、状态映射之前，失败时不发布任何事件。值比较用的是宿主改写
    缓冲区之前捕获的请求字节，因此 target 通过 `data_mut` 改写数据不能改变该值。树内 target 从不在读
    上报事件、其事件值就是请求字节，行为不变；UART 的发送字节仍是寄存器写接受的那个字节，不与请求
    数据比较
  - 类型化视图不变（htif D3）：HTIF 窗口的 typed dword 读在两条路由上都答 0，typed dword 写经同一
    sink 发布一次且不经过 target；HTIF target 的 `receive` 从不被调用（端点没有宿主到 guest 方向），
    `uart_inspection` 仍只描述 UART 窗口，端点也不会被提供 direct memory
  - 运行停止沿用既有事件解码（htif D5）：宿主 target 接受写入并上报事件后，运行在同一 turn 停止，
    退出码、退休指令数与 commit log 与树内运行逐字节一致；宿主接受写入但不上报事件时 Platform 没有
    退出事实，运行走到 turn bound 并以 turn-limit 诊断失败；SBA 访问端点仍是携带 `DebugInbound`
    且不计时的一次设备事务，`DebugBoundary` 的读不发布事件
  - 四条 `htif-u1-*` mutation 控制：`route` 钉住宿主 HTIF 选择，`reset` 钉住每次安装的 transport
    reset 与顺序，`read-event` 钉住读事件规则，`value` 钉住 HTIF 事件值的比较。`device-u2-native-refused`
    随拒绝条件覆盖两个窗口而重生成
  - 本单元不改 C ABI（U2, #240）与 SystemC（U3, #241），不改窗口策略、原子包络、RAM `tohost` 采样、
    A10 oracle 与认证推迟；新增代码引用 `htif D<n>`，既有 `device D<n>` 注释保持指向已归档的合同

- **TLM-Modeled Devices U5: SystemC 宿主设备 target 文档与验收闭环（issue #231）**
  - SystemC 宿主补记 U4 交付的 UART 设备 target（device D9）：`integration/systemc/uart_target.{h,cpp}`
    以普通回调对象实现 `ruscv_device`，模拟 RBR/THR、IER、IIR/FCR、LCR、MCR、LSR、MSR、SCR、DLL/DLM
    与 16 字节收发 FIFO，对 DLAB 清零的已接受 THR 写回 `RUSCV_TLM_OK` 加
    `RUSCV_DEVICE_EVENT_UART_TRANSMIT`，并以 `uart-console:` 逐字节回显到宿主控制台；退出时报告
    selection、事务数、读写数与发送字节。`--uart-target in-tree|host` 选择该 target（默认 `in-tree`），
    其他取值是用法错误，attach 发生在 `ruscv_machine_create` 之后、首次 `ruscv_machine_load_elf` 之前
  - 三个设备故障选项 `--fault-uart-no-lsr`、`--fault-uart-wait` 与 `--fault-uart-event-on-fault`
    各要求 `--uart-target host`（in-tree 或缺失时是用法错误），在 `SystemC transport smoke` 中各自以
    具名断言失败
  - 文档收尾：README 的 C ABI 段写出 `ruscv_device` 表、`ruscv_machine_attach_device` 与
    `ruscv_payload.device_event`（272 → 280 字节，宿主须重编译），SystemC 段写出 `--uart-target`、宿主
    target 模型与三个故障选项；`integration/systemc/README.md` 增加 `uart_target` 模块、用法、退出摘要与
    限制；`docs/integration/systemc-tlm.md` 把设备 target 从“待实现”改为已实现并新增规范段落（窗口策略、
    载体与注记、事件规则、host view、direct memory、reset、attach 入口、宿主模型），并更新故障选项、smoke
    描述、开放项与来源/测试清单；`docs/verification/README.md` 的合同条目改指 U1–U4 的实现测试与 19 条
    `device-*` 控制
  - Rust 源码兼容性提示（device D2）：`MachineConfig` 新增公开字段 `devices`，因此外部代码用 struct
    literal 构造 `MachineConfig` 时会编译失败；这些字面量必须补上 `devices`（通常是
    `DeviceTargets::none()`），或改用 `MachineConfig::new`。C 宿主侧则须按新头文件重新编译，因为
    `ruscv_payload` 已从 272 字节增至 280 字节
  - 本单元不改仿真行为、不改 `docs/dev-plan.md`（轮换由 orchestrator 执行）、不改 A10 oracle 与认证推迟；
    §4 的 19 条 `device-*` mutation 控制均已注册，三个 SystemC 设备故障选项在 smoke 中各自断言，CI 的
    `Quality and tests`、`Debug mutation controls`、`SystemC transport smoke` 与 `Debug attach` 继续覆盖
    完整质量门、guest suite、C ABI、SystemC、工具脚本与 mutation runner

- **TLM-Modeled Devices U3: C ABI 设备表与 attach（issue #229）**
  - 头文件新增 `ruscv_device` 表（device D8）：`window_base`、`window_size`、`user`、`b_transport`、
    `receive`、`transport_reset`。三个回调都必须提供；表在 attach 时复制，`user` 必须活到
    `ruscv_machine_destroy`。`transport_reset` 返回非零表示已静默并接受新访问，返回零按既有 placement
    通道拒绝构建
  - 新入口 `ruscv_machine_attach_device(machine, window, device)`，`window` 取 `RUSCV_DEVICE_UART`（0）。
    必须在 create 之后、首次 `ruscv_machine_load_elf` 之前调用；NULL handle、NULL 表、未知窗口与缺回调的
    表返回 `RUSCV_ERROR_INVALID_ARGUMENT`，`window_base`/`window_size` 不匹配、同一窗口二次 attach、
    晚 attach 与 `ruscv_target` 为 NULL（无 TLM 路由）的 handle 返回 `RUSCV_ERROR_CONFIG`
  - `ruscv_payload` 末尾新增 `int32_t device_event`：结构体从 272 字节增至 280 字节，既有字段的 offset
    不变。设备事件的 tag 为 `RUSCV_DEVICE_EVENT_NONE`（0）、`RUSCV_DEVICE_EVENT_UART_TRANSMIT`（1）与
    `RUSCV_DEVICE_EVENT_HTIF_WRITE`（2），事件的值取自该次访问自身的写数据，因此不需要第二个字段。
    设备事务里的 `delay_fs`、`local_offset_fs`、`host_elapsed_fs`、`may_suspend` 与 `dmi_allowed`
    必须为零；非零 delay、host elapsed 或 hint，改变 address/command/length，以及与窗口不匹配（或无法
    识别）的 tag，都是该次访问的协议失败。RAM 表报设备事件同样被 RAM adapter 拒绝
  - 因为是 append-only ABI，宿主必须按新头文件重新编译：`ruscv_payload` 变大，旧二进制不再兼容
  - 三处 layout pin 同步更新：`src/capi/tests.rs` 的
    `the_direct_memory_fields_are_final_in_their_structs`（含 `ruscv_device` 的 48 字节布局）、
    `tests/capi/smoke.c` 的 size/final-field 检查，以及头文件的 Layout rule 与 final-field 文本
  - `src/capi/tests.rs` 用 Rust `extern "C"` 设备表走真实入口：attach 成功后目标自身计数非零、
    全部拒绝码、设备请求不带任何注记、`UART_TRANSMIT` 被接受而 `HTIF_WRITE` 在 UART 窗口上使运行
    以协议失败结束、host delay/host elapsed/hint 各使运行失败、回调内的重入被拒而
    `ruscv_machine_invalidate_direct_mem` 仍被接受；`tests/capi/smoke.c` 以纯 C 重复拒绝码、一次
    transmit 事务与两处 layout pin。`--all-features` 仍不需要 C++ 工具链
  - 两个 `device-u3-*` mutation 控制：`abi-attach` 钉住 handle 里的表存储，`abi-event` 钉住 ABI
    响应的事件复制

- **TLM-Modeled Devices U2: Rust 主机设备 target（issue #228）**
  - `DeviceTargets` 新增主机选择 `with_uart(target)`（device D2），`MachineConfig` 新增公开字段
    `devices`，`MachineConfig::new` 选择 `DeviceTargets::none()`。`BlockingDeviceEvent`、
    `BlockingDeviceTarget` 与 `DeviceTargets` 同时从 crate 根重导出
  - 选择要求 `Transport::Tlm` 与 `PlatformKind::Native`：native 路由与 flat Platform 都返回既有的
    `MachineError::UnsupportedTransport`，其信息文本新增设备这一情形（device D2）
  - `Platform::build` 在放置镜像之前调用所选 target 的 `transport_reset` 一次（device D6），且排在 RAM
    slot 的重置之前，使一次被拒绝的重置不扰动当前关联。每次重装（`fresh_reset`、`reload_for_ndmreset`、
    `ruscv_machine_reset`）都走同一条路径；返回零表示拒绝，构建经既有 placement 通道失败
  - 主机 UART 窗口没有 Rust 寄存器状态（device D5）：`uart_inspection` 返回 `None`，`receive_uart` 调用
    target 的 `receive`，`MemoryInterface` 的 typed 读与 typed 写都返回 `MemoryError::InvalidAddress`；
    树内窗口保持今天的答案
  - `DebugInbound`（SBA）访问设备窗口仍是同一笔设备事务，携带 debug provenance，不记 delay 与 host
    elapsed（device D5）
  - 十个 `device-u2-*` mutation 控制：UART 路由选择、故障时的事件发布、unknown 优先级、零 delay、零 host
    elapsed、`dmi_allowed`、SBA initiator、native 与 flat 拒绝、每次安装的 transport reset，以及主机窗口的
    `uart_inspection`；三个 `MachineConfig::new` 默认值控制随新字段重生成

- **TLM-Modeled Devices U1: 设备路径与树内 UART/HTIF target（issue #227）**
  - `blocking` 新增设备 target 词汇（device D2）：`BlockingDeviceTarget`（`b_transport`、
    `receive`、`transport_reset`，全部 `&self` 且同步、不等待）、`BlockingDeviceEvent`
    （`UartTransmit(u8)`、`HtifWrite(u64)`，各自属于一个窗口）与 `DeviceTargets`
    （`none()` 表示两个窗口都由树内 target 应答；本单元不新增 `MachineConfig` 字段，主机 UART
    选择属于 U2）
  - `BlockingTransaction` 增加唯一的事件槽：`device_events()` 与 `report_device_event()`。一次被接受的
    访问至多报一个属于本窗口的事件；改变 payload、注记 delay 或 host elapsed、设置 `dmi_allowed`、
    报第二个事件、在非 `Ok` 状态报事件或报别的窗口的事件，都是该次访问的协议失败。RAM target 报设备
    事件同样失败（device D2/D4）
  - TLM 路由此新走设备 target：UART 窗口仍先由 Platform 应用 `0x100` 窗口、byte-only 与拒取指策略，
    HTIF 端点仍要求精确起点、整八字节且拒取指；随后各是一笔阻塞事务（device D1/D3）。原子路径不变：
    UART 仍按宽度拒绝，HTIF 的 RMW envelope 仍在 Platform 内且从不成为设备事务
  - 树内 target（device D3）：`InTreeUart` 复用既有 `Uart16550` 与新增的 crate 内
    `write_reg_reporting`（写出 THR 且 DLAB 清零时返回该字节，FIFO 满也照报）；`InTreeHtif` 的
    dword 读回零、dword 写报一个 `HtifWrite`
  - 每个窗口一个发布 sink，由 `Platform::build` 安装一次（device D4）：UART sink 推
    `PlatformEvent::UartTransmit` 再调 `config.uart_output`，HTIF sink 推 `PlatformEvent::HtifWrite`
    再调 `config.htif_write`。`Platform::build` 不再安装 UART 自身的输出回调；原始写、typed byte 写、
    树内 target 的响应都经同一 sink，且仍在 store 退休前发布
  - 行为不变：无主机 target 时两路由的设备答案、事件与回调序列相同，`mtime` 与 `PlatformEvent`
    序列逐 turn 相同（`Q` = 0），选中 quantum 时设备访问不加时间且仍按 F1 标记该 turn；
    `hello` 在 native、TLM、`--dmi` 与两个 quantum pass 上仍打印 `Hello!`。本单元不改 CLI、C ABI、
    SystemC 宿主或开发镜像

- **Non-blocking U5: 文档收尾与验收闭环**
  - 根 README 的 C ABI 段补上挂起支持（nb D5/D6）：`ruscv_target.suspends_host_time`、
    `ruscv_payload` 的 `may_suspend`/`host_elapsed_fs`、`ruscv_grant.host_elapsed_fs`、挂起表与
    quantum 的 create 拒绝、挂起回调的线程与再入规则，以及“字段追加改变尺寸但不移动既有 offset”的
    重编译要求；SystemC 宿主段补上 `--nb`、`--nb-latency`、`--nb-path`、`--grant-trace`、AT 路径的
    超时/退休/late phase/drain、五个 fault 选项与各自的用法错误，并替换“尚未实现”的旧句
  - `docs/verification/README.md` 更新合同状态与实现证据（`src/blocking/tests.rs`、
    `src/machine/tests.rs`、`src/capi/tests.rs`、`tests/capi/smoke.c`、`scripts/test_systemc_smoke.sh`、
    `scripts/debug/stage6-mutations` 的 15 个 `nb-u1-*`/`nb-u2-*` 控制），并在 SystemC smoke 段
    点名 `--grant-trace` 比较
  - `docs/integration/systemc-tlm.md` 的规范来源改指 nb D7–D10（挂起、base protocol、commit 规则、
    退休与 AT target 模式），并写清 ABI 侧的挂起字段、grant 的 `host_elapsed_fs` 与 `--nb-latency` /
    `--nb-path` 的用法错误；映射表的 grant 行改为 `wait(consumed_fs - host_elapsed_fs)`；开放项仍只余
    TLM 建模设备、SystemC 建模中断线与多笔未完成事务
  - 补上 U1 在 atomic 路径上的计费断言：`an_atomic_response_charges_its_elapsed_time` 让 `e < d` 的
    终态 `Ok` AMO 与 guest 可见 fault 各计一次取整 delay，`e > d` 的终态 AMO 作为 elapsed-above-delay
    失败计 `e`；此前只有 plain 路径与 atomic 的 Incomplete/unknown 有该断言
  - 本单元不改仿真行为、不改 A10 oracle 与认证推迟：§4 的 15 条 mutation 控制均已注册（新增测试不属于
    已有控制的目标），五个 SystemC fault 选项在 smoke 中各自断言，CI 的 `Quality and tests`、
    `Debug mutation controls`、`SystemC transport smoke` 与 `Debug attach` 作业继续覆盖完整质量门、
    guest suite、C ABI、SystemC、工具脚本与 mutation runner

- **Non-blocking U4: 晚响应、退休与 fault 控制（issue #219）**
  - `RuscvInitiator` 用声明上限界定每笔 AT 事务（nb D9）：回调进程的每次等待都以
    `BEGIN_REQ` + `max_delay_fs + 1` fs 为界，因此在声明上限处恰好完成（包括 λ = 0 的
    delta 通知）仍在时限内。生效时间超过超时的阶段是晚阶段，按三种形式各计一次并忽略：
    超时之后的 backward 调用、超时前记录但生效时间在超时之后的阶段，以及注记越
    过超时的 forward `TLM_UPDATED`/`TLM_COMPLETED`；backward `END_REQ` 仍回
    `TLM_ACCEPTED`，backward `BEGIN_RESP` 仍回 `TLM_COMPLETED`
  - 超时仍未完成的事务以 `mark_unknown_completion` 退休：`outcome_unknown` 置位、等待的
    host 时间作为 `e`，由 D4 step 1 计费，overshoot 规则不再重新分类。退休事务的存储保留
    到 target 完成它或释放全部引用：forward 路径已完成的立即释放，`early` 路径作为
    drain 状态保留，由 drain 在阶段生效时间之后发送 forward `END_RESP`。晚到达的响应字节
    永不进入 ABI。target 在事务开启期间继续拒绝 reset 确认，因此晚阶段既不能改 RAM 也
    不能与新一代共存
  - 每次退休后的停止（终端结果或 host failure）都先调用 `ruscv_machine_reset` 并要求返回
    非 OK 状态（target 拒绝确认），再运行 kernel 直到 `RamTarget` 没有已调度阶段且
    initiator 没有 drain 状态，之后才 destroy 句柄并 `sc_stop`；forward 路径已释放事务时
    跳过 reset 探测。退出时报告 late phase 数、退休数、drain 发送的 `END_RESP` 数及其发送的 host
    时间与 target 是否仍有开启的事务
  - `RamTarget` 新增 `TargetFaults`、已调度阶段计数（供 drain 等待）、开放事务查询，并把
    最终阶段延迟集中到 `final_phase_latency()`；三个新 fault 选项在命名位置破坏模型：
    `--fault-late-response` 把本次运行第一笔 AT 访问的最终阶段推迟到声明上限后一个 tick，
    `--fault-wait-blocking` 与 `--fault-wait-debug` 分别在 `b_transport` 与 `transport_dbg`
    等待一个 tick。`RuscvInitiator` 新增 `InitiatorFaults`（含既有 `--fault-keep-offset`）与
    `report_host_time`：`--fault-uncounted-elapsed` 报告 `e` = 0 而保留真实 `d`，
    `--fault-short-delay` 报告比 `e` 小 1 fs 的 `d`
  - 五个 fault 选项按契约 §4 在 `scripts/test_systemc_smoke.sh` 中以各自模式运行并各自断言：
    `--fault-late-response` 在 `accepted`/`early`/`completed`（`uncertain`=1、诊断命名未能
    确定结果、恰好一个 late phase、退休时间恰为声明上限 + 1 fs、已应用计数等于
    `BEGIN_REQ` 计数、`early` 的 drain 在阶段生效时间发出保留的 `END_RESP`、`completed`
    跳过 reset 探测）；`--fault-uncounted-elapsed` 以 kernel 时钟与 Machine 游标不一致的
    host failure 停止；`--fault-short-delay`、`--fault-wait-blocking` 与 `--fault-wait-debug`
    各自命名诊断（前两者 `uncertain`=0，最后一项使 `ruscv_machine_load_elf` 失败）。
    正常 `--nb` pass 必须报告 late phase 数为零；每个选项在错误模式下是用法错误，且一次
    运行只取一个
  - U4 不拥有 mutation-runner 控制，fault 选项即其控制

- **Non-blocking U3: SystemC 基础协议与 AT target 模式（issue #218）**
  - `integration/systemc` 新增 `--nb`、`--nb-latency`（默认 0，最大 10^15 fs）、
    `--nb-path accepted|updated|early|annotated|completed`（默认 `accepted`）与
    `--grant-trace <path>`；`--nb-latency` 或 `--nb-path` 缺少 `--nb` 是用法错误，
    `--grant-trace` 每个 grant 写一行 `consumed_fs`、`committed_end_fs` 与
    `host_elapsed_fs`（nb D7/D8/D10）。`--nb` 把 target 声明为挂起并把 λ 写入声明上限；
    与非零 `--quantum` 的组合仍由 `ruscv_machine_create` 拒绝
  - `RuscvInitiator` 对 `may_suspend` 载荷运行单笔未完成事务的 TLM-2.0 基础协议：
    驱动 `SC_THREAD` 发送 `BEGIN_REQ`，把 backward 阶段按 `sc_time_stamp() +` 注记记录为
    生效时间并用 `sc_event` 唤醒，只在生效时间到达后处理并重新武装；`END_REQ` 回
    `TLM_ACCEPTED`，`BEGIN_RESP` 回 `TLM_COMPLETED`，forward `TLM_UPDATED`/`BEGIN_RESP`
    生效后发送零延迟 `END_RESP`，其他阶段或返回值为未知完成。事务载荷由 initiator
    自持（memory manager、独立数据与扩展存储），请求字节在 `BEGIN_REQ` 前拷入、响应
    字节在完成时拷出；完成时报告 `d = e`。每个 `b_transport` 都测量并报告 `e`，未知
    完成保留该值；`transport_dbg`、`transport_reset` 与 `get_direct_mem_ptr` 也测量
    host 时间，任一回调让时间流逝即分别成为失败传输、未确认 reset 或 `RUSCV_DMI_FAILED`
  - `MachineDriver::run` 在 create 前比较自身线程 id 与 `sc_main` 记录的 id，pthread-process
    构建被当作 host failure 拒绝；grant 循环改为等待 `consumed_fs - host_elapsed_fs`，
    余数为零才跳过等待，并在 `host_elapsed_fs > consumed_fs` 时不再提交时间而是 host
    failure（unless already failed），每个 grant 后仍检查 kernel 时钟等于
    `committed_end_fs`（nb D5/D7）
  - `RamTarget` 的 AT 模式只在 `--nb` 下注册 `nb_transport_fw`，每条路径都在
    `BEGIN_REQ` 调用内按 `b_transport` 的同一存储、原子表、版本时钟与 hint 应用访问一次，
    之后阶段只携带时序；阶段经 `peq_with_cb_and_phase` 调度，零时通知之间保留一个 delta，
    `accepted`/`updated`/`early`/`annotated`/`completed` 分别实现 D8 的返回与注记；
    `BEGIN_RESP` 以 `TLM_COMPLETED` 应答即结束 target 事务，forward `END_RESP` 回
    `TLM_COMPLETED`。事务从 `BEGIN_REQ` 到 target 发出或收到最终阶段期间，
    `transport_reset` 在失效或清空任何内容之前拒绝确认；退出时报告 `BEGIN_REQ` 与
    已应用访问计数
  - `scripts/test_systemc_smoke.sh` 让两个 guest 在五条路径上分别以 λ = 0 与 λ = 1500000 fs
    运行，要求 native 退出码、逐字节相同 commit log、非零且等于已应用计数的 `BEGIN_REQ`、
    每个 grant 一次时钟检查；λ = 0 的 grant trace 必须与 parity 逐字节相同，λ > 0 从
    host 退出总量检查 D10 的三项求和。另加 `--nb --dmi` 在 `accepted`/`completed` 的
    λ = 0 parity 与 `accepted` λ = 1500000 的 AT 计数严格低于非 direct pass、
    `--nb --quantum` 拒绝以及非阻塞用法错误；既有 blocking、DMI、quantum、早期停止与
    quantum fault 控制保持不变。U3 不拥有 mutation-runner 控制

- **Non-blocking U2: C ABI 挂起、payload 与 grant 字段（issue #217）**
  - `ruscv_target` 末尾新增 `int32_t suspends_host_time`（nb D6），结构由 56 字节增至 64 字节：非零表示该 target
    允许在 `may_suspend` 事务里让 host 时间流逝。表在 create 时复制，因此该声明跨 `ruscv_machine_reset` 保留，
    NULL 表选择永不挂起的原生路由。挂起表与非零 `quantum_ticks` 的组合在 `ruscv_machine_create` 即以
    `RUSCV_ERROR_CONFIG` 拒绝（`host::validate_quantum`），因为 grant 内部流逝的 host 时间会使 kernel 时钟不再是
    整个 grant 的量子锚点
  - `ruscv_payload` 末尾新增请求字段 `int32_t may_suspend` 与响应字段 `uint64_t host_elapsed_fs`（nb D3/D4），
    结构由 256 字节增至 272 字节。`src/capi/target.rs` 把适配器的 `may_suspend` 复制进每个 payload，并把回调
    报告的 `host_elapsed_fs` 复制回来——包括因载荷被改动而变成 `Incomplete` 的那条路径，它的 `e` 仍按 D4 计费。
    两者都是时序元数据：地址、宽度、字节、类别与状态从不依赖它们
  - `ruscv_grant` 在 `facts` 之后新增 `uint64_t host_elapsed_fs`（nb D5），结构由 128 字节增至 136 字节：本 grant
    的 `W`，即该代累积 host 时间从 grant 起点到返回的增量而非累计值；每个 grant 都给出该字段，未运行 turn 的
    grant 为 0。宿主等待 `consumed_fs - host_elapsed_fs` 后其时钟仍停在 `committed_end_fs`。`OwnedMachine` 新增只读
    `transport_host_elapsed_femtoseconds`，在 grant 起止两处读取该代总量
  - 头文件把线程规则收窄（nb D6）：payload 带 `may_suspend` 的 `b_transport` 回调可以挂起调用进程并让 host 时间
    流逝，只要它在创建线程上恢复并返回。挂起期间线程局部回调标志保持置位，因此同一线程上其他进程进入 ABI 且
    到达回调检查的每个调用仍以 `RUSCV_ERROR_REENTRANT` 拒绝（`ruscv_machine_destroy` 对活句柄亦然），唯一的
    例外仍是 `ruscv_machine_invalidate_direct_mem`；先失败参数检查的调用与 `ruscv_machine_destroy(NULL)` 保持
    既有结果，其他 OS 线程的调用仍是 `RUSCV_ERROR_NOT_CREATOR_THREAD`。`src/capi/target.rs` 的 `SAFETY` 注释
    同步收窄，头文件的 Layout 规则与各结构 "final field" 文本改指新的末尾字段
  - 布局 pin 随之移动：`the_direct_memory_fields_are_final_in_their_structs`（`src/capi/tests.rs`）与
    `tests/capi/smoke.c` 的布局检查都改指新的末尾字段，并各自新增 `ruscv_grant` 的大小与末尾字段 pin。既有
    offset 不变，但尺寸改变，因此 ABI 与上一版头文件不二进制兼容：宿主必须针对新头文件重新编译后才能链接本库
  - 新增 8 个 crate 内测试覆盖合同 §4 U2 的验收项：挂起表下 guest 访问（含一次 load）的 `may_suspend` 为 1，
    同一 guest 在表标志清零时为 0；两个 grant 各自只报告自己的 `host_elapsed_fs`（不是累计值），且运行了 turn
    之后的零 turn grant 报告 0；`e` 为 1.5 tick 时 `consumed_fs - host_elapsed_fs` 等于 turn tick 数加上每次
    访问半 tick；失败（unknown completion）的 grant 同样给出 `W` 且 `W <= consumed_fs`；被改动的载荷使响应变成
    `Incomplete` 时仍带回并计费该访问的 `e`；挂起表配 quantum 4 在 create 被拒；声明跨 reset 保留；回调内
    `ruscv_machine_run_grant` 与 `ruscv_machine_destroy` 返回 `RUSCV_ERROR_REENTRANT` 而
    `ruscv_machine_invalidate_direct_mem` 仍被接受，这正是被挂起回调留下的状态
  - `tests/capi/smoke.c` 以纯 C 重复标志、每 grant 的 host elapsed（同一代内的两个有界 grant，第二个只报自己的
    增量）、reset 保留、拒绝与布局检查：`suspends_host_time` 为 1 的 C target 报告脚本化的 `e`（1.5 tick），
    不做真实挂起
  - U2 拥有的 3 条负控制（`nb-u2-grant-elapsed`、`nb-u2-payload-flag`、`nb-u2-create-quantum`）注册进
    `scripts/debug/stage6-mutations`，每条由已提交测试在变异下失败、还原后通过

- **Non-blocking U1: Rust 侧挂起（issue #216）**
  - `BlockingTarget` 新增 `suspends_host_time() -> bool`（nb D2），默认 `false`，因此既有实现者照常编译且行为不变；
    `ZeroDelayRam` 保持默认，`TlmRamAdapter` 在构造时读取一次。挂起 target 与非零 `MachineConfig.quantum` 的组合
    是既有的 `MachineError::UnsupportedTransport`，该变体的消息加宽为命名本情形：grant 内部流逝的 host 时间会使
    kernel 时钟不再是整个 grant 的量子锚点 `E`。挂起 target 可与 `dmi` 组合；`MachineConfig` 与 CLI 都不新增
    字段或选项
  - `BlockingTransaction` 新增只读请求访问器 `may_suspend() -> bool`（nb D3；适配器在发起时按 target 声明、
    `BlockingInitiator::Hart` 与开启的 guest-turn 窗口三项设定，其余代码无法设置），以及
    `host_elapsed() -> Femtoseconds` 与 `set_host_elapsed(Femtoseconds)`（nb D4；回调内流逝的精确飞秒，起始为零）。
    两者都是时序元数据：地址、宽度、字节、类别与状态从不依赖它们，`ZeroDelayRam` 忽略前者。`DebugInbound` 的文档
    注释改为只命名系统总线访问，因为 program buffer 与 abstract command 走 Hart 端口
  - `src/blocking/adapter.rs` 的响应分类合并为一个 helper，plain 与 atomic 两条响应路径共用，按 nb D4 的六步执行：
    未知完成（panic 或 outcome-unknown 标志）先行并毒化传输，随后变更载荷，随后该路径的 committed-write 记录与
    D7 store-conditional gate，随后 unmarked-elapsed 规则与 elapsed-above-delay 规则，最后沿用今天的状态分类。
    终端响应（含 guest 可见 fault 与 `GenericError`）计费 `d`，`Incomplete` 与未知完成计费 `e`；规则诊断是报告的
    分类，计费引发的 overshoot 或记账错误只追加在其后。`account_delay` 仍是唯一记账路径，并只在 guest-turn 窗口内
    把计费事务的 `e` 加入 `host_elapsed_femtoseconds`
  - `BoundaryFacts` 新增 `host_elapsed_femtoseconds: u64`（按代累计，与 `transport_delay_femtoseconds` 同源）；
    其手写 `Default` 与两处构造（`src/machine/mod.rs` 中 `..BoundaryFacts::default()` 结尾的 turn 字面量，以及
    `src/machine/owned.rs` 的 `control_boundary`）都显式给出该字段。该公开字段破坏外部穷举结构体字面量。其余公开
    签名不变
  - 新增 17 个 crate 内测试覆盖合同 §4 U1 的验收项：guest fetch、load、store、AMO、LR、SC 与 Sv39 walk 读、
    A/D envelope 携带 `may_suspend` true；同一 target 上的 SBA 读、abstract-command 内存访问与 program-buffer
    读携带 false，非挂起 target 的每个 guest 请求也携带 false，open turn 内只有 Hart 请求携带该标志，而
    SBA 与 program-buffer 访问报告的 `e` 都是 unmarked-elapsed 规则；`e` 与 `d` 取不同 tick 数时的每条 D4
    规则、计费金额、`platform_mtime`、`transport_delay_femtoseconds` 与按代累计的
    `host_elapsed_femtoseconds`（两个 turn）；`mcycle` 与 `minstret` 不变；SBA 报告的 `e` 不进入总量（在
    `control_boundary` 读取，因为 `DebugBoundary` 不携带 `BoundaryFacts`）；`dmi` 下的直接拷贝不产生事务也没有
    标志；带 `quantum` 的挂起 target 被拒；`e` 全为零时与不挂起的同一镜像逐 turn 相同；以及
    `begin_transport_turn` 只有一个 guest-turn 调用点
  - U1 拥有的 12 条负控制（`nb-u1-*`）注册进 `scripts/debug/stage6-mutations`，每条由已提交测试在变异下失败、
    还原后通过

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

- **Quantum U6: 退出审计（issue #206）**
  - 按 D14 校正文档：README 的 quantum 段落说明 U1–U5 已全部实现，并引用实现测试与脚本；CLI `--quantum`
    帮助说明该选项即使取 0 也要求 `--transport tlm`；`docs/verification/README.md` 更新变异 runner 的 unit
    数与合同状态；`docs/integration/systemc-tlm.md` 保持为偏移、grant、sync 与上界规则的规范说明，并写入 D8 的
    定时器延迟上界（`Q` <= 1 为零、`Q` >= 2 为 `Q - 1 + t`）与零延迟例外，开放项只余非阻塞传输；C ABI
    header 的 `quantum_ticks` 说明区分 0（关闭）、1（逐边界同步且精确）与 ≥2（显式降低精度）
  - 补齐 §4 的其余负控制：`quantum-u4-local-offset-off`、`quantum-u4-local-offset-after-flush`、
    `quantum-u4-local-offset-charged`、`quantum-u2-debug-delay-offset`、`quantum-u4-grant-past-sync`、
    `quantum-u4-grant-past-jump` 与 `quantum-u4-boundary-reached` 注册进
    `scripts/debug/stage6-mutations`；`quantum-u2-debug-delay-offset` 由新增的
    `machine::tests::quantum_debug_transport_delay_does_not_enter_the_offset` 观察。每条都由已提交测试在变异下
    失败、还原后通过
  - 修复 `scripts/debug_attach.sh` 的 OpenOCD 版本检查：在 `pipefail` 下 `openocd --version | head -1`
    可能因 SIGPIPE 把已找到的 0.12.0 误报为版本不匹配（CI 的 debug-attach 作业曾因此失败一次）；现在先
    捕获首行再匹配，版本输出只打印一次
  - 本单元不改变仿真行为、A10 oracle、认证推迟或合同文本；CI 的 `Quality and tests`、`Debug mutation
    controls`、`Debug attach` 与 `SystemC transport smoke` 作业继续覆盖完整质量门、guest suite、C ABI、
    SystemC、工具脚本与 mutation runner

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

- **TLM-Modeled Devices U2：`MachineConfig` 新增公开字段**
  - `MachineConfig.devices` 是公开字段（device D2），因此外部代码的 struct literal 构造不再编译；改用
    `MachineConfig::new` 之后给字段赋值。既有字段名与含义不变，`BlockingTarget`、
    `ruscv_machine_create` 与 `BlockingTransaction` 的请求字段同样不变

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
