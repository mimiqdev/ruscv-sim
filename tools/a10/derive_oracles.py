#!/usr/bin/env python3
"""Offline, independently hand-accounted public-v1 oracle derivation.

Reads linked opcode identities, NEVER executes/decodes instructions or reads a
simulator result. Each emit below declares an algorithm/assembly effect directly.
Not used at run time: changing these tables requires oracle-version review.
Usage: python3 tools/a10/derive_oracles.py target/<fresh-build> [--check|--check-artifacts]
--check audits the complete checked-in derivation without changing any file.
--check-artifacts excludes only build-tool identities: identical source/linker/ELF
and semantic/placement expectations remain mandatory. It does not certify tools.
"""
import json
from pathlib import Path
import re
import struct
import sys
from audit_fixtures import merge_tool_sha256

BASE = 0x80000000
MASK = (1 << 64)-1
ROOT = Path(__file__).resolve().parents[2]

BUILD = Path(sys.argv[1])
MANIFEST = ROOT/'tools/a10/public-v1.json'
manifest = json.loads(MANIFEST.read_text())
original_tools = {k:manifest[k] for k in ['tool_versions', 'tool_sha256']}
mode = sys.argv[2:]
if mode not in [[], ['--check'], ['--check-artifacts']]:
    raise ValueError('expected only --check or --check-artifacts')
build = json.loads((BUILD/'build.json').read_text())
# Artifact equivalence ignores producer identity. Only write and strict --check
# replace this platform's pin; an unpinned platform must not be consulted here.
if mode == ['--check-artifacts']:
    tool_sha256 = original_tools['tool_sha256']
elif 'platform' not in build:
    raise ValueError('build.json lacks producer platform (uname -m); rebuild fixtures')
else:
    tool_sha256 = merge_tool_sha256(original_tools['tool_sha256'], build['platform'], build['tools'])
manifest.update(schema='a10-oracle/1', version=1,
    image='ghcr.io/mimiqdev/ruscv-sim-dev@sha256:cc3cfea2499f69d2ee91fc711fb646807a08d8160148c00303d2fa92e3e9a65c',
    source_baseline='e73b12b8467fd635b398382a5cbc7ce75d842f68',
    build_flags=['-march=rv64ima_zicsr','-mabi=lp64'],
    tool_versions={k:v['version'] for k,v in build['tools'].items()},
    tool_sha256=tool_sha256,
    route_matrix={'machine-native':['off','facts','file'], 'machine-flat':['off','facts','file'],
        'native-bytes':['off','file'], 'native-file':['off','file'], 'cli':['off','file'], 'flat':['off']},
    na={'native-bytes/facts':'no public arbitrary sink', 'native-file/facts':'no public arbitrary sink',
        'cli/facts':'no public arbitrary sink', 'flat/facts':'no subscriber toggle', 'flat/file':'no logger',
        'cli/execute_only':'no preload', 'native-bytes/execute_only':'no preload',
        'native-file/execute_only':'no preload', 'all/in_memory_serialized':'deferred; no public writer constructor',
        'native-device/flat':'no flat device map'})

def raw(v,w):
    return list((v & ((1<<(8*w))-1)).to_bytes(w,'little'))
def mem(addr,w,read=None,write=None,atomic=None,conditional=None,aq=False,rl=False):
    return dict(address=addr,width=w,read=None if read is None else raw(read,w),
        write=None if write is None else raw(write,w),atomic=atomic,conditional=conditional,aq=aq,rl=rl)

for f in manifest['fixtures']:
    name=f['id']
    f.update({k:v for k,v in build['fixtures'][name].items() if k.endswith('sha256')})
    opcodes={int(a,16)-BASE:int(b,16) for a,b in re.findall(r'^\s*([0-9a-f]+):\s+([0-9a-f]{8})\s', (BUILD/(name+'.dis')).read_text(),re.M)}
    syms={s:int(a,16) for a,t,s in re.findall(r'^([0-9a-f]+) (\w) (.+)$',(BUILD/(name+'.nm')).read_text(),re.M)}
    regs=[0]*32
    trace=[]
    def emit(pc,writes=(),memory=(),reservation=None,next_pc=None):
        gpr=[]
        for rd,value in writes:
            value &= MASK
            gpr.append(dict(index=rd,before=regs[rd],after=value))
            regs[rd]=value
        trace.append(dict(pc=BASE+pc,instruction=opcodes[pc],next_pc=BASE+(pc+4 if next_pc is None else next_pc),
            gpr=gpr,memory=list(memory),reservation=reservation))
    def seq(start,end,values=None,memories=None,reservations=None,jumps=None):
        for pc in range(start,end,4):
            emit(pc, (values or {}).get(pc,()), (memories or {}).get(pc,()),
                (reservations or {}).get(pc), (jumps or {}).get(pc))
    def exit_tail(start,rd=5):
        seq(start,start+20,{start:[(rd,BASE+start+4096)],start+4:[(rd,BASE+4096)],
            start+8:[(10,0)],start+12:[(10,1)]},
            {start+16:[mem(BASE+4096,8,write=1)]})
    work=1; checksum=0; uart=[]; events=[]; ram=[]; signature=None; signature_addr=None
    if name=='fib':
        seq(0,16,{0:[(1,0)],4:[(2,1)],8:[(3,9)],12:[(4,0)]})
        a,b=0,1
        for i in range(9):
            c=a+b
            seq(16,40,{20:[(5,c)],24:[(1,b)],28:[(2,c)],32:[(4,i+1)]},jumps={36:16})
            a,b=b,c
        emit(16,next_pc=40)
        seq(40,48,{40:[(7,55)]},jumps={44:56})
        seq(56,84,{56:[(5,0)],60:[(4,BASE+60+4096)],64:[(4,BASE+4096)],68:[(6,1)],
            72:[(6,1<<63)],76:[(5,1<<63)]},{80:[mem(BASE+4096,8,write=1<<63)]})
        work=9; checksum=55
    elif name=='hello':
        emit(0,[(10,0x10000000)])
        uart=list(b'Hello!\n')
        for i,ch in enumerate(uart):
            call=4+i*12
            emit(call,[(1,BASE+call+4)],next_pc=92)
            # Existing UART contract retains transmitted bytes in its FIFO:
            # TEMT is set only before the first byte; THRE stays set below 16.
            lsr=0x60 if i==0 else 0x20
            seq(92,108,{92:[(3,lsr)],96:[(3,0x20)]},{92:[mem(0x10000005,1,read=lsr)]},jumps={104:call+4})
            emit(call+4,[(2,ch)])
            emit(call+8,memory=[mem(0x10000000,1,write=ch)])
            events.append(['uart',ch])
        emit(88,next_pc=108)
        seq(108,136,{108:[(4,BASE+108+4096)],112:[(4,BASE+4096)],116:[(5,0)],120:[(6,1)],
            124:[(6,1<<63)],128:[(5,1<<63)]},{132:[mem(BASE+4096,8,write=1<<63)]})
        work=7; checksum=sum(uart)
    elif name=='native_device':
        seq(0,24,{0:[(1,0x10000000)],4:[(2,65)],12:[(1,0x40008000)],16:[(2,7)]},
            {8:[mem(0x10000000,1,write=65)],20:[mem(0x40008000,8,write=7)]})
        uart=[65]; events=[['uart',65],['htif',7]]; checksum=65
    elif name in ['control_loop','ram_loop','mixed_w','mixed_d']:
        scratch=BASE+0x3008
        signature_addr=BASE+0x2000
        if name=='control_loop':
            work=32; checksum=528
            seq(0,12,{0:[(5,32)],4:[(6,0)],8:[(7,0)]})
            for i in range(1,33):
                seq(12,28,{12:[(6,i)],16:[(7,i*(i+1)//2)],20:[(5,32-i)]},jumps={24:12 if i<32 else 28})
            seq(28,60,{28:[(28,528)],44:[(8,BASE+44+8192)],48:[(8,signature_addr)]},
                {52:[mem(signature_addr,8,write=32)],56:[mem(signature_addr+8,8,write=528)]})
            reader=60; tail=64; end=84
        elif name=='ram_loop':
            work=16; checksum=136
            seq(0,36,{0:[(8,BASE+0x3000)],4:[(8,scratch)],8:[(9,BASE+8+8192)],12:[(9,signature_addr)],
                16:[(5,16)],20:[(6,0)],24:[(7,0)],28:[(28,0)]},{28:[mem(scratch,8,read=0)]})
            for i in range(1,17):
                seq(36,64,{36:[(6,i)],44:[(28,i)],52:[(7,i*(i+1)//2)],56:[(5,16-i)]},
                    {40:[mem(scratch,4,write=i)],44:[mem(scratch,4,read=i)]},jumps={60:36 if i<16 else 64})
            seq(64,84,{64:[(29,136)]},{76:[mem(signature_addr,8,write=16)],80:[mem(signature_addr+8,8,write=136)]})
            reader=84; tail=88; end=108
            ram.append(dict(offset=0x3008,bytes=raw(16,8)+[0]*8))
        else:
            work=8; checksum=24
            width=4 if name=='mixed_w' else 8
            seq(0,40,{0:[(8,BASE+0x3000)],4:[(8,scratch)],8:[(9,BASE+8+8192)],12:[(9,signature_addr)],
                16:[(18,8)],20:[(19,0)],24:[(20,0)],28:[(6,-2)],32:[(7,5)],36:[(28,3)]})
            for i in range(1,9):
                seq(40,120,{44:[(11,-2)],52:[(12,3)],60:[(13,0)],68:[(14,-2)],80:[(15,1)],84:[(29,1)],
                    96:[(16,3)],104:[(19,i*3)],108:[(20,i)],112:[(18,8-i)]},
                    {40:[mem(scratch,width,write=-2)],44:[mem(scratch,width,read=-2,write=3,atomic='rmw')],
                     52:[mem(scratch,width,read=3,atomic='lr')],60:[mem(scratch,width,write=-2,atomic='sc',conditional='success')],
                     68:[mem(scratch,width,read=-2,atomic='lr')],76:[mem(scratch,width,write=3)],
                     80:[mem(scratch,width,atomic='sc',conditional='failure')],92:[mem(scratch,width,atomic='sc',conditional='failure')],
                     96:[mem(scratch,width,read=3)]},
                    {52:[False,True],60:[True,False],68:[False,True],80:[True,False]},
                    {116:40 if i<8 else 120})
            seq(120,140,{120:[(29,24)]},{132:[mem(signature_addr,8,write=8)],136:[mem(signature_addr+8,8,write=24)]})
            reader=140; tail=144; end=164
            ram.append(dict(offset=0x3008,bytes=raw(3,8)+[0]*8))
        # Ten independent self-check instructions make every facade sample
        # check all neighbor/scratch bytes, not merely rely on a companion.
        last=0 if name=='control_loop' else 16 if name=='ram_loop' else 3
        seq(reader,reader+40,{reader:[(31,BASE+reader+0x3000)],reader+4:[(31,BASE+0x3000)],
            reader+8:[(5,0x123)],reader+12:[(29,0x123)],reader+20:[(5,last)],
            reader+24:[(29,last)],reader+32:[(5,0)]},
            {reader+8:[mem(BASE+0x3000,8,read=0x123)],reader+20:[mem(scratch,8,read=last)],
             reader+32:[mem(scratch+8,8,read=0)]})
        reader+=40; tail+=40; end+=40
        read_count=len(trace)
        emit(reader,[(30,read_count)])
        seq(tail,end,{tail+4:[(10,7)],tail+8:[(31,BASE+tail+8+4096)],tail+12:[(31,BASE+4096)]},
            {tail:[mem(signature_addr+16,8,write=read_count)],tail+16:[mem(BASE+4096,8,write=7)]})
        signature=raw(work,8)+raw(checksum,8)+raw(read_count,8)
        ram.append(dict(offset=0x3000,bytes=raw(0x123,8)))
        if name=='control_loop': ram.append(dict(offset=0x3008,bytes=[0]*16))
    elif name in ['amo_w','amo_d']:
        w=4 if name=='amo_w' else 8
        target=syms['atomic_word']
        if w==4:
            values={0:[(5,BASE)],4:[(5,target)],8:[(6,-1)],12:[(7,0x7fffffff)],16:[(28,-0x80000000)],
                20:[(28,0x7fffffff)],28:[(6,2)],32:[(29,-1)],36:[(28,-1)],44:[(6,-5)],48:[(30,1)],
                52:[(28,1)],60:[(6,6)],64:[(31,-5)],68:[(28,-5)],76:[(29,6)],80:[(28,6)],88:[(10,0)]}
            memories={12:[mem(target,w,read=0x7fffffff,write=-1,atomic='rmw')],32:[mem(target,w,read=-1,write=1,atomic='rmw')],
                48:[mem(target,w,read=1,write=-5,atomic='rmw')],64:[mem(target,w,read=-5,write=6,atomic='rmw')],76:[mem(target,w,read=6)]}
            seq(0,96,values,memories,jumps={92:100}); exit_tail(100); result=6
        else:
            values={0:[(5,BASE)],4:[(5,target)],8:[(6,-9)],12:[(7,10)],16:[(28,10)],24:[(6,20)],28:[(29,-9)],
                32:[(28,-9)],40:[(6,-3)],44:[(30,11)],48:[(28,11)],56:[(6,4)],60:[(31,-3)],64:[(28,-3)],
                72:[(29,4)],76:[(28,4)],84:[(10,0)]}
            memories={12:[mem(target,w,read=10,write=-9,atomic='rmw')],28:[mem(target,w,read=-9,write=11,atomic='rmw')],
                44:[mem(target,w,read=11,write=-3,atomic='rmw')],60:[mem(target,w,read=-3,write=4,atomic='rmw')],72:[mem(target,w,read=4)]}
            seq(0,92,values,memories,jumps={88:96}); exit_tail(96); result=4
        ram=[dict(offset=target-BASE,bytes=raw(result,w))]; work=4; checksum=result
    elif name in ['lrsc_loop','sc_after_store_failure']:
        target=syms['atomic_word']
        if name=='lrsc_loop':
            seq(0,44,{0:[(5,BASE)],4:[(5,target)],8:[(6,1)],12:[(7,0)],16:[(7,1)],20:[(28,0)],28:[(29,1)],36:[(10,0)]},
                {12:[mem(target,8,read=0,atomic='lr',aq=True)],20:[mem(target,8,write=1,atomic='sc',conditional='success',rl=True)],
                 28:[mem(target,8,read=1)]},{12:[False,True],20:[True,False]},{40:48})
            exit_tail(48); result=1
        else:
            seq(0,60,{0:[(5,BASE)],4:[(5,target)],8:[(6,1)],16:[(7,1)],20:[(6,2)],28:[(6,3)],32:[(28,1)],
                40:[(29,2)],44:[(30,2)],52:[(10,0)]},
                {12:[mem(target,8,write=1)],16:[mem(target,8,read=1,atomic='lr')],24:[mem(target,8,write=2)],
                 32:[mem(target,8,atomic='sc',conditional='failure')],40:[mem(target,8,read=2)]},
                {16:[False,True],32:[True,False]},{56:64})
            exit_tail(64); result=2
        ram=[dict(offset=target-BASE,bytes=raw(result,8))]; checksum=result
    elif name=='sw':
        target=syms['test_data']
        # Explicit linked immediate expansions and instruction-indexed values;
        # there is no generic instruction evaluator here.
        values={0:[(10,BASE)],4:[(10,target)],8:[(1,0x12345000)],12:[(1,0x12345678)],20:[(2,0x12345678)],
          24:[(7,0x12345000)],28:[(7,0x12345678)],36:[(1,1)],40:[(1,0x80000000)],48:[(2,-0x80000000)],
          52:[(7,1)],56:[(7,0x80000000)],60:[(8,-0x80000000)],68:[(1,-1)],76:[(2,-1)],80:[(7,-1)],
          88:[(1,0)],96:[(2,0)],100:[(7,0)],108:[(3,-0x215000)],112:[(3,-0x215241)],
          116:[(3,(-0x215241)<<14)],120:[(3,((-0x215241)<<14)-1037)],124:[(3,(((-0x215241)<<14)-1037)<<12)],
          128:[(3,((((-0x215241)<<14)-1037)<<12)-1029)],132:[(3,(((((-0x215241)<<14)-1037)<<12)-1029)<<14)],
          136:[(3,0xdeadbeefcafebabe)],144:[(4,-0x35014542)],148:[(7,0x33000)],152:[(7,0x32bfb)],
          156:[(7,0x32bfb<<14)],160:[(7,0xcafebabe)],164:[(7,-0x35014542)],
          172:[(20,0xab000)],176:[(20,0xaabbd)],180:[(20,0xaabbd<<12)],184:[(20,0xaabbccdd)],192:[(5,-0x55443323)],
          196:[(7,0xab000)],200:[(7,0xaabbd)],204:[(7,0xaabbd<<12)],208:[(7,0xaabbccdd)],212:[(7,-0x55443323)],
          220:[(21,0x11223000)],224:[(21,0x11223344)],232:[(5,0x11223344)],236:[(7,0x11223000)],240:[(7,0x11223344)],
          248:[(22,0x55667000)],252:[(22,0x55667788)],260:[(5,0x55667788)],264:[(7,0x55667000)],268:[(7,0x55667788)],
          276:[(1,0x11111000)],280:[(1,0x11111111)],288:[(1,0x22222000)],292:[(1,0x22222222)],
          300:[(1,0x33333000)],304:[(1,0x33333333)],312:[(2,0x33333333)],316:[(7,0x33333000)],320:[(7,0x33333333)],
          328:[(11,target+8)],332:[(1,0x9a000)],336:[(1,0x99999)],340:[(1,0x99999<<12)],344:[(1,0x99998888)],
          352:[(2,-0x66667778)],356:[(7,0x9a000)],360:[(7,0x99999)],364:[(7,0x99999<<12)],368:[(7,0x99998888)],
          372:[(7,-0x66667778)],380:[(5,0)]}
        stores=[(16,0,0x12345678),(44,4,0x80000000),(72,8,-1),(92,12,0),(140,0,0xcafebabe),
          (188,0,0xaabbccdd),(228,4,0x11223344),(256,8,0x55667788),(284,0,0x11111111),
          (296,0,0x22222222),(308,0,0x33333333),(348,4,0x99998888)]
        memories={pc:[mem(target+offset,4,write=value)] for pc,offset,value in stores}
        for pc,off,val in [(20,0,0x12345678),(48,4,0x80000000),(76,8,-1),(96,12,0),(144,0,0xcafebabe),
           (192,0,0xaabbccdd),(232,4,0x11223344),(260,8,0x55667788),(312,0,0x33333333),(352,4,0x99998888)]:
            memories[pc]=[mem(target+off,4,read=val)]
        seq(0,388,values,memories,jumps={384:392})
        seq(392,416,{392:[(4,BASE+392+4096)],396:[(4,BASE+4096)],400:[(6,1)],404:[(6,1<<63)],408:[(5,1<<63)]},
            {412:[mem(BASE+4096,8,write=1<<63)]})
        ram=[dict(offset=target-BASE,bytes=sum([raw(v,4) for v in [0x33333333,0x99998888,0x55667788,0]],[]))]
        work=len(stores); checksum=0x33333333+0x99998888+0x55667788
    else: raise ValueError(name)
    exit_code=3 if name in ['control_loop','ram_loop','mixed_w','mixed_d','native_device'] else 0
    f.update(entry=BASE,final_pc=trace[-1]['next_pc'],exit_code=exit_code,process_code=exit_code,
        attempts=len(trace),turns=len(trace),retirements=len(trace),traps=0,work=work,checksum=checksum,
        signature_addr=signature_addr,signature=signature,uart=uart,events=events,ram=ram,regs=regs,trace=trace)
    # ELF metadata identity independently parsed from fixed ELF64 fields.
    elf=(BUILD/(name+'.elf')).read_bytes()
    phoff=struct.unpack_from('<Q',elf,32)[0]; phnum=struct.unpack_from('<H',elf,56)[0]
    segments=[]
    for i in range(phnum):
        typ,flags,off,va,pa,filesz,memsz,align=struct.unpack_from('<IIQQQQQQ',elf,phoff+i*56)
        if typ==1: segments.append(dict(address=va,physical_address=pa,flags=flags,file_size=filesz,zero_fill=memsz-filesz))
    f['segments']=segments
    f['tohost']=syms.get('tohost')
    span=max(s['address']+s['file_size']+s['zero_fill'] for s in segments)-BASE
    f['memory_size']=max(0x10000,1<<(span-1).bit_length())
    shoff=struct.unpack_from('<Q',elf,40)[0]; shnum=struct.unpack_from('<H',elf,60)[0]
    shstr=struct.unpack_from('<H',elf,62)[0]
    names_off,names_size=struct.unpack_from('<QQ',elf,shoff+shstr*64+24)
    names=elf[names_off:names_off+names_size]
    f['signature_file_offset']=None
    for i in range(shnum):
        hdr=shoff+i*64
        noff=struct.unpack_from('<I',elf,hdr)[0]
        if names[noff:].split(b'\0')[0]==b'.signature':
            f['signature_file_offset']=struct.unpack_from('<Q',elf,hdr+24)[0]

# An explicit portable audit preserves the baseline tool metadata instead of
# falsely certifying a different producer. build.json retains actual identities.
if mode == ['--check-artifacts']:
    manifest.update(original_tools)
# Keep one complete trace row per line for bounded, reviewable diffs.
header={k:v for k,v in manifest.items() if k!='fixtures'}
lines=[json.dumps(header,indent=2)[:-2]+',\n  "fixtures": [']
for index,f in enumerate(manifest['fixtures']):
    fields={k:v for k,v in f.items() if k!='trace'}
    rendered=json.dumps(fields,indent=2).splitlines()
    lines.extend('    '+line for line in rendered[:-1])
    lines[-1]+=','
    lines.append('      "trace": [')
    for i,t in enumerate(f['trace']):
        lines.append('        '+json.dumps(t)+(',' if i+1<len(f['trace']) else ''))
    lines.append('      ]')
    lines.append('    }'+(',' if index+1<len(manifest['fixtures']) else ''))
lines.extend(['  ]','}'])
rendered='\n'.join(lines)+'\n'
if mode:
    if rendered != MANIFEST.read_text():
        raise ValueError('checked-in oracle differs from independent derivation')
    if mode == ['--check-artifacts']:
        print('P0 artifact-equivalence/oracle audit: matches; NOT pinned-toolchain evidence (no simulator invoked)')
    else:
        print('P0 linked-source/oracle derivation audit: matches (no simulator invoked)')
else:
    MANIFEST.write_text(rendered)
