#!/usr/bin/env python3
"""Independent assembly/algorithm specification for calibration-workloads/1.

No simulator, instruction decoder or learned results. Every executed slot and
its effect is declared from the family algorithm; linked opcodes supply ONLY
instruction identity. Original public-v1 sources/oracle are never written.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import struct
from integrity import canonical, loads, require

ROOT=Path(__file__).resolve().parents[2]
BASE=0x80000000
MASK=(1<<64)-1
ANCHORS=('fib','sw','hello','amo_w','amo_d','lrsc_loop','sc_after_store_failure','control_loop','ram_loop','mixed_w','mixed_d','native_device')
FAMILIES={'fib':'control','control_loop':'control','sw':'ram','ram_loop':'ram','hello':'uart','native_device':'uart',
          'amo_w':'mixed-w','mixed_w':'mixed-w','amo_d':'mixed-d','mixed_d':'mixed-d','lrsc_loop':'mixed-d','sc_after_store_failure':'mixed-d'}
ITERATIONS={'control':128,'ram':64,'mixed-w':24,'mixed-d':24,'uart':64}
SOURCE_DIR='tools/a10/calibration-fixtures'
ORACLE='tools/a10/calibration-oracle-v2.json'
WORKLOAD='tools/a10/calibration-workloads-v2.json'
PAYLOAD_BYTES=524288
ZERO_FILL_BYTES=8192

def raw(value,width):return list((value&((1<<(8*width))-1)).to_bytes(width,'little'))
def mem(address,width,read=None,write=None,atomic=None,conditional=None):
    return dict(address=address,width=width,read=None if read is None else raw(read,width),write=None if write is None else raw(write,width),atomic=atomic,conditional=conditional,aq=False,rl=False)

class Specification:
    def __init__(self):self.code={};self.trace=[];self.regs=[0]*32
    def put(self,pc,asm,writes=(),memory=(),reservation=None,next_pc=None):
        require(pc%4==0 and (pc not in self.code or self.code[pc]==asm),'inconsistent static instruction slot')
        self.code[pc]=asm
        changes=[]
        for rd,value in writes:
            require(0<=rd<32,'register index')
            value &= MASK
            if rd:changes.append(dict(index=rd,before=self.regs[rd],after=value));self.regs[rd]=value
        self.trace.append(dict(pc=BASE+pc,next_pc=BASE+(pc+4 if next_pc is None else next_pc),gpr=changes,memory=list(memory),reservation=reservation))
        return pc+4
    def li(self,pc,rd,value):
        require(-(1<<31)<=value<(1<<31),'bounded immediate')
        if -2048<=value<=2047:return self.put(pc,f'addi x{rd}, x0, {value}',[(rd,value)])
        hi=(value+2048)>>12;lo=value-(hi<<12)
        pc=self.put(pc,f'lui x{rd}, {hi&0xfffff}',[(rd,hi<<12)])
        return self.put(pc,f'addi x{rd}, x{rd}, {lo}',[(rd,value)])
    def address(self,pc,rd,address):
        distance=address-(BASE+pc);hi=(distance+2048)>>12;lo=distance-(hi<<12)
        require(-(1<<19)<=hi<(1<<19) and -2048<=lo<=2047,'address expansion overflow')
        pc=self.put(pc,f'auipc x{rd}, {hi&0xfffff}',[(rd,address-lo)])
        return self.put(pc,f'addi x{rd}, x{rd}, {lo}',[(rd,address)])
    def branch_check(self,pc,rs1,rs2):return self.put(pc,f'bne x{rs1}, x{rs2}, fail')
    def source(self,fixed):
        require(set(self.code)==set(range(0,max(self.code)+4,4)),'static source gap')
        lines=['# calibration-workloads/1: hand-declared deterministic family work.','.option norvc','.option norelax','.section .text','.globl _start','_start:']
        for pc,asm in sorted(self.code.items()):lines += [f'p{pc}:',f'    {asm}']
        lines += ['halt: j halt','fail:','    addi a0, x0, 41']
        if fixed:lines += ['    lui t6, 0x40008']
        else:lines += ['    la t6, tohost']
        lines += ['    sd a0, 0(t6)','failed: j failed']
        if not fixed:lines += ['.section .tohost,"aw",@progbits','.balign 8','tohost: .dword 0']
        lines += ['.section .signature,"aw",@progbits','.balign 8','signature: .dword 0,0,0','.section .data','.balign 8','neighbors: .dword 0x123','.section .work_bss,"aw",@nobits','.balign 8','scratch: .space 16']
        return '\n'.join(lines)+'\n'

def specification(anchor):
    family=FAMILIES[anchor];n=ITERATIONS[family]
    require(5<=n<=128,'versioned bounded iteration count')
    p=Specification();pc=0;signature=BASE+0x2000;scratch=BASE+0x3008;uart=[];events=[]
    pc=p.address(pc,8,0x10000000 if family=='uart' else scratch) # two explicit slots even UART
    pc=p.address(pc,9,signature)
    pc=p.li(pc,18,n);pc=p.li(pc,19,0);pc=p.li(pc,20,0)
    if family.startswith('mixed'):
        pc=p.li(pc,6,-2);pc=p.li(pc,7,5);pc=p.li(pc,28,3)
    elif family=='uart':pc=p.li(pc,6,65)
    elif family=='ram':
        pc=p.put(pc,'ld x28, 0(x8)',[(28,0)],[mem(scratch,8,read=0)])
        pc=p.branch_check(pc,28,0)
    loop=pc
    for i in range(1,n+1):
        pc=loop
        if family=='control':
            pc=p.put(pc,'addi x20, x20, 1',[(20,i)])
            pc=p.put(pc,'add x19, x19, x20',[(19,i*(i+1)//2)])
        elif family=='ram':
            pc=p.put(pc,'addi x20, x20, 1',[(20,i)])
            pc=p.put(pc,'sw x20, 0(x8)',memory=[mem(scratch,4,write=i)])
            pc=p.put(pc,'lw x28, 0(x8)',[(28,i)],[mem(scratch,4,read=i)])
            pc=p.branch_check(pc,28,20)
            pc=p.put(pc,'add x19, x19, x28',[(19,i*(i+1)//2)])
        elif family=='uart':
            pc=p.put(pc,'sb x6, 0(x8)',memory=[mem(0x10000000,1,write=65)])
            uart.append(65);events.append(['uart',65])
            pc=p.put(pc,'add x19, x19, x6',[(19,i*65)])
            pc=p.put(pc,'addi x20, x20, 1',[(20,i)])
        else:
            width=4 if family=='mixed-w' else 8;suffix='w' if width==4 else 'd';store='sw' if width==4 else 'sd';load='lw' if width==4 else 'ld'
            pc=p.put(pc,f'{store} x6, 0(x8)',memory=[mem(scratch,width,write=-2)])
            pc=p.put(pc,f'amoadd.{suffix} x11, x7, (x8)',[(11,-2)],[mem(scratch,width,read=-2,write=3,atomic='rmw')])
            pc=p.branch_check(pc,11,6)
            pc=p.put(pc,f'lr.{suffix} x12, (x8)',[(12,3)],[mem(scratch,width,read=3,atomic='lr')],[False,True])
            pc=p.branch_check(pc,12,28)
            pc=p.put(pc,f'sc.{suffix} x13, x6, (x8)',[(13,0)],[mem(scratch,width,write=-2,atomic='sc',conditional='success')],[True,False])
            pc=p.branch_check(pc,13,0)
            pc=p.put(pc,f'lr.{suffix} x14, (x8)',[(14,-2)],[mem(scratch,width,read=-2,atomic='lr')],[False,True])
            pc=p.branch_check(pc,14,6)
            pc=p.put(pc,f'{store} x28, 0(x8)',memory=[mem(scratch,width,write=3)])
            pc=p.put(pc,f'sc.{suffix} x15, x7, (x8)',[(15,1)],[mem(scratch,width,atomic='sc',conditional='failure')],[True,False])
            pc=p.li(pc,29,1);pc=p.branch_check(pc,15,29)
            pc=p.put(pc,f'sc.{suffix} x0, x7, (x8)',memory=[mem(scratch,width,atomic='sc',conditional='failure')])
            pc=p.put(pc,f'{load} x16, 0(x8)',[(16,3)],[mem(scratch,width,read=3)])
            pc=p.branch_check(pc,16,28)
            pc=p.put(pc,'add x19, x19, x16',[(19,i*3)])
            pc=p.put(pc,'addi x20, x20, 1',[(20,i)])
        pc=p.put(pc,'addi x18, x18, -1',[(18,n-i)])
        pc=p.put(pc,f'bne x18, x0, p{loop}',next_pc=loop if i<n else pc+4)
    checksum=n*(n+1)//2 if family in ('control','ram') else n*65 if family=='uart' else n*3
    pc=p.li(pc,29,checksum);pc=p.branch_check(pc,19,29)
    pc=p.put(pc,'addi x0, x0, 9') # explicit suppressed destination, not a timing loop
    pc=p.put(pc,'sd x20, 0(x9)',memory=[mem(signature,8,write=n)])
    pc=p.put(pc,'sd x19, 8(x9)',memory=[mem(signature+8,8,write=checksum)])
    last=n if family=='ram' else 3 if family.startswith('mixed') else 0
    pc=p.address(pc,31,BASE+0x3000)
    pc=p.put(pc,'ld x5, 0(x31)',[(5,0x123)],[mem(BASE+0x3000,8,read=0x123)])
    pc=p.li(pc,29,0x123);pc=p.branch_check(pc,5,29)
    pc=p.put(pc,'ld x5, 8(x31)',[(5,last)],[mem(scratch,8,read=last)])
    pc=p.li(pc,29,last);pc=p.branch_check(pc,5,29)
    pc=p.put(pc,'ld x5, 16(x31)',[(5,0)],[mem(scratch+8,8,read=0)])
    pc=p.branch_check(pc,5,0)
    reader=len(p.trace)
    pc=p.put(pc,'csrr x30, minstret',[(30,reader)])
    pc=p.put(pc,'sd x30, 16(x9)',memory=[mem(signature+16,8,write=reader)])
    pc=p.li(pc,10,7)
    fixed=anchor=='native_device';signal=0x40008000 if fixed else BASE+0x1000
    pc=p.address(pc,31,signal)
    pc=p.put(pc,'sd x10, 0(x31)',memory=[mem(signal,8,write=7)])
    if fixed:events.append(['htif',7])
    # Independent slot-count formulas include expanded checksum immediate and
    # suppressed-x0 check, all neighbor reads, reader/signature and exit store.
    fixed_count={'control':29,'ram':31,'mixed-w':31,'mixed-d':31,'uart':30}[family]
    per_iteration={'control':4,'ram':7,'mixed-w':20,'mixed-d':20,'uart':5}[family]
    require(len(p.trace)==fixed_count+n*per_iteration,'hand-derived slot count disagreement')
    return p,dict(work=n,checksum=checksum,signature=raw(n,8)+raw(checksum,8)+raw(reader,8),uart=uart,events=events,
                  ram=[dict(offset=0x3000,bytes=raw(0x123,8)+raw(last,8)+[0]*8)],fixed=fixed,family=family,iterations=n,
                  formula=f'{fixed_count} + {n} * {per_iteration} = {len(p.trace)}; MINSTRET reader precedes final six retirements')

LINKER_HEAD='''/* calibration-workloads/1; existing native/flat RAM placement only. */
ENTRY(_start)
SECTIONS {
 . = 0x80000000; .text : { *(.text*) }
 . = 0x80001000; .tohost : { *(.tohost) }
 . = 0x80002000; .signature : { *(.signature) }
 . = 0x80003000; .data : { *(.data*) }
 .work_bss (NOLOAD) : { *(.work_bss) }
'''
EXEC_LINKER=LINKER_HEAD+'}\n'
LOAD_LINKER=LINKER_HEAD+f' . = 0x80004000; .payload : {{ FILL(0xa5a5a5a5); BYTE(0xa5); . += {PAYLOAD_BYTES-1}; }}\n .payload_bss (NOLOAD) : {{ . += {ZERO_FILL_BYTES}; }}\n}}\n'

def mappings():
    return {'schema':'calibration-workloads/1','version':2,'oracle':'a10-calibration-oracle/1','oracle_version':2,
            'anchor_oracle_sha256':hashlib.sha256((ROOT/'tools/a10/public-v1.json').read_bytes()).hexdigest(),
            'phase_policy':'execution ELF identical across off/facts/file; separate initialized-payload ELF for load_only; zero Hart turns',
            'load_work':{'initialized_payload_bytes':PAYLOAD_BYTES,'payload_byte':165,'zero_fill_bytes':ZERO_FILL_BYTES,'maximum_memory_bytes':1048576},
            'mapping':[{'anchor':a,'family':FAMILIES[a],'iterations':ITERATIONS[FAMILIES[a]],'execution':'cal-'+a+'-exec','load':'cal-'+a+'-load-v2','source':SOURCE_DIR+'/'+a+'.S','execution_linker':SOURCE_DIR+'/execute.ld','load_linker':SOURCE_DIR+'/load-v2.ld','derivation':specification(a)[1]['formula']} for a in ANCHORS]}

def seed():
    old=loads((ROOT/'tools/a10/public-v1.json').read_bytes());header={k:v for k,v in old.items() if k!='fixtures'}
    header.update(schema='a10-calibration-oracle/1',version=2)
    fixtures=[]
    for mapping in mappings()['mapping']:
        for role in ('execution','load'):
            fixtures.append({'id':mapping[role],'source':mapping['source'],'linker':mapping[role+'_linker'],'native_only':mapping['family']=='uart'})
    return dict(header,fixtures=fixtures)

def derive(build):
    manifest=seed();report=loads((build/'build.json').read_bytes())
    require(set(report['fixtures'])=={f['id'] for f in manifest['fixtures']},'new variant build inventory')
    for f in manifest['fixtures']:
        anchor=next(m['anchor'] for m in mappings()['mapping'] if f['id'] in (m['execution'],m['load']));p,decl=specification(anchor)
        require((ROOT/f['source']).read_text()==p.source(decl['fixed']),'source differs from declared assembly/algorithm')
        elf=(build/(f['id']+'.elf')).read_bytes()
        opcodes={int(a,16):int(b,16) for a,b in re.findall(r'^\s*([0-9a-f]+):\s+([0-9a-f]{8})\s',(build/(f['id']+'.dis')).read_text(),re.M)}
        trace=[dict(t,instruction=opcodes[t['pc']]) for t in p.trace]
        phoff=struct.unpack_from('<Q',elf,32)[0];phnum=struct.unpack_from('<H',elf,56)[0];segments=[];file_offsets=[]
        for i in range(phnum):
            typ,flags,off,va,pa,filesz,memsz,align=struct.unpack_from('<IIQQQQQQ',elf,phoff+56*i)
            if typ==1:
                segments.append(dict(address=va,physical_address=pa,flags=flags,file_size=filesz,zero_fill=memsz-filesz));file_offsets.append(off)
        shoff=struct.unpack_from('<Q',elf,40)[0];shnum,shstr=struct.unpack_from('<HH',elf,60)
        names_off,names_size=struct.unpack_from('<QQ',elf,shoff+shstr*64+24);names=elf[names_off:names_off+names_size];sigoff=None
        for i in range(shnum):
            hdr=shoff+64*i;noff=struct.unpack_from('<I',elf,hdr)[0]
            if names[noff:].split(b'\0')[0]==b'.signature':sigoff=struct.unpack_from('<Q',elf,hdr+24)[0]
        span=max(s['address']+s['file_size']+s['zero_fill'] for s in segments)-BASE;size=max(65536,1<<(span-1).bit_length())
        require(size<=1048576 and sigoff is not None,'RAM/signature bounds')
        if f['id'].endswith('-load-v2'):
            # Offline exact initialized-byte/zero-fill placement, no simulator.
            payload=[i for i,s in enumerate(segments) if s['address']<=BASE+0x4000<s['address']+s['file_size']]
            require(len(payload)==1,'payload load segment')
            i=payload[0];off=file_offsets[i]+BASE+0x4000-segments[i]['address']
            require(elf[off:off+PAYLOAD_BYTES]==bytes([165])*PAYLOAD_BYTES,'initialized payload oracle')
            require(sum(s['zero_fill'] for s in segments)>=ZERO_FILL_BYTES+16,'declared zero fill')
        f.update({k:report['fixtures'][f['id']][k] for k in ('source_sha256','linker_sha256','elf_sha256')})
        f.update(entry=BASE,final_pc=trace[-1]['next_pc'],exit_code=3,process_code=3,attempts=len(trace),turns=len(trace),retirements=len(trace),traps=0,
                 work=decl['work'],checksum=decl['checksum'],signature_addr=BASE+0x2000,signature=decl['signature'],uart=decl['uart'],events=decl['events'],ram=decl['ram'],regs=p.regs,trace=trace,
                 segments=segments,tohost=None if decl['fixed'] else BASE+0x1000,memory_size=size,signature_file_offset=sigoff)
    require_tool_identities(report,manifest)
    return manifest


def require_tool_identities(report,manifest):
    """The ALWAYS-on integrity guarantee is the outcome-level check of the
    declared assembly slots/algorithm/effects against the actually built ELF
    specs; that is what catches any toolchain difference that changes
    encodings. Tool identity comparisons (version AND binary sha256) are the
    pinned-container evidence convention and apply ONLY under
    RISCV_REQUIRE_A10_PINNED_TOOLS; outside it the build's actually recorded
    tool identities are retained without comparison against the pinned ARM64
    manifest strings (no per-arch manifests)."""
    require(report['tools'].keys()==manifest['tool_versions'].keys(),'pinned producer inventory')
    if os.environ.get('RISCV_REQUIRE_A10_PINNED_TOOLS'):
        for k,v in report['tools'].items():
            require(v['version']==manifest['tool_versions'][k],'required pinned producer tool version')
            require(v['sha256']==manifest['arm64_tool_sha256'][k],'required pinned producer identities')

if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('operation',choices=('write-sources','derive','check'));parser.add_argument('--build')
    args=parser.parse_args()
    if args.operation=='write-sources':
        directory=ROOT/SOURCE_DIR;directory.mkdir(exist_ok=True)
        for a in ANCHORS:
            p,d=specification(a);path=directory/(a+'.S');expected=p.source(d['fixed'])
            if path.exists():require(path.read_text()==expected,'existing execution source identity cannot be silently rewritten')
            else:path.write_text(expected)
        require((directory/'execute.ld').read_text()==EXEC_LINKER,'existing execution linker cannot be rewritten')
        (directory/'load-v2.ld').write_text(LOAD_LINKER)
        (ROOT/WORKLOAD).write_bytes(canonical(mappings())+b'\n')
        (ROOT/ORACLE).write_bytes(canonical(seed())+b'\n')
    else:
        require(args.build is not None,'fresh build required')
        oracle=derive(Path(args.build));rendered=canonical(oracle)+b'\n'
        require((ROOT/WORKLOAD).read_bytes()==canonical(mappings())+b'\n','mapping/version/iteration/derivation disagreement')
        if args.operation=='derive':(ROOT/ORACLE).write_bytes(rendered)
        else:require((ROOT/ORACLE).read_bytes()==rendered,'new oracle differs from independent linked-source derivation')
