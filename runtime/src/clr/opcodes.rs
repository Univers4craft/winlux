//! Opcodes CIL (Common Intermediate Language) selon ECMA-335.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u16)]
#[allow(non_camel_case_types)]
pub enum OpCode {
    Nop = 0x00,
    Break = 0x01,
    Ldarg_0 = 0x02,
    Ldarg_1 = 0x03,
    Ldarg_2 = 0x04,
    Ldarg_3 = 0x05,
    Ldloc_0 = 0x06,
    Ldloc_1 = 0x07,
    Ldloc_2 = 0x08,
    Ldloc_3 = 0x09,
    Stloc_0 = 0x0A,
    Stloc_1 = 0x0B,
    Stloc_2 = 0x0C,
    Stloc_3 = 0x0D,
    Ldarg_S = 0x0E,
    Ldarga_S = 0x0F,
    Starg_S = 0x10,
    Ldloc_S = 0x11,
    Ldloca_S = 0x12,
    Stloc_S = 0x13,
    Ldnull = 0x14,
    Ldc_I4_M1 = 0x15,
    Ldc_I4_0 = 0x16,
    Ldc_I4_1 = 0x17,
    Ldc_I4_2 = 0x18,
    Ldc_I4_3 = 0x19,
    Ldc_I4_4 = 0x1A,
    Ldc_I4_5 = 0x1B,
    Ldc_I4_6 = 0x1C,
    Ldc_I4_7 = 0x1D,
    Ldc_I4_8 = 0x1E,
    Ldc_I4_S = 0x1F,
    Ldc_I4 = 0x20,
    Ldc_I8 = 0x21,
    Ldc_R4 = 0x22,
    Ldc_R8 = 0x23,
    Dup = 0x25,
    Pop = 0x26,
    Jmp = 0x27,
    Call = 0x28,
    Calli = 0x29,
    Ret = 0x2A,
    Br_S = 0x2B,
    Brfalse_S = 0x2C,
    Brtrue_S = 0x2D,
    Beq_S = 0x2E,
    Bge_S = 0x2F,
    Bgt_S = 0x30,
    Ble_S = 0x31,
    Blt_S = 0x32,
    Bne_Un_S = 0x33,
    Bge_Un_S = 0x34,
    Bgt_Un_S = 0x35,
    Ble_Un_S = 0x36,
    Blt_Un_S = 0x37,
    Br = 0x38,
    Brfalse = 0x39,
    Brtrue = 0x3A,
    Beq = 0x3B,
    Bge = 0x3C,
    Bgt = 0x3D,
    Ble = 0x3E,
    Blt = 0x3F,
    Bne_Un = 0x40,
    Bge_Un = 0x41,
    Bgt_Un = 0x42,
    Ble_Un = 0x43,
    Blt_Un = 0x44,
    Switch = 0x45,
    Ldind_I1 = 0x46,
    Ldind_U1 = 0x47,
    Ldind_I2 = 0x48,
    Ldind_U2 = 0x49,
    Ldind_I4 = 0x4A,
    Ldind_U4 = 0x4B,
    Ldind_I8 = 0x4C,
    Ldind_I = 0x4D,
    Ldind_R4 = 0x4E,
    Ldind_R8 = 0x4F,
    Ldind_Ref = 0x50,
    Stind_Ref = 0x51,
    Stind_I1 = 0x52,
    Stind_I2 = 0x53,
    Stind_I4 = 0x54,
    Stind_I8 = 0x55,
    Stind_R4 = 0x56,
    Stind_R8 = 0x57,
    Add = 0x58,
    Sub = 0x59,
    Mul = 0x5A,
    Div = 0x5B,
    Div_Un = 0x5C,
    Rem = 0x5D,
    Rem_Un = 0x5E,
    And = 0x5F,
    Or = 0x60,
    Xor = 0x61,
    Shl = 0x62,
    Shr = 0x63,
    Shr_Un = 0x64,
    Neg = 0x65,
    Not = 0x66,
    Conv_I1 = 0x67,
    Conv_I2 = 0x68,
    Conv_I4 = 0x69,
    Conv_I8 = 0x6A,
    Conv_R4 = 0x6B,
    Conv_R8 = 0x6C,
    Conv_U4 = 0x6D,
    Conv_U8 = 0x6E,
    Callvirt = 0x6F,
    Cpobj = 0x70,
    Ldobj = 0x71,
    Ldstr = 0x72,
    Newobj = 0x73,
    Castclass = 0x74,
    Isinst = 0x75,
    Conv_R_Un = 0x76,
    Unbox = 0x79,
    Throw = 0x7A,
    Ldfld = 0x7B,
    Ldflda = 0x7C,
    Stfld = 0x7D,
    Ldsfld = 0x7E,
    Ldsflda = 0x7F,
    Stsfld = 0x80,
    Stobj = 0x81,
    Conv_Ovf_I1_Un = 0x82,
    Conv_Ovf_I2_Un = 0x83,
    Conv_Ovf_I4_Un = 0x84,
    Conv_Ovf_I8_Un = 0x85,
    Conv_Ovf_U1_Un = 0x86,
    Conv_Ovf_U2_Un = 0x87,
    Conv_Ovf_U4_Un = 0x88,
    Conv_Ovf_U8_Un = 0x89,
    Conv_Ovf_I_Un = 0x8A,
    Conv_Ovf_U_Un = 0x8B,
    Box = 0x8C,
    Newarr = 0x8D,
    Ldlen = 0x8E,
    Ldelema = 0x8F,
    Ldelem_I1 = 0x90,
    Ldelem_U1 = 0x91,
    Ldelem_I2 = 0x92,
    Ldelem_U2 = 0x93,
    Ldelem_I4 = 0x94,
    Ldelem_U4 = 0x95,
    Ldelem_I8 = 0x96,
    Ldelem_I = 0x97,
    Ldelem_R4 = 0x98,
    Ldelem_R8 = 0x99,
    Ldelem_Ref = 0x9A,
    Stelem_I = 0x9B,
    Stelem_I1 = 0x9C,
    Stelem_I2 = 0x9D,
    Stelem_I4 = 0x9E,
    Stelem_I8 = 0x9F,
    Stelem_R4 = 0xA0,
    Stelem_R8 = 0xA1,
    Stelem_Ref = 0xA2,
    Ldelem = 0xA3,
    Stelem = 0xA4,
    Unbox_Any = 0xA5,
    Conv_Ovf_I1 = 0xB3,
    Conv_Ovf_U1 = 0xB4,
    Conv_Ovf_I2 = 0xB5,
    Conv_Ovf_U2 = 0xB6,
    Conv_Ovf_I4 = 0xB7,
    Conv_Ovf_U4 = 0xB8,
    Conv_Ovf_I8 = 0xB9,
    Conv_Ovf_U8 = 0xBA,
    Refanyval = 0xC2,
    Ckfinite = 0xC3,
    Mkrefany = 0xC6,
    Ldtoken = 0xD0,
    Conv_U2 = 0xD1,
    Conv_U1 = 0xD2,
    Conv_I = 0xD3,
    Conv_Ovf_I = 0xD4,
    Conv_Ovf_U = 0xD5,
    Add_Ovf = 0xD6,
    Add_Ovf_Un = 0xD7,
    Mul_Ovf = 0xD8,
    Mul_Ovf_Un = 0xD9,
    Sub_Ovf = 0xDA,
    Sub_Ovf_Un = 0xDB,
    Endfinally = 0xDC,
    Leave = 0xDD,
    Leave_S = 0xDE,
    Stind_I = 0xDF,
    Conv_U = 0xE0,
    // Préfixe 0xFE
    Arglist = 0xFE00,
    Ceq = 0xFE01,
    Cgt = 0xFE02,
    Cgt_Un = 0xFE03,
    Clt = 0xFE04,
    Clt_Un = 0xFE05,
    Ldftn = 0xFE06,
    Ldvirtftn = 0xFE07,
    Ldarg = 0xFE09,
    Ldarga = 0xFE0A,
    Starg = 0xFE0B,
    Ldloc = 0xFE0C,
    Ldloca = 0xFE0D,
    Stloc = 0xFE0E,
    Localloc = 0xFE0F,
    Endfilter = 0xFE11,
    Unaligned = 0xFE12,
    Volatile = 0xFE13,
    Tail = 0xFE14,
    Initobj = 0xFE15,
    Constrained = 0xFE16,
    Cpblk = 0xFE17,
    Initblk = 0xFE18,
    Rethrow = 0xFE1A,
    Sizeof = 0xFE1C,
    Refanytype = 0xFE1D,
    Readonly = 0xFE1E,
}

pub struct Instruction {
    pub offset: usize,
    pub op: OpCode,
    pub size: usize,
}

pub fn decode(il: &[u8], offset: usize) -> Option<Instruction> {
    let b0 = *il.get(offset)?;
    let (op, size) = if b0 == 0xFE {
        let b1 = *il.get(offset + 1)?;
        let code = 0xFE00u16 | b1 as u16;
        let op = match code {
            0xFE00 => OpCode::Arglist,
            0xFE01 => OpCode::Ceq,
            0xFE02 => OpCode::Cgt,
            0xFE03 => OpCode::Cgt_Un,
            0xFE04 => OpCode::Clt,
            0xFE05 => OpCode::Clt_Un,
            0xFE06 => OpCode::Ldftn,
            0xFE07 => OpCode::Ldvirtftn,
            0xFE09 => OpCode::Ldarg,
            0xFE0A => OpCode::Ldarga,
            0xFE0B => OpCode::Starg,
            0xFE0C => OpCode::Ldloc,
            0xFE0D => OpCode::Ldloca,
            0xFE0E => OpCode::Stloc,
            0xFE0F => OpCode::Localloc,
            0xFE11 => OpCode::Endfilter,
            0xFE12 => OpCode::Unaligned,
            0xFE13 => OpCode::Volatile,
            0xFE14 => OpCode::Tail,
            0xFE15 => OpCode::Initobj,
            0xFE16 => OpCode::Constrained,
            0xFE17 => OpCode::Cpblk,
            0xFE18 => OpCode::Initblk,
            0xFE1A => OpCode::Rethrow,
            0xFE1C => OpCode::Sizeof,
            0xFE1D => OpCode::Refanytype,
            0xFE1E => OpCode::Readonly,
            _ => return None,
        };
        let operand = match op {
            OpCode::Ldarg
            | OpCode::Ldarga
            | OpCode::Starg
            | OpCode::Ldloc
            | OpCode::Ldloca
            | OpCode::Stloc => 2,
            OpCode::Sizeof | OpCode::Constrained | OpCode::Ldftn | OpCode::Ldvirtftn => 4,
            OpCode::Unaligned => 1,
            _ => 0,
        };
        (op, 2 + operand)
    } else {
        let op = match b0 {
            0x00 => OpCode::Nop,
            0x01 => OpCode::Break,
            0x02 => OpCode::Ldarg_0,
            0x03 => OpCode::Ldarg_1,
            0x04 => OpCode::Ldarg_2,
            0x05 => OpCode::Ldarg_3,
            0x06 => OpCode::Ldloc_0,
            0x07 => OpCode::Ldloc_1,
            0x08 => OpCode::Ldloc_2,
            0x09 => OpCode::Ldloc_3,
            0x0A => OpCode::Stloc_0,
            0x0B => OpCode::Stloc_1,
            0x0C => OpCode::Stloc_2,
            0x0D => OpCode::Stloc_3,
            0x0E => OpCode::Ldarg_S,
            0x0F => OpCode::Ldarga_S,
            0x10 => OpCode::Starg_S,
            0x11 => OpCode::Ldloc_S,
            0x12 => OpCode::Ldloca_S,
            0x13 => OpCode::Stloc_S,
            0x14 => OpCode::Ldnull,
            0x15 => OpCode::Ldc_I4_M1,
            0x16 => OpCode::Ldc_I4_0,
            0x17 => OpCode::Ldc_I4_1,
            0x18 => OpCode::Ldc_I4_2,
            0x19 => OpCode::Ldc_I4_3,
            0x1A => OpCode::Ldc_I4_4,
            0x1B => OpCode::Ldc_I4_5,
            0x1C => OpCode::Ldc_I4_6,
            0x1D => OpCode::Ldc_I4_7,
            0x1E => OpCode::Ldc_I4_8,
            0x1F => OpCode::Ldc_I4_S,
            0x20 => OpCode::Ldc_I4,
            0x21 => OpCode::Ldc_I8,
            0x22 => OpCode::Ldc_R4,
            0x23 => OpCode::Ldc_R8,
            0x25 => OpCode::Dup,
            0x26 => OpCode::Pop,
            0x27 => OpCode::Jmp,
            0x28 => OpCode::Call,
            0x29 => OpCode::Calli,
            0x2A => OpCode::Ret,
            0x2B => OpCode::Br_S,
            0x2C => OpCode::Brfalse_S,
            0x2D => OpCode::Brtrue_S,
            0x2E => OpCode::Beq_S,
            0x2F => OpCode::Bge_S,
            0x30 => OpCode::Bgt_S,
            0x31 => OpCode::Ble_S,
            0x32 => OpCode::Blt_S,
            0x33 => OpCode::Bne_Un_S,
            0x34 => OpCode::Bge_Un_S,
            0x35 => OpCode::Bgt_Un_S,
            0x36 => OpCode::Ble_Un_S,
            0x37 => OpCode::Blt_Un_S,
            0x38 => OpCode::Br,
            0x39 => OpCode::Brfalse,
            0x3A => OpCode::Brtrue,
            0x3B => OpCode::Beq,
            0x3C => OpCode::Bge,
            0x3D => OpCode::Bgt,
            0x3E => OpCode::Ble,
            0x3F => OpCode::Blt,
            0x40 => OpCode::Bne_Un,
            0x41 => OpCode::Bge_Un,
            0x42 => OpCode::Bgt_Un,
            0x43 => OpCode::Ble_Un,
            0x44 => OpCode::Blt_Un,
            0x45 => OpCode::Switch,
            0x46 => OpCode::Ldind_I1,
            0x47 => OpCode::Ldind_U1,
            0x48 => OpCode::Ldind_I2,
            0x49 => OpCode::Ldind_U2,
            0x4A => OpCode::Ldind_I4,
            0x4B => OpCode::Ldind_U4,
            0x4C => OpCode::Ldind_I8,
            0x4D => OpCode::Ldind_I,
            0x4E => OpCode::Ldind_R4,
            0x4F => OpCode::Ldind_R8,
            0x50 => OpCode::Ldind_Ref,
            0x51 => OpCode::Stind_Ref,
            0x52 => OpCode::Stind_I1,
            0x53 => OpCode::Stind_I2,
            0x54 => OpCode::Stind_I4,
            0x55 => OpCode::Stind_I8,
            0x56 => OpCode::Stind_R4,
            0x57 => OpCode::Stind_R8,
            0x58 => OpCode::Add,
            0x59 => OpCode::Sub,
            0x5A => OpCode::Mul,
            0x5B => OpCode::Div,
            0x5C => OpCode::Div_Un,
            0x5D => OpCode::Rem,
            0x5E => OpCode::Rem_Un,
            0x5F => OpCode::And,
            0x60 => OpCode::Or,
            0x61 => OpCode::Xor,
            0x62 => OpCode::Shl,
            0x63 => OpCode::Shr,
            0x64 => OpCode::Shr_Un,
            0x65 => OpCode::Neg,
            0x66 => OpCode::Not,
            0x67 => OpCode::Conv_I1,
            0x68 => OpCode::Conv_I2,
            0x69 => OpCode::Conv_I4,
            0x6A => OpCode::Conv_I8,
            0x6B => OpCode::Conv_R4,
            0x6C => OpCode::Conv_R8,
            0x6D => OpCode::Conv_U4,
            0x6E => OpCode::Conv_U8,
            0x6F => OpCode::Callvirt,
            0x70 => OpCode::Cpobj,
            0x71 => OpCode::Ldobj,
            0x72 => OpCode::Ldstr,
            0x73 => OpCode::Newobj,
            0x74 => OpCode::Castclass,
            0x75 => OpCode::Isinst,
            0x76 => OpCode::Conv_R_Un,
            0x79 => OpCode::Unbox,
            0x7A => OpCode::Throw,
            0x7B => OpCode::Ldfld,
            0x7C => OpCode::Ldflda,
            0x7D => OpCode::Stfld,
            0x7E => OpCode::Ldsfld,
            0x7F => OpCode::Ldsflda,
            0x80 => OpCode::Stsfld,
            0x81 => OpCode::Stobj,
            0x82 => OpCode::Conv_Ovf_I1_Un,
            0x83 => OpCode::Conv_Ovf_I2_Un,
            0x84 => OpCode::Conv_Ovf_I4_Un,
            0x85 => OpCode::Conv_Ovf_I8_Un,
            0x86 => OpCode::Conv_Ovf_U1_Un,
            0x87 => OpCode::Conv_Ovf_U2_Un,
            0x88 => OpCode::Conv_Ovf_U4_Un,
            0x89 => OpCode::Conv_Ovf_U8_Un,
            0x8A => OpCode::Conv_Ovf_I_Un,
            0x8B => OpCode::Conv_Ovf_U_Un,
            0x8C => OpCode::Box,
            0x8D => OpCode::Newarr,
            0x8E => OpCode::Ldlen,
            0x8F => OpCode::Ldelema,
            0x90 => OpCode::Ldelem_I1,
            0x91 => OpCode::Ldelem_U1,
            0x92 => OpCode::Ldelem_I2,
            0x93 => OpCode::Ldelem_U2,
            0x94 => OpCode::Ldelem_I4,
            0x95 => OpCode::Ldelem_U4,
            0x96 => OpCode::Ldelem_I8,
            0x97 => OpCode::Ldelem_I,
            0x98 => OpCode::Ldelem_R4,
            0x99 => OpCode::Ldelem_R8,
            0x9A => OpCode::Ldelem_Ref,
            0x9B => OpCode::Stelem_I,
            0x9C => OpCode::Stelem_I1,
            0x9D => OpCode::Stelem_I2,
            0x9E => OpCode::Stelem_I4,
            0x9F => OpCode::Stelem_I8,
            0xA0 => OpCode::Stelem_R4,
            0xA1 => OpCode::Stelem_R8,
            0xA2 => OpCode::Stelem_Ref,
            0xA3 => OpCode::Ldelem,
            0xA4 => OpCode::Stelem,
            0xA5 => OpCode::Unbox_Any,
            0xB3 => OpCode::Conv_Ovf_I1,
            0xB4 => OpCode::Conv_Ovf_U1,
            0xB5 => OpCode::Conv_Ovf_I2,
            0xB6 => OpCode::Conv_Ovf_U2,
            0xB7 => OpCode::Conv_Ovf_I4,
            0xB8 => OpCode::Conv_Ovf_U4,
            0xB9 => OpCode::Conv_Ovf_I8,
            0xBA => OpCode::Conv_Ovf_U8,
            0xC2 => OpCode::Refanyval,
            0xC3 => OpCode::Ckfinite,
            0xC6 => OpCode::Mkrefany,
            0xD0 => OpCode::Ldtoken,
            0xD1 => OpCode::Conv_U2,
            0xD2 => OpCode::Conv_U1,
            0xD3 => OpCode::Conv_I,
            0xD4 => OpCode::Conv_Ovf_I,
            0xD5 => OpCode::Conv_Ovf_U,
            0xD6 => OpCode::Add_Ovf,
            0xD7 => OpCode::Add_Ovf_Un,
            0xD8 => OpCode::Mul_Ovf,
            0xD9 => OpCode::Mul_Ovf_Un,
            0xDA => OpCode::Sub_Ovf,
            0xDB => OpCode::Sub_Ovf_Un,
            0xDC => OpCode::Endfinally,
            0xDD => OpCode::Leave,
            0xDE => OpCode::Leave_S,
            0xDF => OpCode::Stind_I,
            0xE0 => OpCode::Conv_U,
            _ => return None,
        };
        let operand = match op {
            OpCode::Ldarg_S
            | OpCode::Ldarga_S
            | OpCode::Starg_S
            | OpCode::Ldloc_S
            | OpCode::Ldloca_S
            | OpCode::Stloc_S
            | OpCode::Ldc_I4_S
            | OpCode::Br_S
            | OpCode::Brfalse_S
            | OpCode::Brtrue_S
            | OpCode::Beq_S
            | OpCode::Bge_S
            | OpCode::Bgt_S
            | OpCode::Ble_S
            | OpCode::Blt_S
            | OpCode::Bne_Un_S
            | OpCode::Bge_Un_S
            | OpCode::Bgt_Un_S
            | OpCode::Ble_Un_S
            | OpCode::Blt_Un_S
            | OpCode::Leave_S => 1,

            OpCode::Ldc_I4
            | OpCode::Ldc_R4
            | OpCode::Br
            | OpCode::Brfalse
            | OpCode::Brtrue
            | OpCode::Beq
            | OpCode::Bge
            | OpCode::Bgt
            | OpCode::Ble
            | OpCode::Blt
            | OpCode::Bne_Un
            | OpCode::Bge_Un
            | OpCode::Bgt_Un
            | OpCode::Ble_Un
            | OpCode::Blt_Un
            | OpCode::Leave
            | OpCode::Jmp
            | OpCode::Call
            | OpCode::Calli
            | OpCode::Callvirt
            | OpCode::Newobj
            | OpCode::Castclass
            | OpCode::Isinst
            | OpCode::Ldfld
            | OpCode::Ldflda
            | OpCode::Stfld
            | OpCode::Ldsfld
            | OpCode::Ldsflda
            | OpCode::Stsfld
            | OpCode::Box
            | OpCode::Unbox
            | OpCode::Unbox_Any
            | OpCode::Newarr
            | OpCode::Ldelema
            | OpCode::Ldelem
            | OpCode::Stelem
            | OpCode::Cpobj
            | OpCode::Ldobj
            | OpCode::Stobj
            | OpCode::Ldstr
            | OpCode::Ldtoken
            | OpCode::Initobj
            | OpCode::Mkrefany
            | OpCode::Refanyval => 4,

            OpCode::Ldc_I8 | OpCode::Ldc_R8 => 8,

            OpCode::Switch => {
                let n = il
                    .get(offset + 1..offset + 5)
                    .map(|s| u32::from_le_bytes(s.try_into().unwrap()) as usize)
                    .unwrap_or(0);
                4 + n * 4
            }

            _ => 0,
        };
        (op, 1 + operand)
    };
    Some(Instruction { offset, op, size })
}

pub fn read_i8(il: &[u8], offset: usize) -> i8 {
    il.get(offset).copied().unwrap_or(0) as i8
}

pub fn read_u8(il: &[u8], offset: usize) -> u8 {
    il.get(offset).copied().unwrap_or(0)
}

pub fn read_i32(il: &[u8], offset: usize) -> i32 {
    il.get(offset..offset + 4)
        .map(|s| i32::from_le_bytes(s.try_into().unwrap()))
        .unwrap_or(0)
}

pub fn read_u32(il: &[u8], offset: usize) -> u32 {
    il.get(offset..offset + 4)
        .map(|s| u32::from_le_bytes(s.try_into().unwrap()))
        .unwrap_or(0)
}

pub fn read_i64(il: &[u8], offset: usize) -> i64 {
    il.get(offset..offset + 8)
        .map(|s| i64::from_le_bytes(s.try_into().unwrap()))
        .unwrap_or(0)
}

pub fn read_f32(il: &[u8], offset: usize) -> f32 {
    il.get(offset..offset + 4)
        .map(|s| f32::from_le_bytes(s.try_into().unwrap()))
        .unwrap_or(0.0)
}

pub fn read_f64(il: &[u8], offset: usize) -> f64 {
    il.get(offset..offset + 8)
        .map(|s| f64::from_le_bytes(s.try_into().unwrap()))
        .unwrap_or(0.0)
}
