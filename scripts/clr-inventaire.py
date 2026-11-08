#!/usr/bin/env python3
"""Inventaire ECMA-335 d'une assembly .NET : AssemblyRef, membres externes, génériques,
opcodes utilisés et P/Invoke. Usage : python3 scripts/clr-inventaire.py Programme.exe
Sert à mesurer, sur un programme témoin, ce que le CLR intégré doit fournir (#926)."""
import struct,sys,collections
def u16(b,o):return struct.unpack_from('<H',b,o)[0]
def u32(b,o):return struct.unpack_from('<I',b,o)[0]
d=open(sys.argv[1],'rb').read()
pe=u32(d,0x3c);nsec=u16(d,pe+6);opt=pe+24;magic=u16(d,opt)
dd=opt+(96 if magic==0x10b else 112)
secs=[];so=opt+u16(d,pe+20)
for i in range(nsec):
    o=so+40*i;secs.append((u32(d,o+12),u32(d,o+8),u32(d,o+20)))
def rva(r):
    for va,vs,raw in secs:
        if va<=r<va+max(vs,1):return raw+r-va
    raise Exception(hex(r))
cli=rva(u32(d,dd+14*8));md=rva(u32(d,cli+8));flags=u32(d,cli+16)
vl=u32(d,md+12);o=md+16+vl+2;ns=u16(d,o);o+=2;st={}
for i in range(ns):
    off,sz=u32(d,o),u32(d,o+4);o+=8;e=d.index(b'\0',o);name=d[o:e].decode();o=(e+4)&~3;st[name]=(md+off,sz)
T=st['#~'][0];hs=d[T+6];valid=struct.unpack_from('<Q',d,T+8)[0];o=T+24;rows={}
for t in range(64):
    if valid>>t&1:rows[t]=u32(d,o);o+=4
SI=4 if hs&1 else 2;GI=4 if hs&2 else 2;BI=4 if hs&4 else 2
def R(t):return rows.get(t,0)
def idx(t):return 4 if R(t)>0xffff else 2
def cidx(ts,bits):return 4 if max(R(t) for t in ts if t is not None)>= (1<<(16-bits)) else 2
TDOR=[2,1,0x1b];HC=[4,8,0x17];HCA=[6,4,1,2,8,9,10,0,0xe,0x17,0x14,0x11,0x1a,0x1b,0x20,0x23,0x26,0x27,0x28,0x2a,0x2c,0x2b];
HFM=[4,0xa];HDS=[2,6,0x20];MRP=[2,1,0x1a,6,0x1b];HS=[0x14,0x17];MD=[6,0xa];MF=[4,6];IMPL=[0x26,0x23,0x27];CAT=[None,None,6,0xa,None];RS=[0,0x1a,0x23,1];TOMD=[2,6]
S='s';G='g';B='b'
def ci(ts,bits):return ('c',ts,bits)
sch={0:[2,S,G,G,G],1:[ci(RS,2),S,S],2:[4,S,S,ci(TDOR,2),('i',4),('i',6)],3:[('i',4)],4:[2,S,B],5:[('i',6)],6:[4,2,2,S,B,('i',8)],7:[('i',8)],8:[2,2,S],9:[('i',2),ci(TDOR,2)],
0xa:[ci(MRP,3),S,B],0xb:[1,1,ci(HC,2),B],0xc:[ci(HCA,5),ci(CAT,3),B],0xd:[ci(HFM,1),B],0xe:[2,ci(HDS,2),B],0xf:[2,4,('i',2)],0x10:[4,('i',4)],0x11:[B],0x12:[('i',2),('i',0x14)],0x13:[('i',0x14)],0x14:[2,S,ci(TDOR,2)],
0x15:[('i',2),('i',0x17)],0x16:[('i',0x17)],0x17:[2,S,B],0x18:[2,('i',6),ci(HS,1)],0x19:[('i',2),ci(MD,1),ci(MD,1)],0x1a:[S],0x1b:[B],0x1c:[2,ci(MF,1),S,('i',0x1a)],0x1d:[4,('i',4)],0x1e:[4,4],0x1f:[4],
0x20:[4,2,2,2,2,4,B,S,S],0x21:[4],0x22:[4,4,4],0x23:[2,2,2,2,4,B,S,S,B],0x24:[4,('i',0x23)],0x25:[4,4,4,('i',0x23)],0x26:[4,S,B],0x27:[4,4,S,S,ci(IMPL,2)],0x28:[4,4,S,ci(IMPL,2)],0x29:[('i',2),('i',2)],0x2a:[2,2,ci(TOMD,1),S],0x2b:[ci(MD,1),B],0x2c:[('i',0x2a),ci(TDOR,2)]}
def sz(c):
    if c==S:return SI
    if c==G:return GI
    if c==B:return BI
    if isinstance(c,int):return c
    if c[0]=='i':return idx(c[1])
    return cidx(c[1],c[2])
tab={}
for t in sorted(rows):
    cols=sch[t];w=[sz(c) for c in cols];rs=[]
    for r in range(rows[t]):
        vals=[]
        for c in w:
            vals.append(u32(d,o) if c==4 else (u16(d,o) if c==2 else d[o]));o+=c
        rs.append(vals)
    tab[t]=rs
def sstr(i):
    a=st['#Strings'][0]+i;return d[a:d.index(b'\0',a)].decode('utf-8','replace')
def blob(i):
    a=st['#Blob'][0]+i;b=d[a]
    if b&0x80==0:n=b;a+=1
    elif b&0xc0==0x80:n=((b&0x3f)<<8)|d[a+1];a+=2
    else:n=((b&0x1f)<<24)|(d[a+1]<<16)|(d[a+2]<<8)|d[a+3];a+=4
    return d[a:a+n]
def tdor(v):
    t=[2,1,0x1b][v&3];r=v>>2;return t,r
def tname(t,r):
    if t==1:
        row=tab[1][r-1];rs=row[0];kind=[0,0x1a,0x23,1][rs&3];rr=rs>>2
        pre='';
        if kind==1: pre=tname(1,rr)+'/'
        ns=sstr(row[2]);n=sstr(row[1]);
        asm=sstr(tab[0x23][rr-1][6]) if kind==0x23 else '?'
        return pre+(ns+'.' if ns else '')+n+('' if pre else ' ['+asm+']')
    if t==2:
        row=tab[2][r-1];ns=sstr(row[2]);return (ns+'.' if ns else '')+sstr(row[1])
    return 'TypeSpec#%d'%r
out=sys.stdout
print('== AssemblyRef');[print(' ',sstr(r[6]),'%d.%d.%d.%d'%tuple(r[:4])) for r in tab.get(0x23,[])]
print('== Lignes', {hex(k):v for k,v in rows.items()})
print('generiques: GenericParam=%d TypeSpec=%d MethodSpec=%d'%(R(0x2a),R(0x1b),R(0x2b)))
# delegates: typedefs extending System.MulticastDelegate
dl=[sstr(r[1]) for r in tab.get(2,[]) if (lambda tr:tr[0]==1 and 'MulticastDelegate' in tname(*tr))(tdor(r[3]))]
print('delegues definis:',len(dl))
print('== MemberRef externes (%d)'%R(0xa))
mr=collections.Counter()
for r in tab.get(0xa,[]):
    c=r[0];t=[2,1,0x1a,6,0x1b][c&7];rr=c>>3
    if t==1: owner=tname(1,rr)
    elif t==0x1b: owner='TypeSpec#%d'%rr
    else: continue
    print('  ',owner,'::',sstr(r[1]))
# --- opcodes
OP={}
def defop(code,name,arg):OP[code]=(name,arg)
one=[(0x00,'nop',0),(0x01,'break',0)]+[(0x02+i,'ldarg.%d'%i,0) for i in range(4)]+[(0x06+i,'ldloc.%d'%i,0) for i in range(4)]+[(0x0a+i,'stloc.%d'%i,0) for i in range(4)]+[
(0x0e,'ldarg.s',1),(0x0f,'ldarga.s',1),(0x10,'starg.s',1),(0x11,'ldloc.s',1),(0x12,'ldloca.s',1),(0x13,'stloc.s',1),(0x14,'ldnull',0)]+[(0x15+i,'ldc.i4.%d'%(i-1),0) for i in range(10)]+[
(0x1f,'ldc.i4.s',1),(0x20,'ldc.i4',4),(0x21,'ldc.i8',8),(0x22,'ldc.r4',4),(0x23,'ldc.r8',8),(0x25,'dup',0),(0x26,'pop',0),(0x27,'jmp',4),(0x28,'call',4),(0x29,'calli',4),(0x2a,'ret',0)]+[
(0x2b+i,n+'.s',1) for i,n in enumerate(['br','brfalse','brtrue','beq','bge','bgt','ble','blt','bne.un','bge.un','bgt.un','ble.un','blt.un'])]+[
(0x38+i,n,4) for i,n in enumerate(['br','brfalse','brtrue','beq','bge','bgt','ble','blt','bne.un','bge.un','bgt.un','ble.un','blt.un'])]+[(0x45,'switch',-1)]+[
(0x46+i,n,0) for i,n in enumerate(['ldind.i1','ldind.u1','ldind.i2','ldind.u2','ldind.i4','ldind.u4','ldind.i8','ldind.i','ldind.r4','ldind.r8','ldind.ref','stind.ref','stind.i1','stind.i2','stind.i4','stind.i8','stind.r4','stind.r8','add','sub','mul','div','div.un','rem','rem.un','and','or','xor','shl','shr','shr.un','neg','not','conv.i1','conv.i2','conv.i4','conv.i8','conv.r4','conv.r8','conv.u4','conv.u8'])]+[
(0x6f,'callvirt',4),(0x70,'cpobj',4),(0x71,'ldobj',4),(0x72,'ldstr',4),(0x73,'newobj',4),(0x74,'castclass',4),(0x75,'isinst',4),(0x76,'conv.r.un',0),(0x79,'unbox',4),(0x7a,'throw',0),(0x7b,'ldfld',4),(0x7c,'ldflda',4),(0x7d,'stfld',4),(0x7e,'ldsfld',4),(0x7f,'ldsflda',4),(0x80,'stsfld',4),(0x81,'stobj',4),
(0x82,'conv.ovf.i1.un',0),(0x83,'conv.ovf.i2.un',0),(0x84,'conv.ovf.i4.un',0),(0x85,'conv.ovf.i8.un',0),(0x86,'conv.ovf.u1.un',0),(0x87,'conv.ovf.u2.un',0),(0x88,'conv.ovf.u4.un',0),(0x89,'conv.ovf.u8.un',0),(0x8a,'conv.ovf.i.un',0),(0x8b,'conv.ovf.u.un',0),
(0x8c,'box',4),(0x8d,'newarr',4),(0x8e,'ldlen',0),(0x8f,'ldelema',4)]+[(0x90+i,n,0) for i,n in enumerate(['ldelem.i1','ldelem.u1','ldelem.i2','ldelem.u2','ldelem.i4','ldelem.u4','ldelem.i8','ldelem.i','ldelem.r4','ldelem.r8','ldelem.ref','stelem.i','stelem.i1','stelem.i2','stelem.i4','stelem.i8','stelem.r4','stelem.r8','stelem.ref'])]+[
(0xa3,'ldelem',4),(0xa4,'stelem',4),(0xa5,'unbox.any',4)]+[(0xb3+i,n,0) for i,n in enumerate(['conv.ovf.i1','conv.ovf.u1','conv.ovf.i2','conv.ovf.u2','conv.ovf.i4','conv.ovf.u4','conv.ovf.i8','conv.ovf.u8'])]+[
(0xc2,'refanyval',4),(0xc3,'ckfinite',0),(0xc6,'mkrefany',4),(0xd0,'ldtoken',4),(0xd1,'conv.u2',0),(0xd2,'conv.u1',0),(0xd3,'conv.i',0),(0xd4,'conv.ovf.i',0),(0xd5,'conv.ovf.u',0),(0xd6,'add.ovf',0),(0xd7,'add.ovf.un',0),(0xd8,'mul.ovf',0),(0xd9,'mul.ovf.un',0),(0xda,'sub.ovf',0),(0xdb,'sub.ovf.un',0),(0xdc,'endfinally',0),(0xdd,'leave',4),(0xde,'leave.s',1),(0xdf,'stind.i',0),(0xe0,'conv.u',0)]
for c,n,a in one:defop(c,n,a)
two=[(0,'arglist',0),(1,'ceq',0),(2,'cgt',0),(3,'cgt.un',0),(4,'clt',0),(5,'clt.un',0),(6,'ldftn',4),(7,'ldvirtftn',4),(9,'ldarg',2),(0xa,'ldarga',2),(0xb,'starg',2),(0xc,'ldloc',2),(0xd,'ldloca',2),(0xe,'stloc',2),(0xf,'localloc',0),(0x11,'endfilter',0),(0x12,'unaligned.',1),(0x13,'volatile.',0),(0x14,'tail.',0),(0x15,'initobj',4),(0x16,'constrained.',4),(0x17,'cpblk',0),(0x18,'initblk',0),(0x19,'no.',1),(0x1a,'rethrow',0),(0x1c,'sizeof',4),(0x1d,'refanytype',0),(0x1e,'readonly.',0)]
for c,n,a in two:defop(0xfe00|c,n,a)
ops=collections.Counter();bodies=0;eh=0
for r in tab.get(6,[]):
    if not r[0]:continue
    try:p=rva(r[0])
    except Exception:continue
    bodies+=1;h=d[p]
    if h&3==2:size=h>>2;p+=1;more=False
    else:size=u32(d,p+4);more=bool(u16(d,p)&8);hsz=(u16(d,p)>>12)*4;p+=hsz
    q=p;end=p+size
    while q<end:
        c=d[q];q+=1
        if c==0xfe:c=0xfe00|d[q];q+=1
        n,a=OP.get(c,('?%x'%c,0));ops[n]+=1
        if a==-1:k=u32(d,q);q+=4+4*k
        else:q+=a
    if more:eh+=1
print('== corps IL',bodies,'methodes avec clauses EH',eh)
print('== opcodes',' '.join('%s=%d'%kv for kv in sorted(ops.items(),key=lambda x:-x[1])))
print('== P/Invoke (ImplMap)',R(0x1c))
for r in tab.get(0x1c,[]):print('   ',sstr(tab[0x1a][r[3]-1][0]),sstr(r[2]))
