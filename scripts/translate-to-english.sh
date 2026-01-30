#!/bin/bash
# Batch replace Chinese comments with English in Rust files
# Note: Using perl for better regex support

set -e

cd ~/Developer/projects/ruscv

echo "Processing src/core/mod.rs..."
perl -i -pe 's/核心模块/core module/g; s/取指-译码-执行循环/fetch-decode-execute cycle/g; s/特权模式/privilege mode/g; s/核心状态/core state/g; s/通用寄存器 x0-x31/General purpose registers x0-x31/g; s/核心/core/g; s/TLM接口/TLM interface/g; s/设置TLM接口/Set TLM interface/g; s/\/\/ 取迹/\/\/ Fetch/g; s/\/\/ 译码/\/\/ Decode/g; s/\/\/ 执行/\/\/ Execute/g; s/更新PC（由执行器处理，除非发生异常）/Update PC (handled by executor unless exception)/g; s/reset后regs被清零/regs cleared after reset/g; s/x0 始终为0/x0 always returns 0/g' src/core/mod.rs

echo "Processing src/decode/mod.rs..."
perl -i -pe 's/支持RV32I基础指令集的译码/RV32I instruction decoder/g; s/无效的指令编码: 0x\{0:08x\}/Invalid instruction encoding: 0x{0:08x}/g; s/保留指令/Reserved instruction/g; s/未实现的指令/Unimplemented instruction/g; s/指令格式/Instruction format/g; s/寄存器-寄存器/Register-Register/g; s/立即数操作/Immediate operation/g; s/存储/Store/g; s/条件分支/Conditional branch/g; s/长立即数 (LUI, AUIPC)/Long immediate (LUI, AUIPC)/g; s/无条件跳转 (JAL)/Unconditional jump (JAL)/g; s/操作码 (主要操作码)/Opcode (primary)/g; s/功能码 (funct3)/Function code (funct3)/g' src/decode/mod.rs

echo "Processing src/execute/mod.rs..."
perl -i -pe 's/实现RV32I指令的执行逻辑/RV32I instruction execution/g; s/未对齐的内存访问: 地址 0x\{0:08x\}, 对齐要求 \{1\}/Misaligned memory access: addr 0x{0:08x}, alignment {1}/g; s/无效的寄存器访问: x\{0\}/Invalid register access: x{0}/g; s/无效的操作/Invalid operation/g; s/ECALL异常/ECALL exception/g; s/EBREAK异常/EBREAK exception/g; s/内存访问错误: \{0\}/Memory access error: {0}/g; s/Load Upper Immediate/Load Upper Immediate (LUI)/g; s/Add Upper Immediate to PC/Add Upper Immediate to PC (AUIPC)/g; s/Jump and Link/Jump and Link (JAL)/g; s/Jump and Link Register/Jump and Link Register (JALR)/g; s/有符号比较/signed comparison/g; s/无符号比较/unsigned comparison/g; s/实际的分支条件需要根据funct3值修正/Branch conditions need correction based on funct3 value/g; s/R-type 操作指令/R-type operation instructions/g' src/execute/mod.rs

echo "Processing src/memory/mod.rs..."
perl -i -pe 's/无效的内存地址: 0x\{0:08x\}/Invalid memory address: 0x{0:08x}/g; s/未对齐访问: 地址 0x\{0:08x\}, 需要 \{1\}-字节对齐/Misaligned access: addr 0x{0:08x}, requires {1}-byte alignment/g; s/内存访问越界/Memory access out of bounds/g; s/存储器接口 Trait/Memory interface trait/g; s/读取字 (4字节)/Read word (4 bytes)/g; s/读取半字 (2字节)/Read half (2 bytes)/g; s/读取字节 (1字节)/Read byte (1 byte)/g; s/写入字 (4字节)/Write word (4 bytes)/g; s/写入半字 (2字节)/Write half (2 bytes)/g; s/写入字节 (1字节)/Write byte (1 byte)/g; s/存储器数据 (使用RwLock支持线程安全读写)/Memory data (thread-safe R\/W via RwLock)/g' src/memory/mod.rs

echo "Processing src/tlm/mod.rs..."
perl -i -pe 's/TLM2.0 接口抽象层/TLM2.0 interface abstraction/g; s/TLM 传输类型/TLM transaction type/g; s/请求开始/Request begin/g; s/请求结束/Request end/g; s/响应开始/Response begin/g; s/响应结束/Response end/g; s/TLM 响应状态/TLM response status/g; s/成功/Success/g; s/地址错误/Address error/g; s/命令错误/Command error/g; s/突发错误/Burst error/g; s/数据错误/Data error/g; s/无效地址/Invalid address/g; s/等待请求/Wait request/g; s/等待响应/Wait response/g; s/需要释放/Release required/g; s/TLM 命令类型/TLM command type/g; s/TLM 通用事务/TLM generic transaction/g; s/TLM 发起者接口 (Initiator)/TLM initiator interface/g; s/用于发起TLM事务/Used to initiate TLM transactions/g; s/TLM 目标接口 (Target)/TLM target interface/g; s/用于响应TLM事务/Used to respond to TLM transactions/g; s/TLM 通用接口 (用于核心与外部交互)/TLM generic interface (for core-external communication)/g; s/TLM 总线 (连接多个TLM组件)/TLM bus (connects multiple TLM components)/g; s/创建新的TLM总线/Create new TLM bus/g; s/默认10ns延迟/Default 10ns delay/g; s/简单内存 TLM 封装/Simple memory TLM wrapper/g; s/创建新的TLM简单存储器/Create new TLM simple memory/g; s/调试 TLM 接口/Debug TLM interface/g; s/无效的地址: 0x\{0:08x\}/Invalid address: 0x{0:08x}/g; s/无效的传输长度: \{0\}/Invalid transaction length: {0}/g; s/传输超时/Transaction timeout/g; s/总线忙/Bus busy/g; s/未实现/Not implemented/g' src/tlm/mod.rs

echo "Done! All Chinese comments replaced with English."
echo ""
echo "Verifying no Chinese remains..."
if grep -r '[\u4e00-\u9fff]' src/ --include="*.rs" 2>/dev/null; then
    echo "WARNING: Some Chinese still found!"
else
    echo "SUCCESS: No Chinese characters found in src/"
fi
