# ISS and Virtual Platform Architectural Prior Art

**Status:** Research

**Authority:** Informational; this note does not accept an ADR or create a product commitment

**Evidence reviewed:** 2026-09-04

## Purpose

This note compares public architectural evidence from instruction set simulators (ISSs), full-system emulators, virtual platforms (VPs), reference ISA models, and a simulation integration standard. It asks how mature systems divide:

- architectural state and instruction semantics;
- platform composition and lifecycle;
- physical transactions and fault classification;
- architectural events, platform events, and run stops;
- instruction budgets, virtual time, and scheduling;
- debugging, tracing, and other observation; and
- fast execution, direct memory access, and temporal decoupling.

The comparison is intended to test the boundaries in [ADR-0001](../architecture/decisions/0001-hart-execution-outcome-and-observation.md), [ADR-0002](../architecture/decisions/0002-physical-access-transaction-and-fault.md), and [ADR-0003](../architecture/decisions/0003-runner-machine-and-platform-ownership.md), and to bound the later interrupt, time, and stop-event decision. The repository's [architecture principles](../architecture/principles.md), [active milestone](../dev-plan.md), source code, and verified tests remain authoritative.

This is not a feature comparison, benchmark, compatibility claim, or recommendation to reproduce another project's API. It does not assess ISA compliance or claim that a named product's architecture is internally uniform beyond the evidence cited here.

## Evidence method and terminology

### Evidence classes

Statements in the system profiles use these labels:

- **Documented fact (DF):** stated by the project's or vendor's public documentation.
- **Source observation (SO):** directly visible in the cited public source revision, but not necessarily promised as a stable API.
- **Project interpretation (PI):** an inference for `ruscv-sim`; it is not a claim made by the cited system.
- **Not established (NE):** the reviewed authoritative public evidence was insufficient for a narrower claim.

The comparison tables and design bounds are **PI** unless a cell says otherwise. They synthesize the labeled evidence; they are not substitutes for the profiles or citations.

### Comparison vocabulary

Terms are normalized for comparison rather than borrowed from any one product:

- **Architectural engine:** state and behavior that implement the guest ISA, including architectural traps and translation.
- **Platform:** the physical address space, memory, devices, interrupt sources, and host-facing target behavior.
- **Composition root:** the owner that instantiates and connects processors, buses, memory, and devices.
- **Run controller:** the owner that starts execution, applies limits, receives stop facts, and returns control to a caller.
- **Physical transaction:** a routed fetch, read, write, or atomic access below guest virtual translation.
- **Architectural fault:** a guest-visible exception condition.
- **Simulator failure:** a failure of the model, adapter, or host that is not guest architecture.
- **Stop fact:** a reason execution reached a control boundary; it need not itself be the policy decision that terminates a run.
- **Virtual time:** modeled time advanced by a scheduler, distinct from host wall time and from a raw retired-instruction count.

These categories expose boundaries that projects name differently. A close vocabulary match does not imply identical semantics.

## Executive findings

1. **Composition and ISA semantics are consistently separable.** QEMU, gem5, Renode, Arm Fast Models/FVPs, and the Sail emulator all have a platform or harness layer around processor semantics. Spike is more compact, but still separates `processor_t` from `sim_t`, the routed bus, and devices. No reviewed system provides evidence that a platform should own a second copy of ISA semantics.
2. **Run control is broader than a Hart/CPU result.** Mature systems represent debugger stops, limits, guest shutdown, event-loop exits, and internal failure outside the instruction semantics even when their concrete APIs do not expose one clean taxonomy.
3. **Transactions and architectural faults are related but not identical.** QEMU has explicit transaction result categories and target-specific CPU fault mapping; gem5 separates packets from ISA `Fault` objects; TLM has transport response statuses but no ISA mapping. This supports a normalized physical result below a Hart-owned cause mapping.
4. **Time is not safely modeled as “one instruction equals one cycle.”** Spike and the Sail harness use deliberately coarse instruction-linked time; gem5, Renode, SystemC/TLM, QEMU, and Fast Models expose event time, budgets, or quanta. Fast Models explicitly documents the accuracy/performance cost of temporal decoupling.
5. **Precise observation must coexist with a bypassable fast path.** Spike, QEMU TCG, Renode, Sail, and Fast Models all pay additional costs or introduce synchronization when fine-grained debugging/tracing is enabled. Observation semantics should be stable, but inactive sinks must not force per-instruction allocation or MMIO interception.
6. **Acceleration is an execution strategy, not an ownership rewrite.** Translation blocks, direct memory interfaces, alternate CPU models, KVM, and temporal decoupling preserve a surrounding machine/platform boundary and require explicit exits, invalidation, or synchronization.
7. **No reviewed design can be copied wholesale.** Spike is intentionally compact; gem5 optimizes for detailed event simulation; QEMU and Renode have much wider product surfaces; Sail prioritizes executable specification; Fast Models and Simics expose proprietary products; SystemC/TLM is an integration standard rather than an ISA/platform implementation.

## System profiles

### Spike

Evidence is from `riscv-software-src/riscv-isa-sim` at commit [`4ffd6ba860f4190ceac2716fa3c2cf139e85538f`](https://github.com/riscv-software-src/riscv-isa-sim/tree/4ffd6ba860f4190ceac2716fa3c2cf139e85538f).

- **Ownership — SO:** [`sim_t`](https://github.com/riscv-software-src/riscv-isa-sim/blob/4ffd6ba860f4190ceac2716fa3c2cf139e85538f/riscv/sim.h#L30-L125) owns the processor collection, routed bus, devices, debug module, HTIF-facing run entry, and simulator configuration. [`processor_t::step`](https://github.com/riscv-software-src/riscv-isa-sim/blob/4ffd6ba860f4190ceac2716fa3c2cf139e85538f/riscv/execute.cc#L206-L360) owns architectural execution, interrupt/trap entry, retirement accounting, debug entry, and the processor fast/slow path.
- **Lifecycle — SO:** `sim_t` constructs processors and devices, has a simulator reset, forwards processor reset to the debug module, and delegates its run to HTIF. This is a compact simulator composition root rather than a reusable machine lifecycle protocol.
- **Transactions and faults — SO:** [`bus_t::load` and `bus_t::store`](https://github.com/riscv-software-src/riscv-isa-sim/blob/4ffd6ba860f4190ceac2716fa3c2cf139e85538f/riscv/devices.cc#L13-L98) route an address and byte width to an `abstract_device_t` and return a Boolean completion. Architectural memory traps are thrown and handled inside processor execution. The bus result does not expose a rich, transport-neutral fault taxonomy.
- **Events and stops — SO:** processor traps and debug entry are handled inside `processor_t`; HTIF owns the host interaction used by `sim_t::run`; the simulator loop also has an instruction limit and remote-bitbang polling. Those mechanisms are distinguishable in code, but not returned as one immutable per-step outcome record.
- **Time — SO:** [`sim_t::step`](https://github.com/riscv-software-src/riscv-isa-sim/blob/4ffd6ba860f4190ceac2716fa3c2cf139e85538f/riscv/sim.cc#L352-L386) rotates among Harts at a fixed `INTERLEAVE` of 5000 instructions and advances the real-time clock device coarsely. This is deterministic functional time, not device-accurate scheduling.
- **Observability — SO:** processor commit logging records instruction identity plus register and memory activity when enabled; the processor's slow-path predicate includes commit logging, histogramming, triggers, WFI, single-step, and debug. Observation is close to the semantic owner, but its public shape is a logging mechanism rather than the `CommitRecord`/`TrapRecord` boundary proposed here.
- **Acceleration — SO:** execution uses decoded instruction handlers and an instruction-cache path; Spike does not present a general host-code translation or timed VP layer in this evidence. Optional observation moves execution to a slower path.
- **PI:** Spike supports keeping ISA semantics and trap entry in the Hart while leaving devices and host exit outside it. Its Boolean device access and coarse run loop are useful simplicity references, not sufficient contracts for target/device fault classification, causal platform events, or a future timed adapter.

### QEMU 10.1.3, including RISC-V and Arm system emulation

Evidence is from QEMU tag [`v10.1.3`](https://github.com/qemu/qemu/tree/v10.1.3). RISC-V and Arm were both inspected to avoid treating one target's implementation as universal.

- **Ownership — DF/SO:** QEMU documents its [QEMU Object Model](https://github.com/qemu/qemu/blob/v10.1.3/docs/devel/qom.rst) as the object/type foundation. [`MachineState`/`MachineClass`](https://github.com/qemu/qemu/blob/v10.1.3/include/hw/boards.h#L175-L440) own machine composition and the selected accelerator; generic [`CPUState`](https://github.com/qemu/qemu/blob/v10.1.3/include/hw/core/cpu.h#L405-L586) is specialized by target state and target callbacks. The [RISC-V virtual machine](https://github.com/qemu/qemu/blob/v10.1.3/hw/riscv/virt.c) and [Arm virtual machine](https://github.com/qemu/qemu/blob/v10.1.3/hw/arm/virt.c) compose different platforms while their CPUs use the common machine, memory, and accelerator infrastructure.
- **Lifecycle — DF/SO:** object initialization and realization establish the graph; QEMU's [Resettable interface](https://github.com/qemu/qemu/blob/v10.1.3/docs/devel/reset.rst) defines assert/hold/release phases and hierarchical child reset. Machine configuration, reset, and VM run state are distinct concerns rather than one CPU step API.
- **Transactions and faults — DF/SO:** [physical memory is an acyclic graph of `MemoryRegion` objects rooted by `AddressSpace`](https://github.com/qemu/qemu/blob/v10.1.3/docs/devel/memory.rst#L17-L25). [`MemTxResult`](https://github.com/qemu/qemu/blob/v10.1.3/include/exec/memattrs.h#L82-L91) distinguishes success, device error, decode error, and access denial. Target callbacks perform architectural mapping; for example, [`riscv_cpu_do_transaction_failed`](https://github.com/qemu/qemu/blob/v10.1.3/target/riscv/cpu_helper.c#L1682-L1703) classifies a failed physical transaction using the original access type.
- **Events and stops — SO:** [`runstate.h`](https://github.com/qemu/qemu/blob/v10.1.3/include/system/runstate.h) exposes separate requests for reset, suspend, wakeup, shutdown with cause/code, powerdown, debug, VM stop, host signal, and guest panic. TCG returns from translated execution for exceptions, interrupts, exit requests, instruction-count exhaustion, and other control conditions; the outer system loop interprets them.
- **Time — SO:** QEMU distinguishes virtual clocks, device clock trees, host time, and instruction-count budgeting. TCG's [`cpu-exec.c`](https://github.com/qemu/qemu/blob/v10.1.3/accel/tcg/cpu-exec.c) carries per-CPU instruction budgets, exits when an `icount` budget expires, and coordinates virtual clock advancement; this is not equivalent to a target architectural cycle counter.
- **Observability — DF/SO:** CPU classes expose state dump, GDB register access, breakpoints, and single-step; QEMU also supplies tracing and monitor/control surfaces. These cross CPU, device, and run-control layers rather than forcing all observation through an instruction result.
- **Acceleration — DF/SO:** [TCG](https://github.com/qemu/qemu/blob/v10.1.3/docs/devel/tcg.rst) translates guest basic blocks, links compatible blocks, and invalidates translated code when required. `MachineState` selects an accelerator, allowing TCG or supported hardware acceleration behind common machine/CPU/device surfaces.
- **Target cross-check — SO:** both [`RISCVCPU`](https://github.com/qemu/qemu/blob/v10.1.3/target/riscv/cpu.c) and [`ARMCPU`](https://github.com/qemu/qemu/blob/v10.1.3/target/arm/cpu.c) plug target reset, MMU, interrupt, debug, and exception behavior into common `CPUState`; they do not put RISC-V and Arm semantics in `MachineState` or `MemoryRegion`.
- **PI:** QEMU strongly supports separate machine composition, physical routing, target-owned architectural fault mapping, and outer run-state policy. Its APIs are broader and more coupled to dynamic translation/migration than this project needs; adopting the ownership pattern does not require adopting QOM, TCG internals, or QEMU's exact transaction statuses.

### gem5 25.0.0.1

Evidence is from gem5 tag [`v25.0.0.1`](https://github.com/gem5/gem5/tree/v25.0.0.1) and the official [event-driven programming documentation](https://www.gem5.org/documentation/learning_gem5/part2/events/).

- **Ownership — DF/SO:** Python configuration builds a graph of C++ `SimObject`s. [`SimObject`](https://github.com/gem5/gem5/blob/v25.0.0.1/src/sim/sim_object.hh#L74-L218) is event-managed, serializable, drainable, and part of an object tree. Boards/systems compose CPUs, memory systems, and devices; CPU models implement architectural execution through thread contexts and ports.
- **Lifecycle — SO:** [`m5.instantiate`](https://github.com/gem5/gem5/blob/v25.0.0.1/src/python/m5/simulate.py#L75-L181) creates the configured graph, initializes it, restores a checkpoint or calls `initState`, and later calls `startup`. Draining quiesces objects before checkpointing or model changes. This is a richer lifecycle than construct/reset/run alone.
- **Transactions and faults — SO:** request and response [`Port`s](https://github.com/gem5/gem5/blob/v25.0.0.1/src/mem/port.hh#L134-L267) support functional, atomic, and timing requests; timing rejection requires retry. [`Packet`](https://github.com/gem5/gem5/blob/v25.0.0.1/src/mem/packet.hh#L76-L176) has explicit error responses. Separately, [`FaultBase`](https://github.com/gem5/gem5/blob/v25.0.0.1/src/sim/faults.hh#L58-L155) and ISA-specific fault classes represent architectural faults. A packet error is therefore not itself a universal guest exception.
- **Events and stops — DF/SO:** gem5 is event-driven; events have callbacks and are scheduled at ticks. [`simulate`](https://github.com/gem5/gem5/blob/v25.0.0.1/src/sim/simulate.cc#L187-L264) installs a bounded simulation-limit event and returns the global event that exited the loop. The Python [`Simulator`](https://github.com/gem5/gem5/blob/v25.0.0.1/src/python/gem5/simulate/simulator.py#L537-L580) maps exit events to policy handlers.
- **Time — SO:** [`EventQueue`](https://github.com/gem5/gem5/blob/v25.0.0.1/src/sim/eventq.hh#L95-L170) orders events by tick and priority, including explicit same-tick tie breakers. Multiple event queues and synchronization support parallel simulation. CPU instruction progress and simulation ticks are related by the selected CPU/memory model, not collapsed into one quantity.
- **Observability — DF/SO:** statistics, probes, debug flags, event traces, state serialization, and checkpointing observe different layers. Functional accesses support inspection without pretending to be timing transactions.
- **Acceleration — DF/SO:** gem5 offers CPU models with different speed/detail trade-offs, including atomic, timing, out-of-order, and KVM-backed execution where supported. The model can be switched only through lifecycle/drain rules; faster execution does not erase the surrounding event and port model.
- **PI:** gem5 is the clearest evidence that event scheduling, run-exit policy, transaction protocol, and architectural faults are separate contracts. Its timing protocols are intentionally more detailed than the minimum ISS baseline; `ruscv-sim` should preserve room for them without requiring gem5-style packets or a detailed event queue in every Hart step.

### Renode

Evidence combines current public documentation with pinned source from `renode-infrastructure` commit [`0374a356cc06bcac7f285fd6c130806b9eb33951`](https://github.com/renode/renode-infrastructure/tree/0374a356cc06bcac7f285fd6c130806b9eb33951), referenced by the Renode tree at commit [`63d4e2dd52717666f70c9900317654dd7ce5b2f4`](https://github.com/renode/renode/tree/63d4e2dd52717666f70c9900317654dd7ce5b2f4).

- **Ownership — DF/SO:** an Emulation contains Machines; a [`Machine`](https://github.com/renode/renode-infrastructure/blob/0374a356cc06bcac7f285fd6c130806b9eb33951/src/Emulator/Main/Core/Machine.cs#L38-L126) creates and owns its `SystemBus`, peripheral tree, optional local time source, lifecycle, and debug resources. The [platform description format](https://renode.readthedocs.io/en/latest/advanced/platform_description_format.html) describes peripheral instances, registration points, and connections independently of the CPU translation library.
- **Lifecycle — DF/SO:** the Machine has start, pause, reset, abort, preserved-state, and dispose paths. Reset obtains a paused state and resets registered resettable peripherals. [`LoadELF`](https://github.com/renode/renode-infrastructure/blob/0374a356cc06bcac7f285fd6c130806b9eb33951/src/Emulator/Main/Core/Extensions/FileLoaderExtensions.cs#L417-L459) requires the Machine to be paused, chooses ELF physical or virtual segment addresses by explicit option, writes through the bus, and separately updates CPU ELF metadata.
- **Transactions and faults — SO:** `SystemBus` routes typed-width accesses, supports optional width translation, bus hooks, and unmapped-access policy. Under `ThrowException`, an unmapped access raises typed [`BusAccessException(BusAccessError.AddressError)`](https://github.com/renode/renode-infrastructure/blob/0374a356cc06bcac7f285fd6c130806b9eb33951/src/Emulator/Main/Peripherals/Bus/SystemBus.cs#L2494-L2517); other configured policies can report, suppress, or return a default value. Translation CPUs catch bus errors and map them through CPU-specific handling. Renode therefore demonstrates both explicit bus faults and the risk of policy-driven silent behavior.
- **Events and stops — SO:** [`ICPU`](https://github.com/renode/renode-infrastructure/blob/0374a356cc06bcac7f285fd6c130806b9eb33951/src/Emulator/Main/Peripherals/CPU/ICPU.cs#L18-L50) exposes halt state/events, step, bus, and a time handle. [`TranslationCPU.ExecuteInstructions`](https://github.com/renode/renode-infrastructure/blob/0374a356cc06bcac7f285fd6c130806b9eb33951/src/Emulator/Peripherals/Peripherals/CPU/TranslationCPU.cs#L703-L768) returns distinct statuses for normal progress, WFI, external MMU fault, breakpoint, watchpoint, and interruption. Machine pause/abort remain separate lifecycle/control states.
- **Time — DF/SO:** Renode's [time framework](https://renode.readthedocs.io/en/latest/advanced/time_framework.html) uses time sources and sinks with virtual-time quanta. Pinned source shows [`TimeSourceBase`](https://github.com/renode/renode-infrastructure/blob/0374a356cc06bcac7f285fd6c130806b9eb33951/src/Emulator/Main/Time/TimeSourceBase.cs#L26-L60) owning elapsed virtual time and a quantum, while [`TimeHandle`](https://github.com/renode/renode-infrastructure/blob/0374a356cc06bcac7f285fd6c130806b9eb33951/src/Emulator/Main/Time/TimeHandle.cs#L140-L217) grants a bounded interval that a sink must report back.
- **Observability — DF/SO:** Renode documents [GDB integration](https://renode.readthedocs.io/en/latest/debugging/gdb.html) and [execution tracing](https://renode.readthedocs.io/en/latest/execution-tracing/execution-tracing.html); the bus supports access hooks/logging and the Machine preserves debugger breakpoints/watchpoints across saved state.
- **Acceleration — DF/SO:** translation CPUs execute instruction budgets through a native translation library and exit blocks for breakpoints/watchpoints or interruption. Renode also documents [HDL co-simulation](https://renode.readthedocs.io/en/latest/advanced/co-simulating-with-an-hdl-simulator.html), including socket and in-process Verilator integration, behind platform peripherals or CPU integration rather than a second command-line policy layer.
- **PI:** Renode most closely resembles the target Machine/Platform shape and provides useful evidence for pause-before-image-mutation, explicit CPU budgets, and virtual-time grants. Its CPU-as-peripheral hierarchy, configurable unmapped-access behavior, and public API names should not be copied as semantic decisions. In particular, a compatibility default value is not a substitute for ADR-0002 fault classification.

### Sail RISC-V executable model and emulator harness

Evidence is from `riscv/sail-riscv` commit [`abeec0f2eb20b5508b756c37e7274a7e5919ac15`](https://github.com/riscv/sail-riscv/tree/abeec0f2eb20b5508b756c37e7274a7e5919ac15).

- **Ownership — DF/SO:** the [README](https://github.com/riscv/sail-riscv/blob/abeec0f2eb20b5508b756c37e7274a7e5919ac15/README.md#L24-L42) says Sail sources generate a C++ model that a separate C++ harness wraps into `sail_riscv_sim`. The model defines instruction semantics. The [`c_emulator` overview](https://github.com/riscv/sail-riscv/blob/abeec0f2eb20b5508b756c37e7274a7e5919ac15/README.md#L324-L346) documents ELF loading, platform devices, the physical map, and RVFI integration; [GDB support](https://github.com/riscv/sail-riscv/blob/abeec0f2eb20b5508b756c37e7274a7e5919ac15/README.md#L126-L127) is documented separately; and pinned [`ModelImpl` source](https://github.com/riscv/sail-riscv/blob/abeec0f2eb20b5508b756c37e7274a7e5919ac15/c_emulator/riscv_model_impl.h#L30-L159) exposes callback registration and generated-model callback overrides.
- **Lifecycle — SO:** the harness initializes platform constants, loads and validates ELF segments against configured memory regions, discovers symbols and HTIF metadata, initializes the Sail model at the entry point, runs, and then writes optional signatures/memory dumps and finalizes. It is an emulator lifecycle, not a general reset/reuse protocol.
- **Transactions and faults — SO:** [`ModelImpl`](https://github.com/riscv/sail-riscv/blob/abeec0f2eb20b5508b756c37e7274a7e5919ac15/c_emulator/riscv_model_impl.h#L19-L180) implements generated model platform callbacks for physical memory, memory exceptions, traps, retirement, page-table walks, registers, and reservations. Sail's [`errors.sail`](https://github.com/riscv/sail-riscv/blob/abeec0f2eb20b5508b756c37e7274a7e5919ac15/model/prelude/errors.sail#L9-L30) separately defines model-internal exceptions for unimplemented, internal-error, and reserved behavior.
- **Events and stops — SO:** the harness loop stops on a Sail internal exception, HTIF completion, or instruction limit, and GDB/RVFI/callback integrations can control stepping. These are explicit in the harness, while guest trap semantics remain in the generated model.
- **Time — SO:** the pinned [`run_sail` loop](https://github.com/riscv/sail-riscv/blob/abeec0f2eb20b5508b756c37e7274a7e5919ac15/c_emulator/riscv_sim.cpp#L422-L513) calls `tick_clock` after a configured number of instructions or on a waiting step while WFI remains active. [`ModelImpl::tick_clock`](https://github.com/riscv/sail-riscv/blob/abeec0f2eb20b5508b756c37e7274a7e5919ac15/c_emulator/riscv_model_impl.cpp#L513-L515) delegates to the generated clock function, whose [Sail definition](https://github.com/riscv/sail-riscv/blob/abeec0f2eb20b5508b756c37e7274a7e5919ac15/model/sys/platform.sail#L229-L241) increments `mtime` by one per tick. This is a deliberate reference-platform policy, not a claim of hardware timing fidelity.
- **Observability — DF/SO:** generated callbacks expose instruction fetch, memory read/write/exception, register/CSR/PC changes, trap, retirement, page-table walk, and TLB activity. The harness can emit RVFI and specification-coverage data. These facts originate close to the semantic model rather than from post-step register snapshots.
- **Acceleration — DF:** Sail can generate executable emulators and theorem-prover definitions from one specification. The reviewed sequential emulator is a reference execution path; it does not establish a VP-oriented translation-block or temporal-decoupling contract.
- **PI:** Sail strongly supports one semantic source with a separate platform/run harness and explicit internal-error separation. Its fine-grained callbacks are evidence about available semantic facts, not a recommendation to let external observers execute re-entrant callbacks during a partially applied `ruscv-sim` instruction.

### Arm Fast Models and Fixed Virtual Platforms

Evidence is from Arm's public, non-confidential [Fast Models Reference Guide 11.32, document 100964](https://documentation-service.arm.com/documentation/100964/1132/) and [Fast Models Fixed Virtual Platforms Reference Guide 11.28, document 100966](https://documentation-service.arm.com/documentation/100966/1128/). Section and page references below use those editions.

- **Ownership — DF:** Programmer's View processor and device components supply functionally accurate models; FVPs are prebuilt platform compositions, while the Fast Models package supplies source examples and tools for customization. FVP component instances and parameters are hierarchical. Most FVPs expose CADI, MTI, and Iris debug/trace interfaces (FVP Guide §§1, 2.2–2.6).
- **Lifecycle — DF:** an FVP executable accepts hierarchical configuration, application/image loading, optional debugger-server startup, immediate or debugger-controlled run, reset through the model/debug surfaces, and shutdown. `--application` loads ELF or S-record images; `--data` performs raw placement; `--start` overrides the initial PC (FVP Guide §§2.3–2.5). The public guide exposes product controls rather than a transport-neutral lifecycle API.
- **Transactions and faults — DF:** PVBus is the Fast Models memory-like bus protocol. Masters and slaves transact through components and decoders; slave ranges may be memory-like, device-like, abort, or ignore. Transactions are normally atomic unless a device explicitly blocks them. PVBus is functionally accurate but intentionally does not reproduce detailed hardware bus traffic or contention (Fast Models Guide §§1.4.7–1.4.8, pp. 32–34).
- **Events and stops — DF:** FVP command-line controls distinguish wall-clock CPU limit, cycle limit, wall-clock run limit, simulated-time limit, program breakpoints, and debug-server waiting/running. Core status distinguishes running, externally halted, WFE/WFI standby, reset, dormant, and shutdown (FVP Guide §§2.3, 2.7).
- **Time — DF:** Fast Models explicitly prioritizes speed and programmer-visible functional behavior over instruction and bus timing. Processors execute a quantum at one simulation-time point; dynamic quantum adapts to the next SystemC event and a minimum synchronization latency. Non-DMI transactions, signal changes, WFE/WFI, barriers, and selected timer accesses can force earlier synchronization (Fast Models Guide §§1.4.2–1.4.6, pp. 28–32). FVP rate limiting is a separate option that aligns simulated and wall-clock progress for interactive I/O (FVP Guide §2.8).
- **Observability — DF:** CADI/Iris provide execution control and register/memory access; MTI and plug-ins provide trace. FVPs can log debug-interface calls and report simulated time, host user/system/wall time, and a performance index. The guide warns that intercepting every PVBus transaction defeats direct-memory optimizations and can force processor single-stepping (Fast Models Guide §1.4.7; FVP Guide §§2.2–2.6).
- **Acceleration — DF:** Code Translation processor components, cached direct access to normal memory, snooping, DMI, and temporal decoupling improve speed. Failure to obtain prefetch/direct access can drop instruction execution to a much slower single-step path (Fast Models Guide §§1.4.7–1.4.8).
- **PI:** Fast Models is the strongest public evidence for treating DMI and temporal decoupling as performance strategies below stable semantic boundaries. It also demonstrates why the later time decision must define mandatory synchronization/exit points and why a trace implementation cannot assume every fast RAM access appears as a normal bus callback.

### Intel Simics public release

Evidence is limited to Intel's public [Intel Simics Simulator product page](https://www.intel.com/content/www/us/en/developer/articles/tool/simics-simulator.html), updated 2023-09-08. Detailed product manuals were not used to infer proprietary interfaces.

- **Ownership — DF:** Intel describes the base product as the simulator core, user interface, framework components, configuration management, and APIs used to build fast functional virtual platforms. The public package includes a Quick-Start Platform, a simple RISC-V target system, processor models, platform models, and the Device Modeling Language compiler.
- **Lifecycle — DF:** the public description identifies target control and checkpointing as base-product capabilities and says the Quick-Start Platform can be configured for processor count/type, memory, disks, networks, and PCIe devices.
- **Transactions and faults — NE:** the reviewed public page does not establish a transport result taxonomy or the boundary that maps device failures to guest architectural faults.
- **Events and stops — NE:** target control is documented at product level, but the reviewed page does not establish distinct stop categories or same-boundary arbitration rules.
- **Time — DF/NE:** a multithreaded scheduler and interfaces to performance, power, and thermal modeling are listed. Exact virtual-time, instruction-budget, and synchronization semantics were not established from this source.
- **Observability — DF:** command-line control, debugging, inspection, tracing, target control, and checkpointing are listed as core capabilities.
- **Acceleration — DF/NE:** Intel describes fast functional simulation and a multithreaded scheduler. The public page is insufficient to attribute a specific translation, DMI, or temporal-decoupling mechanism.
- **PI:** Simics confirms that configuration, inspection, target control, and checkpoints are first-class VP concerns around processor/device models. It is intentionally excluded from narrower API conclusions where only product-level public evidence was available.

### SystemC 3.0.2 and TLM-2.0

Implementation evidence is from the Accellera reference implementation at tag [`3.0.2`](https://github.com/accellera-official/systemc/tree/3.0.2), commit `70b0fc8e4a74acc677b0fc73cea08f940c2115d5`. Accellera's [download page](https://www.accellera.org/downloads/standards/systemc) lists 3.0.2 as the current release. The [README](https://github.com/accellera-official/systemc/blob/3.0.2/README.md#L4-L24) identifies the code as a reference implementation of IEEE Std 1666-2023 and defers to the standard where they differ.

- **Ownership — DF/SO:** SystemC supplies the simulation kernel and component/process model; TLM supplies transaction interfaces and payload conventions. It does not supply a RISC-V Hart, image loader, board, or run-result taxonomy. Those remain responsibilities of an integrating model.
- **Lifecycle — SO:** [`sc_simcontext`](https://github.com/accellera-official/systemc/blob/3.0.2/src/sysc/kernel/sc_simcontext.h) exposes bounded/unbounded `sc_start`, pause, stop, status, current time, and pending-activity queries around elaborated components. Reset and image installation are model-specific, not generic TLM operations.
- **Transactions and faults — SO:** TLM-2.0 defines blocking, nonblocking forward/backward, debug transport, and direct-memory interfaces in [`tlm_fw_bw_ifs.h`](https://github.com/accellera-official/systemc/blob/3.0.2/src/tlm_core/tlm_2/tlm_2_interfaces/tlm_fw_bw_ifs.h#L29-L190). The [`tlm_generic_payload`](https://github.com/accellera-official/systemc/blob/3.0.2/src/tlm_core/tlm_2/tlm_generic_payload/tlm_gp.h#L90-L103) distinguishes OK, incomplete, generic, address, command, burst, and byte-enable statuses. These are transport statuses, not RISC-V trap causes or a guarantee of ADR-0002 atomic envelopes.
- **Events and stops — SO:** the kernel schedules delta and timed events and allows pause/stop or return after a bounded `sc_start`. The generic kernel status identifies lifecycle state, not why a guest, debugger, platform device, or adapter requested a product-level stop.
- **Time — SO:** transport calls carry annotated `sc_time`; [`tlm_quantumkeeper`](https://github.com/accellera-official/systemc/blob/3.0.2/src/tlm_utils/tlm_quantumkeeper.h#L31-L162) tracks initiator-local time and synchronizes when the global quantum is reached. This permits an initiator to run ahead while bounding divergence.
- **Observability — SO:** TLM debug transport requires non-intrusive access with no side effects, waits, or event notifications; SystemC tracing/reporting observe kernel/model state. TLM itself does not define architectural commit or trap records.
- **Acceleration — SO:** [`tlm_dmi`](https://github.com/accellera-official/systemc/blob/3.0.2/src/tlm_core/tlm_2/tlm_2_interfaces/tlm_dmi.h#L27-L109) grants a host pointer over an address range with read/write permission and latency; the backward interface invalidates ranges. Quantum keeping provides temporal decoupling. Neither mechanism is an alternate ISA implementation.
- **PI:** SystemC/TLM is a suitable integration-side vocabulary, not the Hart's canonical semantic API. A future adapter must normalize status, preserve raw bytes and atomic semantics, invalidate DMI, and return delay facts to the Machine-associated scheduler. `sc_stop` cannot replace a structured Runner stop result.

## Comparative synthesis

### Ownership and lifecycle

| Approach | Architectural semantics owner | Platform/composition owner | Run-control owner | Lifecycle character |
| --- | --- | --- | --- | --- |
| Spike | `processor_t` | `sim_t`, `bus_t`, devices | `sim_t` + HTIF | Compact construct/reset/run; limited reusable lifecycle separation |
| QEMU | Target CPU state/helpers under generic `CPUState` | QOM machine/device/address-space graph | Main VM/run-state control | Init/realize, hierarchical reset, run, migration/teardown |
| gem5 | Selected CPU/ISA model and thread context | Configured `SimObject` board/system graph | Python/C++ simulation loop and exit-event handlers | Instantiate, restore/init, startup, drain, checkpoint |
| Renode | CPU/translation library | Emulation/Machine, `SystemBus`, peripherals | Monitor/API plus Machine/CPU control | Create, paused mutation, start/pause/reset/abort/save/dispose |
| Sail RISC-V | Generated executable ISA model | C++ emulator harness/platform callbacks | Harness/GDB/RVFI loop | Initialize/load/run/finalize; not a general reusable Machine protocol |
| Arm Fast Models/FVP | Processor model | Hierarchical FVP component instance graph | FVP shell/debug interfaces | Configure/load/start/reset/control/shutdown |
| Intel Simics | Processor/device models (public detail limited) | Simulator framework and configured VP | Target-control/user-interface layer | Configurable target + checkpointing; detailed phases NE |
| SystemC/TLM | Not supplied | Integrating SystemC model | Embedder calling kernel control | Elaborate/start/pause/stop; reset/image policy not supplied |

**PI:** The target `Frontend → Runner → Machine → {Hart, Platform}` split is a normalized design, not a clone. QEMU, gem5, and Renode show explicit composition roots; Sail shows a particularly clean semantics/harness distinction; SystemC confirms that an external scheduler can drive components without owning their ISA semantics.

### Transactions, faults, and stop facts

| Approach | Physical transaction shape | Fault boundary | Stops/events visible in reviewed evidence |
| --- | --- | --- | --- |
| Spike | Routed address + width + byte buffer; Boolean result | Processor catches architectural traps; bus failure detail is coarse | Trap/debug internal to processor; HTIF, limit, remote control outside |
| QEMU | `AddressSpace`/`MemoryRegion`, attributes, explicit `MemTxResult` | Target CPU maps transaction failure to target exception | Guest/host shutdown causes, reset, suspend, debug, VM stop, panic, TB exits |
| gem5 | Functional, atomic, timing packets with retry and errors | Packet errors and ISA `Fault` objects are separate | Timestamped events, global exit events, simulation limits, user interrupt |
| Renode | Typed bus widths, optional width translation and hooks | Typed bus exception when configured; CPU-specific mapping | WFI, MMU fault, breakpoint, watchpoint, interruption, Machine pause/abort |
| Sail RISC-V | Generated platform callbacks for memory and exception facts | Guest trap callbacks and model-internal exceptions are separate | HTIF, instruction limit, internal exception, GDB/RVFI control |
| Arm Fast Models/FVP | Atomic PVBus transactions; memory/device/abort/ignore ranges | Public guide documents bus behavior, not a reusable ISA fault API | Debug wait/run, breakpoints, several limits, halt/WFI/reset/shutdown states |
| Intel Simics | NE | NE | Product-level target control documented; taxonomy NE |
| SystemC/TLM | Generic payload; blocking/nonblocking/debug; response status | Transport status only; ISA mapping belongs to model | Kernel events and pause/stop; product stop cause not supplied |

**PI:** ADR-0002's normalized result must sit between the QEMU/TLM-style transport result and the Hart's architectural cause mapping. A backend default value (possible in Renode) or ignore range (possible in PVBus) is an explicit platform policy, not an accidental replacement for a physical fault. A generic TLM error cannot by itself decide page fault versus access fault versus simulator failure.

### Time, observability, and acceleration

| Approach | Time/budget model | Observation | Acceleration or speed strategy |
| --- | --- | --- | --- |
| Spike | Instruction interleave + coarse RTC ticks | Optional commit log, histogram, triggers, debug | Cached decoded handlers; optional slow path |
| QEMU | Virtual clocks + device clocks + instruction budgets | GDB, monitor, state dump, breakpoints, tracing | TCG blocks/linking/invalidation; supported hardware accelerators |
| gem5 | Ordered tick event queues; CPU/memory-model timing | Statistics, probes, debug events, checkpoints | Multiple CPU fidelity levels, fast-forwarding, KVM where supported |
| Renode | Virtual-time source/sink grants and quanta | GDB, execution trace, bus hooks/logging, profiler | Translation blocks; host integration; HDL co-simulation |
| Sail RISC-V | Harness step/tick policy | Semantic callbacks, RVFI, spec coverage, trace | Generated executable model; not a timed VP accelerator contract |
| Arm Fast Models/FVP | SystemC time + dynamic quantum + early synchronization | CADI/Iris/MTI, plug-ins, stats | Code translation, direct RAM/DMI, snooping, temporal decoupling |
| Intel Simics | Multithreaded scheduler; details NE | Debug, inspect, trace, checkpoint | Fast functional simulation; mechanism NE |
| SystemC/TLM | Global event time + annotated delays + local quantum | Kernel/model trace and debug transport; no ISA records | DMI with invalidation; temporal decoupling |

**PI:** The recurring high-performance shape is “execute to a budget or mandatory boundary, then return precise facts.” Direct RAM and translated blocks require invalidation; time decoupling requires a deadline/quantum; debugging and detailed trace require earlier exits. These are reasons to stabilize Hart outcomes and Machine scheduling ports before optimizing them.

## Cross-check of the proposed ADRs

Research cannot accept a Proposed ADR. The verdicts below mean “consistent with the reviewed prior art and suitable to retain for architecture review,” not “implemented” or “normative.”

### ADR-0001 — Hart execution outcome and observation records

**Verdict: retain unchanged; supported by the reviewed evidence.**

- Spike, QEMU RISC-V/Arm, gem5, and Sail all keep architectural trap/exception handling with the CPU/ISA model rather than with a generic physical bus.
- QEMU and gem5 demonstrate why transport failure and architectural fault are separate stages. Sail's internal-error union demonstrates why unsupported/internal model failure must not be fabricated as a guest trap.
- Spike and Sail expose architectural effects close to execution; neither justifies Runner-side opcode re-fetch or post-hoc register comparison as the authoritative record.
- Translation-block systems preserve earlier completed work and exit at precise boundaries for traps, debug, invalidation, or budget exhaustion. This is compatible with ADR-0001's one-record-per-retired-instruction rule for future blocks.
- Fast Models' documented performance penalty for complete interception reinforces, rather than weakens, ADR-0001's separation between semantic facts and optional sink/presentation cost.

**Implementation caution, not an ADR change:** inactive observers need a zero- or low-allocation fast path, while enabled observers receive the same completed Hart facts. Internal effect collection may be optimized, but an observer must not become a re-entrant instruction callback.

### ADR-0002 — Physical-access transaction and fault contract

**Verdict: retain unchanged; supported, with its stronger guarantees treated as capabilities to verify rather than assumptions about backends.**

- QEMU's address-space routing and `MemTxResult`, gem5's packet/error protocols, Renode's typed bus errors, PVBus abort/ignore ranges, and TLM response statuses all support an explicit physical result below ISA mapping.
- QEMU's target-specific transaction-failure callbacks support retaining original access kind in the Hart when selecting an architectural exception.
- TLM generic payload does not by itself guarantee RISC-V AMO/LR/SC semantics, all-or-nothing device effects, or a guest-versus-host failure classification. ADR-0002 is correct to require an adapter and explicit atomic capability instead of naming TLM as the Hart API.
- Renode width translation and policy-controlled unmapped reads are useful compatibility features, but they confirm that silent splitting/defaulting is policy that must not leak through a strict semantic port.
- SystemC DMI and Fast Models direct-memory optimizations confirm that DMI must remain behind routing, side-effect, atomicity, and invalidation rules.

**Verification consequence:** each backend must prove or reject its advertised atomicity, failed-operation side-effect, and DMI invalidation guarantees. The contract must not infer those guarantees from the presence of a transport API.

### ADR-0003 — Runner, Machine, and Platform ownership

**Verdict: retain the ownership direction unchanged; supported by all applicable systems.**

- QEMU, gem5, Renode, Fast Models/FVP, and Simics expose a composition/configuration layer around CPUs and devices. Sail and Spike show the same distinction in smaller harnesses.
- QEMU run state, gem5 exit events, Renode control states, and FVP limits all show that terminal policy is wider than a CPU trap or bus callback.
- Renode's paused ELF loading and Sail's harness loader support keeping host-side image placement out of guest instruction execution. FVP's separate `--application`, `--data`, and `--start` controls reinforce the distinction between bytes, placement, and architectural entry state.
- No evidence supports moving RISC-V translation, trap selection, or retirement into the Platform, or allowing a device to construct the final frontend result.
- gem5 drain/checkpoint and Renode pause/reset show that coherent lifecycle/inspection boundaries matter once asynchronous devices and external controls exist.

**Implementation caution, not an ADR change:** ADR-0003's “fresh reset restores installed image-owned mutable storage” is stronger than a bare peripheral reset in several reviewed systems. A backend must implement snapshot/reinstall semantics or explicitly decline a fresh-rerun guarantee; ordinary component reset must not be mislabeled as image restoration.

### Overall consistency verdict

The three Proposed ADRs are mutually consistent with the public evidence. No concrete contradiction was found. The research supports these non-negotiable joins:

1. physical transport reports facts; Hart classifies architectural meaning;
2. Hart completes retirement or trap entry before immutable observation crosses the boundary;
3. Machine composes and provides a coherent lifecycle; Runner owns outer policy; and
4. acceleration and SystemC/TLM remain replaceable strategies/adapters around the same Hart semantics.

The later interrupt/time/stop decision should refine scheduling and arbitration only. It should not reopen those ownership decisions.

## Bounds for the interrupt, time, and stop-event decision

### Fixed by existing architecture

The later decision must preserve these already-proposed boundaries:

- An accepted interrupt enters the Hart before fetch at the defined step boundary and produces `TrapEntered`, not a retired instruction.
- A synchronous guest exception, platform event, debugger stop, execution limit, observer failure, and simulator failure remain distinguishable.
- A successful `tohost`/platform-exit write completes and retires before its causal platform-exit event becomes terminal at the Runner.
- Physical delay is metadata until a Machine-associated scheduler consumes it; a device or Hart does not advance global time unilaterally.
- Runner owns the outer loop and terminal policy. Machine or its associated scheduler may execute a budgeted quantum but must return precise per-step architectural facts and causal platform events.
- Future block execution cannot hide commits before a trap/stop or turn prefetched/speculative work into observation.

### Minimum semantic contract to decide

The later ADR should define a Machine execution exchange equivalent in meaning to:

```text
request:
  instruction/step budget
  virtual-time deadline or no deadline
  control state (continue, single-step, stop requested)

response:
  ordered Hart outcomes and architectural observations
  ordered causal Platform events
  instructions/steps consumed
  modeled-time/delay consumed
  next pending event/deadline information
  control-boundary reason(s)
```

This is semantic pseudocode, not a Rust layout. It keeps instruction progress, time progress, and stop provenance independent.

### Required time properties

1. **Declared quantities:** keep at least retired instructions, Hart attempts/steps, architectural counter deltas, virtual-time delta, and host elapsed time conceptually distinct. A configuration may relate them, but must name the relation.
2. **Monotonic virtual time:** use a declared resolution and conversion rule; no component may report time from the future as already globally committed or move committed time backward.
3. **Deadline-aware budgeting:** a scheduler must not grant a block/quantum beyond the next mandatory platform event, timer deadline, debugger precision point, or Runner bound unless it has a defined rollback mechanism. The baseline should avoid requiring rollback.
4. **Delay ownership:** accumulate optional physical delays separately from load data and fault status. The time decision must say whether successful accesses, guest-visible physical faults, and simulator failures consume modeled time.
5. **Idle behavior:** WFI itself follows ISA retirement semantics; subsequent waiting is a Hart/run state, not a stream of fake retired instructions. The scheduler may jump virtual time to the next eligible event only under a defined idle rule.
6. **Deterministic ordering:** events at the same virtual timestamp need a stable ordering key, such as `(timestamp, class priority, insertion sequence)`. Exact classes are deferred, but nondeterministic container iteration is not an acceptable policy.
7. **Host input:** asynchronous host/device input must be timestamped or otherwise ordered at a synchronization boundary if deterministic replay is promised.

### Required interrupt properties

1. Platform/device models own line assertion and deassertion; Machine wiring presents normalized inputs to the Hart.
2. Hart owns eligibility, masking, delegation, architectural priority, trap entry, and trap observation.
3. The sampling boundary is before instruction fetch for a single step. A block strategy must expose an equivalent precise exit before executing an instruction that should instead be preempted.
4. Interrupt assertion is a Platform fact; interrupt acceptance is a Hart outcome. They may occur at different scheduler boundaries and must not share one Boolean state transition.
5. Pending line state, queued edge events, and claimed/in-service controller state require explicit reset and inspection semantics.

### Required stop properties

The result must preserve, at minimum, these source categories even if the Runner also selects one primary reason for presentation:

- architectural trap encountered under a Runner policy that stops on traps;
- successful Platform exit;
- debugger breakpoint/watchpoint/single-step completion;
- external/user stop request;
- instruction/step budget exhausted;
- virtual-time deadline reached;
- observer/reporting failure after a completed outcome;
- simulator/internal/adapter failure; and
- normal quiescence/no runnable work, if the selected product supports it.

A stop is sampled only at a coherent architectural boundary. Already completed outcomes remain valid. An asynchronous stop request may shorten the next quantum, but it may not retroactively unretire an instruction.

### Simultaneous-condition constraints

The later ADR must choose a deterministic primary-result rule, but it must not discard co-incident facts. At minimum:

| Boundary case | Required preservation |
| --- | --- |
| Interrupt eligible before fetch and instruction budget remains | Interrupt trap enters; no instruction commit for that step. |
| Instruction retires and causes a Platform exit while the budget becomes exhausted | Preserve the commit, Platform exit event, and exhausted-budget fact; never report an unretired exit write. |
| Synchronous trap and a debugger stop request become visible at the same return boundary | Preserve completed trap entry and the external request; policy selects whether execution remains stopped. |
| Observer fails after delivery of a completed commit/trap | Preserve the architectural outcome; report observer failure separately. |
| Physical completion is unknown after a possible external side effect | Simulator failure with uncertain-state context; no fabricated commit, trap, or successful Platform event. |
| Timer/device event and external stop share a timestamp | Apply documented deterministic ordering and retain both facts if both become eligible. |

A single lossy `Stopped(bool)` or “first callback wins” mechanism cannot satisfy these cases.

### Choices intentionally left to that ADR

The public evidence narrows but does not answer these project choices:

1. the relation between instruction progress, architectural `mcycle`/`minstret`, and virtual time in the minimal ISS configuration;
2. whether a guest trap is normally continued or surfaced as a Runner terminal reason for each product mode;
3. the exact same-timestamp priority among Platform exit, debugger request, limit, timer event, and external stop after preserving all facts;
4. maximum host-stop latency and therefore the largest interruptible block/quantum;
5. whether and how an idle Hart advances directly to the next event;
6. time units, precision, overflow behavior, and rounding of adapter delays; and
7. replay requirements for asynchronous host input.

These are bounded scheduling/arbitration questions. They do not include ownership of ISA semantics, physical routing, image loading, or final result policy.

### Verification scenarios for the later contract

A future decision should be testable with at least:

- interrupt asserted before a step, during a multi-instruction budget, and at the same timestamp as a timer deadline;
- zero instruction budget, exact last-instruction budget, and budget plus causal `tohost` exit;
- WFI with no pending event, WFI with a future timer, and WFI awakened by an external interrupt;
- a successful access with zero, absent, and nonzero delay; a physical fault with delay; and an adapter failure with unknown completion;
- breakpoint/watchpoint and user stop during interpreted and block execution;
- enabled/disabled observation producing identical Hart outcomes and final architectural state;
- same-timestamp events inserted in different host orders producing the declared deterministic result; and
- native and TLM-adapted Platform runs producing equivalent raw transaction, fault, event, and time facts within the chosen abstraction.

## Patterns to adopt and traps to avoid

### Adopt as architectural constraints

- One architectural engine with platform-specific composition around it.
- A normalized physical result with explicit target fault versus simulator failure.
- Budgeted execution that returns at mandatory interrupt/event/debug/invalidation boundaries.
- A coherent pause/reset/inspect boundary before image or debugger mutation.
- Event provenance and possibly multiple co-incident stop facts in the Runner result.
- Optional observation sinks fed by semantic facts from completed Hart transitions.
- Explicit DMI range, permissions, latency, and invalidation beneath the same physical contract.

### Do not import without a project decision

- Spike's Boolean bus result or fixed instruction interleave.
- QEMU's QOM hierarchy, `MemTxResult` values, or translation-block internals.
- gem5's detailed timing packet protocol and event-queue topology.
- Renode's CPU-as-peripheral organization, automatic width translation, or default unmapped-access policy.
- Sail's concrete callback ABI or harness time rule.
- Arm proprietary PVBus/CADI/Iris APIs or FVP timing defaults.
- Simics APIs not established by public evidence.
- TLM generic payload as the Hart API, or `sc_stop` as the application result.

## Evidence limitations and exclusions

- This is an architecture study, not runtime validation. No external simulator was built or benchmarked for this note.
- Public source observations are snapshots, not compatibility guarantees. Version pins identify exactly what was inspected.
- Arm and Simics evidence is public vendor documentation, but implementation source is proprietary. Arm's public guides provide substantial semantic detail; the reviewed Simics page supports only product-level conclusions.
- Renode documentation links track current public documentation, while implementation claims cite exact source commits.
- Imperas/OVP is relevant prior art, but this review did not obtain sufficiently reliable authoritative public material to support the same seven-dimension analysis. It is omitted rather than described from mirrors or unverified marketing summaries.
- Absence of a feature from this note means only **NE from reviewed evidence**, not that the product lacks it.

## Evidence register

| System | Exact evidence baseline | Principal public evidence |
| --- | --- | --- |
| Spike | `riscv-isa-sim` `4ffd6ba860f4190ceac2716fa3c2cf139e85538f` | `sim.h`, `sim.cc`, `execute.cc`, `devices.cc`, `processor.h` at the linked commit |
| QEMU | `v10.1.3`, commit `93be9e6bd43e460a6d497aa96c282c5b5acf4d06` | QOM, memory, reset, TCG docs; machine/CPU/run-state/transaction sources; RISC-V and Arm target paths |
| gem5 | `v25.0.0.1`, commit `ddd4ae35adb0a3df1f1ba11e9a973a5c2f8c2944` | `SimObject`, ports/packets/faults, event queue, simulation loop, Python lifecycle/Simulator; official event guide |
| Renode | Renode `63d4e2dd52717666f70c9900317654dd7ce5b2f4`; infrastructure `0374a356cc06bcac7f285fd6c130806b9eb33951` | Official Machine/platform/time/control/trace/GDB/co-simulation docs and pinned Machine/bus/CPU/time/loader source |
| Sail RISC-V | `abeec0f2eb20b5508b756c37e7274a7e5919ac15` | README, generated-model wrapper, emulator harness, main/platform/error Sail sources |
| Arm Fast Models/FVP | [Fast Models Guide 11.32, document `100964`](https://documentation-service.arm.com/documentation/100964/1132/); [FVP Guide 11.28, document `100966`](https://documentation-service.arm.com/documentation/100966/1128/) | Public non-confidential Arm Documentation Service guides |
| Intel Simics | Public release page, updated 2023-09-08 | Intel product description only; narrower internals marked NE |
| SystemC/TLM | SystemC 3.0.2, commit `70b0fc8e4a74acc677b0fc73cea08f940c2115d5` | Accellera reference implementation and standards download page |

## Conclusion

The reviewed systems converge on a durable principle: processor semantics, physical platform behavior, composition/lifecycle, and outer run policy are different concerns even when a compact implementation stores several of them in one object. Rich systems add explicit event time, lifecycle quiescence, transport status, and observation surfaces; fast systems add blocks, direct memory, and quanta, but then require precise exits and invalidation.

For `ruscv-sim`, that evidence supports the direction already proposed in ADR-0001/0002/0003. The next architecture decision should define a deterministic, deadline-aware Machine scheduling exchange and a non-lossy stop arbitration model. It should not move architectural semantics out of the Hart, make TLM canonical, or create a second ISS path for the VP.
