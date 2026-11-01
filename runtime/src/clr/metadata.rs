//! Parseur de métadonnées CLI (ECMA-335 §II.24).
//!
//! Lit les heaps (#Strings, #US, #Blob, #GUID) et les tables de métadonnées
//! depuis un assembly .NET chargé en mémoire.

// Décrit le format ECMA-335 complet ; l'interpréteur n'en lit encore qu'une partie.
#![allow(dead_code)]

use super::tables::*;

const BSJB_SIGNATURE: u32 = 0x424A5342;

#[derive(Debug)]
pub struct MetadataError(pub String);

impl std::fmt::Display for MetadataError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "metadata CLI : {}", self.0)
    }
}

fn rd16(b: &[u8], o: usize) -> u16 {
    b.get(o..o + 2)
        .map(|s| u16::from_le_bytes([s[0], s[1]]))
        .unwrap_or(0)
}
fn rd32(b: &[u8], o: usize) -> u32 {
    b.get(o..o + 4)
        .map(|s| u32::from_le_bytes(s.try_into().unwrap()))
        .unwrap_or(0)
}

#[derive(Clone)]
pub struct Assembly {
    data: Vec<u8>,
    sections: Vec<SectionInfo>,
    pub clr_header_rva: u32,

    strings_offset: usize,
    strings_size: usize,
    us_offset: usize,
    us_size: usize,
    blob_offset: usize,
    blob_size: usize,
    guid_offset: usize,
    guid_size: usize,

    tables_data_offset: usize,
    pub row_counts: [u32; 64],
    row_offsets: [usize; 64],
    row_sizes: [usize; 64],
    string_idx_size: usize,
    guid_idx_size: usize,
    blob_idx_size: usize,

    pub entry_point_token: u32,
    pub clr_flags: u32,
}

#[derive(Clone, Copy)]
struct SectionInfo {
    rva: u32,
    vsize: u32,
    raw_ptr: u32,
}

impl Assembly {
    pub fn parse(data: Vec<u8>) -> Result<Assembly, MetadataError> {
        let err = |msg: &str| MetadataError(msg.to_string());

        if data.get(..2) != Some(b"MZ") {
            return Err(err("pas un PE"));
        }
        let pe_off = rd32(&data, 0x3c) as usize;
        if data.get(pe_off..pe_off + 4) != Some(b"PE\0\0") {
            return Err(err("signature PE manquante"));
        }
        let coff = pe_off + 4;
        let num_sections = rd16(&data, coff + 2) as usize;
        let opt_size = rd16(&data, coff + 16) as usize;
        let opt = coff + 20;
        let magic = rd16(&data, opt);

        let dir_base = match magic {
            0x10b => opt + 96,
            0x20b => opt + 112,
            _ => return Err(err("magic PE inconnu")),
        };

        let sections_start = opt + opt_size;
        let mut sections = Vec::with_capacity(num_sections);
        for i in 0..num_sections {
            let s = sections_start + i * 40;
            sections.push(SectionInfo {
                rva: rd32(&data, s + 12),
                vsize: rd32(&data, s + 8),
                raw_ptr: rd32(&data, s + 20),
            });
        }

        let clr_entry = dir_base + 14 * 8;
        let clr_rva = rd32(&data, clr_entry);
        if clr_rva == 0 {
            return Err(err("pas d'en-tête CLR"));
        }
        let clr_off =
            rva_to_offset(&sections, clr_rva).ok_or_else(|| err("RVA CLR hors sections"))?;

        let clr_flags = rd32(&data, clr_off + 16);
        let meta_rva = rd32(&data, clr_off + 8);
        let entry_point_token = rd32(&data, clr_off + 20);

        let meta_off =
            rva_to_offset(&sections, meta_rva).ok_or_else(|| err("RVA metadata hors sections"))?;

        if rd32(&data, meta_off) != BSJB_SIGNATURE {
            return Err(err("signature BSJB manquante"));
        }

        let version_len = rd32(&data, meta_off + 12) as usize;
        let streams_offset = meta_off + 16 + ((version_len + 3) & !3);
        let _flags = rd16(&data, streams_offset);
        let num_streams = rd16(&data, streams_offset + 2) as usize;

        let mut strings_offset = 0usize;
        let mut strings_size = 0usize;
        let mut us_offset = 0;
        let mut us_size = 0;
        let mut blob_offset = 0;
        let mut blob_size = 0;
        let mut guid_offset = 0;
        let mut guid_size = 0;
        let mut tables_offset = 0;

        let mut cursor = streams_offset + 4;
        for _ in 0..num_streams {
            if cursor + 8 > data.len() {
                return Err(err("en-tête de flux tronqué"));
            }
            let off = rd32(&data, cursor) as usize;
            let size = rd32(&data, cursor + 4) as usize;
            let name_start = cursor + 8;
            let name_bytes = data.get(name_start..).unwrap_or(&[]);
            let name_end = name_bytes.iter().position(|&b| b == 0).unwrap_or(0) + name_start;
            let name = std::str::from_utf8(&data[name_start..name_end]).unwrap_or("");
            // Un flux doit être contenu dans le fichier : offset et taille
            // sont vérifiés ici pour que les tas se lisent sans débordement.
            let abs_off = meta_off + off;
            if abs_off > data.len() || size > data.len() - abs_off {
                return Err(err("flux de métadonnées hors fichier"));
            }
            match name {
                "#Strings" => {
                    strings_offset = abs_off;
                    strings_size = size;
                }
                "#US" => {
                    us_offset = abs_off;
                    us_size = size;
                }
                "#Blob" => {
                    blob_offset = abs_off;
                    blob_size = size;
                }
                "#GUID" => {
                    guid_offset = abs_off;
                    guid_size = size;
                }
                "#~" | "#-" => {
                    tables_offset = abs_off;
                }
                _ => {}
            }
            let name_len = name_end - name_start + 1;
            cursor = name_start + ((name_len + 3) & !3);
        }

        if tables_offset == 0 {
            return Err(err("stream #~ absent"));
        }

        let _reserved = rd32(&data, tables_offset);
        let _major = data.get(tables_offset + 4).copied().unwrap_or(0);
        let _minor = data.get(tables_offset + 5).copied().unwrap_or(0);
        let heap_sizes = data.get(tables_offset + 6).copied().unwrap_or(0);

        let string_idx_size = if heap_sizes & 0x01 != 0 { 4 } else { 2 };
        let guid_idx_size = if heap_sizes & 0x02 != 0 { 4 } else { 2 };
        let blob_idx_size = if heap_sizes & 0x04 != 0 { 4 } else { 2 };

        let valid_lo = rd32(&data, tables_offset + 8) as u64;
        let valid_hi = rd32(&data, tables_offset + 12) as u64;
        let valid = valid_lo | (valid_hi << 32);

        let mut row_counts = [0u32; 64];
        let mut rc_offset = tables_offset + 24;
        for i in 0..64u8 {
            if valid & (1u64 << i) != 0 {
                row_counts[i as usize] = rd32(&data, rc_offset);
                rc_offset += 4;
            }
        }

        let tables_data_offset = rc_offset;

        let mut asm = Assembly {
            data,
            sections,
            clr_header_rva: clr_rva,
            strings_offset,
            strings_size,
            us_offset,
            us_size,
            blob_offset,
            blob_size,
            guid_offset,
            guid_size,
            tables_data_offset,
            row_counts,
            row_offsets: [0; 64],
            row_sizes: [0; 64],
            string_idx_size,
            guid_idx_size,
            blob_idx_size,
            entry_point_token,
            clr_flags,
        };

        asm.compute_row_sizes();
        asm.compute_row_offsets();
        Ok(asm)
    }

    fn coded_index_size(&self, ci: CodedIndex) -> usize {
        let bits = ci.tag_bits();
        let max_rows = ci
            .tables()
            .iter()
            .filter_map(|t| t.map(|tid| self.row_counts[tid as usize]))
            .max()
            .unwrap_or(0);
        if max_rows < (1u32 << (16 - bits)) {
            2
        } else {
            4
        }
    }

    fn compute_row_sizes(&mut self) {
        let s = self.string_idx_size;
        let g = self.guid_idx_size;
        let b = self.blob_idx_size;

        let tid_size = |t: TableId| -> usize {
            if self.row_counts[t as usize] < 0x10000 {
                2
            } else {
                4
            }
        };

        let ci = |c: CodedIndex| -> usize { self.coded_index_size(c) };

        let sizes: [(u8, usize); 34] = [
            (0x00, 2 + s + g + g + g),                       // Module
            (0x01, ci(CodedIndex::ResolutionScope) + s + s), // TypeRef
            (
                0x02,
                4 + s
                    + s
                    + ci(CodedIndex::TypeDefOrRef)
                    + tid_size(TableId::Field)
                    + tid_size(TableId::MethodDef),
            ), // TypeDef
            (0x04, 2 + s + b),                               // Field
            (0x06, 4 + 2 + 2 + s + b + tid_size(TableId::Param)), // MethodDef
            (0x08, 2 + 2 + s),                               // Param
            (
                0x09,
                tid_size(TableId::TypeDef) + ci(CodedIndex::TypeDefOrRef),
            ), // InterfaceImpl
            (0x0A, ci(CodedIndex::MemberRefParent) + s + b), // MemberRef
            (0x0B, 2 + ci(CodedIndex::HasConstant) + b),     // Constant
            (
                0x0C,
                ci(CodedIndex::HasCustomAttribute) + ci(CodedIndex::CustomAttributeType) + b,
            ), // CustomAttribute
            (0x0D, ci(CodedIndex::HasFieldMarshal) + b),     // FieldMarshal
            (0x0E, 2 + ci(CodedIndex::HasDeclSecurity) + b), // DeclSecurity
            (0x0F, 2 + 4 + tid_size(TableId::TypeDef)),      // ClassLayout
            (0x10, 4 + tid_size(TableId::Field)),            // FieldLayout
            (0x11, b),                                       // StandAloneSig
            (0x12, tid_size(TableId::TypeDef) + tid_size(TableId::Event)), // EventMap
            (0x14, 2 + s + ci(CodedIndex::TypeDefOrRef)),    // Event
            (
                0x15,
                tid_size(TableId::TypeDef) + tid_size(TableId::Property),
            ), // PropertyMap
            (0x17, 2 + s + b),                               // Property
            (
                0x18,
                2 + tid_size(TableId::MethodDef) + ci(CodedIndex::HasSemantics),
            ), // MethodSemantics
            (
                0x19,
                tid_size(TableId::TypeDef)
                    + ci(CodedIndex::MethodDefOrRef)
                    + ci(CodedIndex::MethodDefOrRef),
            ), // MethodImpl
            (0x1A, s),                                       // ModuleRef
            (0x1B, b),                                       // TypeSpec
            (
                0x1C,
                2 + ci(CodedIndex::MemberForwarded) + s + tid_size(TableId::ModuleRef),
            ), // ImplMap
            (0x1D, 4 + tid_size(TableId::Field)),            // FieldRVA
            (0x20, 4 + 2 + 2 + 2 + 2 + 4 + b + s + s),       // Assembly
            (0x23, 2 + 2 + 2 + 2 + 4 + b + s + s + b),       // AssemblyRef
            (0x26, 4 + s + b),                               // File
            (0x27, 4 + 4 + s + s + ci(CodedIndex::Implementation)), // ExportedType
            (0x28, 4 + 4 + s + ci(CodedIndex::Implementation)), // ManifestResource
            (
                0x29,
                tid_size(TableId::TypeDef) + tid_size(TableId::TypeDef),
            ), // NestedClass
            (0x2A, 2 + 2 + ci(CodedIndex::TypeOrMethodDef) + s), // GenericParam
            (0x2B, ci(CodedIndex::MethodDefOrRef) + b),      // MethodSpec
            (
                0x2C,
                tid_size(TableId::GenericParam) + ci(CodedIndex::TypeDefOrRef),
            ), // GenericParamConstraint
        ];

        for (tid, size) in sizes {
            self.row_sizes[tid as usize] = size;
        }
    }

    fn compute_row_offsets(&mut self) {
        let mut offset = self.tables_data_offset;
        for i in 0..64u8 {
            if self.row_counts[i as usize] > 0 {
                self.row_offsets[i as usize] = offset;
                offset += self.row_sizes[i as usize] * self.row_counts[i as usize] as usize;
            }
        }
    }

    fn row_data(&self, table: TableId, row: u32) -> &[u8] {
        let tid = table as usize;
        if row == 0 || row > self.row_counts[tid] {
            return &[];
        }
        let off = self.row_offsets[tid] + (row as usize - 1) * self.row_sizes[tid];
        let end = off + self.row_sizes[tid];
        self.data.get(off..end).unwrap_or(&[])
    }

    fn read_idx(&self, data: &[u8], off: &mut usize, size: usize) -> u32 {
        let v = if size == 2 {
            rd16(data, *off) as u32
        } else {
            rd32(data, *off)
        };
        *off += size;
        v
    }

    fn read_u16(&self, data: &[u8], off: &mut usize) -> u16 {
        let v = rd16(data, *off);
        *off += 2;
        v
    }

    fn read_u32_val(&self, data: &[u8], off: &mut usize) -> u32 {
        let v = rd32(data, *off);
        *off += 4;
        v
    }

    fn str_idx(&self, data: &[u8], off: &mut usize) -> u32 {
        self.read_idx(data, off, self.string_idx_size)
    }

    fn guid_idx(&self, data: &[u8], off: &mut usize) -> u32 {
        self.read_idx(data, off, self.guid_idx_size)
    }

    fn blob_idx(&self, data: &[u8], off: &mut usize) -> u32 {
        self.read_idx(data, off, self.blob_idx_size)
    }

    fn table_idx(&self, table: TableId, data: &[u8], off: &mut usize) -> u32 {
        let size = if self.row_counts[table as usize] < 0x10000 {
            2
        } else {
            4
        };
        self.read_idx(data, off, size)
    }

    fn coded_idx(&self, ci: CodedIndex, data: &[u8], off: &mut usize) -> u32 {
        let size = self.coded_index_size(ci);
        self.read_idx(data, off, size)
    }

    // --- Heap access ---

    pub fn get_string(&self, idx: u32) -> &str {
        let start = self.strings_offset + idx as usize;
        if start >= self.strings_offset + self.strings_size {
            return "";
        }
        let Some(bytes) = self
            .data
            .get(start..self.strings_offset + self.strings_size)
        else {
            return "";
        };
        let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
        std::str::from_utf8(&bytes[..end]).unwrap_or("")
    }

    pub fn get_user_string(&self, idx: u32) -> String {
        let start = self.us_offset + idx as usize;
        if start >= self.us_offset + self.us_size {
            return String::new();
        }
        let (blob_len, hdr) = decode_blob_length(self.data.get(start..).unwrap_or(&[]));
        if blob_len == 0 {
            return String::new();
        }
        let str_start = start + hdr;
        let byte_count = blob_len.saturating_sub(1);
        let bytes = self
            .data
            .get(str_start..str_start + byte_count)
            .unwrap_or(&[]);
        let u16s: Vec<u16> = bytes
            .as_chunks::<2>()
            .0
            .iter()
            .map(|c| u16::from_le_bytes(*c))
            .collect();
        String::from_utf16_lossy(&u16s)
    }

    pub fn get_blob(&self, idx: u32) -> &[u8] {
        let start = self.blob_offset + idx as usize;
        if start >= self.blob_offset + self.blob_size {
            return &[];
        }
        let (len, hdr) = decode_blob_length(self.data.get(start..).unwrap_or(&[]));
        let data_start = start + hdr;
        self.data.get(data_start..data_start + len).unwrap_or(&[])
    }

    // --- Table row readers ---

    pub fn row_count(&self, table: TableId) -> u32 {
        self.row_counts[table as usize]
    }

    pub fn get_module(&self, row: u32) -> ModuleRow {
        let d = self.row_data(TableId::Module, row);
        let mut o = 0;
        let generation = self.read_u16(d, &mut o);
        let name = self.str_idx(d, &mut o);
        let mvid = self.guid_idx(d, &mut o);
        let _enc_id = self.guid_idx(d, &mut o);
        let _enc_base = self.guid_idx(d, &mut o);
        ModuleRow {
            generation,
            name,
            mvid,
        }
    }

    pub fn get_type_ref(&self, row: u32) -> TypeRefRow {
        let d = self.row_data(TableId::TypeRef, row);
        let mut o = 0;
        let resolution_scope = self.coded_idx(CodedIndex::ResolutionScope, d, &mut o);
        let name = self.str_idx(d, &mut o);
        let namespace = self.str_idx(d, &mut o);
        TypeRefRow {
            resolution_scope,
            name,
            namespace,
        }
    }

    pub fn get_type_def(&self, row: u32) -> TypeDefRow {
        let d = self.row_data(TableId::TypeDef, row);
        let mut o = 0;
        let flags = self.read_u32_val(d, &mut o);
        let name = self.str_idx(d, &mut o);
        let namespace = self.str_idx(d, &mut o);
        let extends = self.coded_idx(CodedIndex::TypeDefOrRef, d, &mut o);
        let field_list = self.table_idx(TableId::Field, d, &mut o);
        let method_list = self.table_idx(TableId::MethodDef, d, &mut o);
        TypeDefRow {
            flags,
            name,
            namespace,
            extends,
            field_list,
            method_list,
        }
    }

    pub fn get_field(&self, row: u32) -> FieldRow {
        let d = self.row_data(TableId::Field, row);
        let mut o = 0;
        let flags = self.read_u16(d, &mut o);
        let name = self.str_idx(d, &mut o);
        let signature = self.blob_idx(d, &mut o);
        FieldRow {
            flags,
            name,
            signature,
        }
    }

    pub fn get_method_def(&self, row: u32) -> MethodDefRow {
        let d = self.row_data(TableId::MethodDef, row);
        let mut o = 0;
        let rva = self.read_u32_val(d, &mut o);
        let impl_flags = self.read_u16(d, &mut o);
        let flags = self.read_u16(d, &mut o);
        let name = self.str_idx(d, &mut o);
        let signature = self.blob_idx(d, &mut o);
        let param_list = self.table_idx(TableId::Param, d, &mut o);
        MethodDefRow {
            rva,
            impl_flags,
            flags,
            name,
            signature,
            param_list,
        }
    }

    pub fn get_param(&self, row: u32) -> ParamRow {
        let d = self.row_data(TableId::Param, row);
        let mut o = 0;
        let flags = self.read_u16(d, &mut o);
        let sequence = self.read_u16(d, &mut o);
        let name = self.str_idx(d, &mut o);
        ParamRow {
            flags,
            sequence,
            name,
        }
    }

    pub fn get_member_ref(&self, row: u32) -> MemberRefRow {
        let d = self.row_data(TableId::MemberRef, row);
        let mut o = 0;
        let class = self.coded_idx(CodedIndex::MemberRefParent, d, &mut o);
        let name = self.str_idx(d, &mut o);
        let signature = self.blob_idx(d, &mut o);
        MemberRefRow {
            class,
            name,
            signature,
        }
    }

    pub fn get_type_spec(&self, row: u32) -> TypeSpecRow {
        let d = self.row_data(TableId::TypeSpec, row);
        let mut o = 0;
        let signature = self.blob_idx(d, &mut o);
        TypeSpecRow { signature }
    }

    pub fn get_standalone_sig(&self, row: u32) -> StandAloneSigRow {
        let d = self.row_data(TableId::StandAloneSig, row);
        let mut o = 0;
        let signature = self.blob_idx(d, &mut o);
        StandAloneSigRow { signature }
    }

    pub fn get_impl_map(&self, row: u32) -> ImplMapRow {
        let d = self.row_data(TableId::ImplMap, row);
        let mut o = 0;
        let mapping_flags = self.read_u16(d, &mut o);
        let member_forwarded = self.coded_idx(CodedIndex::MemberForwarded, d, &mut o);
        let import_name = self.str_idx(d, &mut o);
        let import_scope = self.table_idx(TableId::ModuleRef, d, &mut o);
        ImplMapRow {
            mapping_flags,
            member_forwarded,
            import_name,
            import_scope,
        }
    }

    pub fn get_module_ref(&self, row: u32) -> ModuleRefRow {
        let d = self.row_data(TableId::ModuleRef, row);
        let mut o = 0;
        let name = self.str_idx(d, &mut o);
        ModuleRefRow { name }
    }

    pub fn get_assembly(&self, row: u32) -> AssemblyRow {
        let d = self.row_data(TableId::Assembly, row);
        let mut o = 0;
        let hash_alg = self.read_u32_val(d, &mut o);
        let major = self.read_u16(d, &mut o);
        let minor = self.read_u16(d, &mut o);
        let build = self.read_u16(d, &mut o);
        let revision = self.read_u16(d, &mut o);
        let flags = self.read_u32_val(d, &mut o);
        let public_key = self.blob_idx(d, &mut o);
        let name = self.str_idx(d, &mut o);
        let culture = self.str_idx(d, &mut o);
        AssemblyRow {
            hash_alg,
            major,
            minor,
            build,
            revision,
            flags,
            public_key,
            name,
            culture,
        }
    }

    pub fn get_assembly_ref(&self, row: u32) -> AssemblyRefRow {
        let d = self.row_data(TableId::AssemblyRef, row);
        let mut o = 0;
        let major = self.read_u16(d, &mut o);
        let minor = self.read_u16(d, &mut o);
        let build = self.read_u16(d, &mut o);
        let revision = self.read_u16(d, &mut o);
        let flags = self.read_u32_val(d, &mut o);
        let public_key_or_token = self.blob_idx(d, &mut o);
        let name = self.str_idx(d, &mut o);
        let culture = self.str_idx(d, &mut o);
        let hash_value = self.blob_idx(d, &mut o);
        AssemblyRefRow {
            major,
            minor,
            build,
            revision,
            flags,
            public_key_or_token,
            name,
            culture,
            hash_value,
        }
    }

    pub fn get_nested_class(&self, row: u32) -> NestedClassRow {
        let d = self.row_data(TableId::NestedClass, row);
        let mut o = 0;
        let nested_class = self.table_idx(TableId::TypeDef, d, &mut o);
        let enclosing_class = self.table_idx(TableId::TypeDef, d, &mut o);
        NestedClassRow {
            nested_class,
            enclosing_class,
        }
    }

    pub fn get_generic_param(&self, row: u32) -> GenericParamRow {
        let d = self.row_data(TableId::GenericParam, row);
        let mut o = 0;
        let number = self.read_u16(d, &mut o);
        let flags = self.read_u16(d, &mut o);
        let owner = self.coded_idx(CodedIndex::TypeOrMethodDef, d, &mut o);
        let name = self.str_idx(d, &mut o);
        GenericParamRow {
            number,
            flags,
            owner,
            name,
        }
    }

    pub fn get_method_spec(&self, row: u32) -> MethodSpecRow {
        let d = self.row_data(TableId::MethodSpec, row);
        let mut o = 0;
        let method = self.coded_idx(CodedIndex::MethodDefOrRef, d, &mut o);
        let instantiation = self.blob_idx(d, &mut o);
        MethodSpecRow {
            method,
            instantiation,
        }
    }

    pub fn get_class_layout(&self, row: u32) -> ClassLayoutRow {
        let d = self.row_data(TableId::ClassLayout, row);
        let mut o = 0;
        let packing_size = self.read_u16(d, &mut o);
        let class_size = self.read_u32_val(d, &mut o);
        let parent = self.table_idx(TableId::TypeDef, d, &mut o);
        ClassLayoutRow {
            packing_size,
            class_size,
            parent,
        }
    }

    pub fn get_field_rva(&self, row: u32) -> FieldRvaRow {
        let d = self.row_data(TableId::FieldRva, row);
        let mut o = 0;
        let rva = self.read_u32_val(d, &mut o);
        let field = self.table_idx(TableId::Field, d, &mut o);
        FieldRvaRow { rva, field }
    }

    pub fn get_interface_impl(&self, row: u32) -> InterfaceImplRow {
        let d = self.row_data(TableId::InterfaceImpl, row);
        let mut o = 0;
        let class = self.table_idx(TableId::TypeDef, d, &mut o);
        let interface = self.coded_idx(CodedIndex::TypeDefOrRef, d, &mut o);
        InterfaceImplRow { class, interface }
    }

    // --- RVA resolution ---

    pub fn rva_to_offset(&self, rva: u32) -> Option<usize> {
        rva_to_offset(&self.sections, rva)
    }

    pub fn rva_slice(&self, rva: u32, len: usize) -> Option<&[u8]> {
        let off = self.rva_to_offset(rva)?;
        self.data.get(off..off + len)
    }

    pub fn rva_data_from(&self, rva: u32) -> &[u8] {
        match self.rva_to_offset(rva) {
            Some(off) => self.data.get(off..).unwrap_or(&[]),
            None => &[],
        }
    }

    pub fn method_body(&self, rva: u32) -> Option<MethodBody> {
        if rva == 0 {
            return None;
        }
        let data = self.rva_data_from(rva);
        if data.is_empty() {
            return None;
        }
        parse_method_body(data)
    }

    // --- Helpers ---

    pub fn type_methods(&self, type_row: u32) -> std::ops::Range<u32> {
        let td = self.get_type_def(type_row);
        let end = if type_row < self.row_count(TableId::TypeDef) {
            self.get_type_def(type_row + 1).method_list
        } else {
            self.row_count(TableId::MethodDef) + 1
        };
        td.method_list..end
    }

    pub fn type_fields(&self, type_row: u32) -> std::ops::Range<u32> {
        let td = self.get_type_def(type_row);
        let end = if type_row < self.row_count(TableId::TypeDef) {
            self.get_type_def(type_row + 1).field_list
        } else {
            self.row_count(TableId::Field) + 1
        };
        td.field_list..end
    }

    pub fn method_params(&self, method_row: u32) -> std::ops::Range<u32> {
        let md = self.get_method_def(method_row);
        let end = if method_row < self.row_count(TableId::MethodDef) {
            self.get_method_def(method_row + 1).param_list
        } else {
            self.row_count(TableId::Param) + 1
        };
        md.param_list..end
    }

    pub fn find_impl_map(&self, method_token: Token) -> Option<ImplMapRow> {
        let coded = (method_token.rid() << 1) | 1;
        for i in 1..=self.row_count(TableId::ImplMap) {
            let row = self.get_impl_map(i);
            if row.member_forwarded == coded {
                return Some(row);
            }
        }
        None
    }
}

fn rva_to_offset(sections: &[SectionInfo], rva: u32) -> Option<usize> {
    for s in sections {
        // Calcul en 64 bits : `rva + vsize` d'une section corrompue déborde.
        if rva >= s.rva && (rva as u64) < s.rva as u64 + s.vsize as u64 {
            return Some((s.raw_ptr as u64 + (rva - s.rva) as u64) as usize);
        }
    }
    None
}

fn decode_blob_length(data: &[u8]) -> (usize, usize) {
    match data.first().copied() {
        None => (0, 0),
        Some(b) if b & 0x80 == 0 => (b as usize, 1),
        Some(b) if b & 0xC0 == 0x80 => {
            let hi = (b & 0x3F) as usize;
            let lo = data.get(1).copied().unwrap_or(0) as usize;
            (hi << 8 | lo, 2)
        }
        Some(b) => {
            let b0 = (b & 0x1F) as usize;
            let b1 = data.get(1).copied().unwrap_or(0) as usize;
            let b2 = data.get(2).copied().unwrap_or(0) as usize;
            let b3 = data.get(3).copied().unwrap_or(0) as usize;
            (b0 << 24 | b1 << 16 | b2 << 8 | b3, 4)
        }
    }
}

#[derive(Debug, Clone)]
pub struct MethodBody {
    pub max_stack: u16,
    pub code: Vec<u8>,
    pub locals: Vec<u8>,
    pub local_var_sig_token: u32,
    pub exception_clauses: Vec<ExceptionClause>,
}

#[derive(Debug, Clone)]
pub struct ExceptionClause {
    pub flags: u32,
    pub try_offset: u32,
    pub try_length: u32,
    pub handler_offset: u32,
    pub handler_length: u32,
    pub class_token_or_filter: u32,
}

fn parse_method_body(data: &[u8]) -> Option<MethodBody> {
    let first = *data.first()?;
    if first & 0x03 == 0x02 {
        let size = (first >> 2) as usize;
        Some(MethodBody {
            max_stack: 8,
            code: data.get(1..1 + size)?.to_vec(),
            locals: Vec::new(),
            local_var_sig_token: 0,
            exception_clauses: Vec::new(),
        })
    } else if first & 0x03 == 0x03 {
        let flags = rd16(data, 0);
        let header_size = ((flags >> 12) & 0xF) as usize * 4;
        let max_stack = rd16(data, 2);
        let code_size = rd32(data, 4) as usize;
        let local_var_sig_token = rd32(data, 8);
        let code = data.get(header_size..header_size + code_size)?.to_vec();

        let mut exception_clauses = Vec::new();
        if flags & 0x08 != 0 {
            let mut sect_off = (header_size + code_size + 3) & !3;
            while sect_off < data.len() {
                let kind = *data.get(sect_off)?;
                let is_fat = kind & 0x40 != 0;
                let has_more = kind & 0x80 != 0;
                if kind & 0x01 != 0 {
                    if is_fat {
                        let sect_size = (rd32(data, sect_off) >> 8) as usize;
                        if sect_size < 4 {
                            break;
                        }
                        // Clauses bornées par la section ET par le corps.
                        let count = ((sect_size - 4) / 24)
                            .min(data.len().saturating_sub(sect_off + 4) / 24);
                        for i in 0..count {
                            let base = sect_off + 4 + i * 24;
                            exception_clauses.push(ExceptionClause {
                                flags: rd32(data, base),
                                try_offset: rd32(data, base + 4),
                                try_length: rd32(data, base + 8),
                                handler_offset: rd32(data, base + 12),
                                handler_length: rd32(data, base + 16),
                                class_token_or_filter: rd32(data, base + 20),
                            });
                        }
                        sect_off += sect_size;
                    } else {
                        let sect_size = *data.get(sect_off + 1)? as usize;
                        if sect_size < 4 {
                            break;
                        }
                        let count = ((sect_size - 4) / 12)
                            .min(data.len().saturating_sub(sect_off + 4) / 12);
                        for i in 0..count {
                            let base = sect_off + 4 + i * 12;
                            exception_clauses.push(ExceptionClause {
                                flags: rd16(data, base) as u32,
                                try_offset: rd16(data, base + 2) as u32,
                                try_length: *data.get(base + 4)? as u32,
                                handler_offset: rd16(data, base + 5) as u32,
                                handler_length: *data.get(base + 7)? as u32,
                                class_token_or_filter: rd32(data, base + 8),
                            });
                        }
                        sect_off += sect_size;
                    }
                } else {
                    break;
                }
                if !has_more {
                    break;
                }
                sect_off = (sect_off + 3) & !3;
            }
        }

        Some(MethodBody {
            max_stack,
            code,
            locals: Vec::new(),
            local_var_sig_token,
            exception_clauses,
        })
    } else {
        None
    }
}

pub fn decode_compressed_u32(data: &[u8], off: &mut usize) -> u32 {
    let (val, hdr) = decode_blob_length(data.get(*off..).unwrap_or(&[]));
    *off += hdr;
    val as u32
}

/// Éléments de type dans les signatures (ECMA-335 §II.23.1.16)
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ElementType {
    Void,
    Boolean,
    Char,
    I1,
    U1,
    I2,
    U2,
    I4,
    U4,
    I8,
    U8,
    R4,
    R8,
    String,
    Ptr(Box<ElementType>),
    ByRef(Box<ElementType>),
    ValueType(u32),
    Class(u32),
    Var(u32),
    Array(Box<ElementType>),
    GenericInst(Box<ElementType>, Vec<ElementType>),
    TypedByRef,
    IntPtr,
    UIntPtr,
    FnPtr,
    Object,
    SzArray(Box<ElementType>),
    MVar(u32),
    Sentinel,
}

/// Profondeur maximale d'imbrication d'une signature de type (`T[][]...`,
/// `List<List<...>>`) : au-delà, le blob est tenu pour corrompu.
const MAX_TYPE_DEPTH: u32 = 64;

/// Nombre d'éléments annoncé par un blob, borné par les octets restants
/// (chaque élément occupe au moins un octet).
fn bounded_count(count: u32, blob: &[u8], off: usize) -> usize {
    (count as usize).min(blob.len().saturating_sub(off))
}

pub fn decode_type(blob: &[u8], off: &mut usize) -> ElementType {
    decode_type_depth(blob, off, 0)
}

fn decode_type_depth(blob: &[u8], off: &mut usize, depth: u32) -> ElementType {
    if depth > MAX_TYPE_DEPTH {
        *off = blob.len();
        return ElementType::Object;
    }
    let decode_type = |blob: &[u8], off: &mut usize| decode_type_depth(blob, off, depth + 1);
    let tag = blob.get(*off).copied().unwrap_or(0);
    *off += 1;
    match tag {
        0x01 => ElementType::Void,
        0x02 => ElementType::Boolean,
        0x03 => ElementType::Char,
        0x04 => ElementType::I1,
        0x05 => ElementType::U1,
        0x06 => ElementType::I2,
        0x07 => ElementType::U2,
        0x08 => ElementType::I4,
        0x09 => ElementType::U4,
        0x0A => ElementType::I8,
        0x0B => ElementType::U8,
        0x0C => ElementType::R4,
        0x0D => ElementType::R8,
        0x0E => ElementType::String,
        0x0F => {
            let inner = decode_type(blob, off);
            ElementType::Ptr(Box::new(inner))
        }
        0x10 => {
            let inner = decode_type(blob, off);
            ElementType::ByRef(Box::new(inner))
        }
        0x11 => {
            let token = decode_compressed_u32(blob, off);
            ElementType::ValueType(token)
        }
        0x12 => {
            let token = decode_compressed_u32(blob, off);
            ElementType::Class(token)
        }
        0x13 => {
            let idx = decode_compressed_u32(blob, off);
            ElementType::Var(idx)
        }
        0x14 => {
            let inner = decode_type(blob, off);
            ElementType::Array(Box::new(inner))
        }
        0x15 => {
            let inner = decode_type(blob, off);
            let count = bounded_count(decode_compressed_u32(blob, off), blob, *off);
            let mut args = Vec::with_capacity(count);
            for _ in 0..count {
                args.push(decode_type(blob, off));
            }
            ElementType::GenericInst(Box::new(inner), args)
        }
        0x16 => ElementType::TypedByRef,
        0x18 => ElementType::IntPtr,
        0x19 => ElementType::UIntPtr,
        0x1B => ElementType::FnPtr,
        0x1C => ElementType::Object,
        0x1D => {
            let inner = decode_type(blob, off);
            ElementType::SzArray(Box::new(inner))
        }
        0x1E => {
            let idx = decode_compressed_u32(blob, off);
            ElementType::MVar(idx)
        }
        0x41 => ElementType::Sentinel,
        _ => ElementType::Object,
    }
}

pub struct MethodSig {
    pub calling_convention: u8,
    pub generic_param_count: u32,
    pub ret_type: ElementType,
    pub param_types: Vec<ElementType>,
}

impl MethodSig {
    pub fn has_this(&self) -> bool {
        self.calling_convention & 0x20 != 0
    }
}

pub fn decode_method_sig(blob: &[u8]) -> MethodSig {
    let mut off = 0;
    let cc = blob.get(off).copied().unwrap_or(0);
    off += 1;
    let generic_param_count = if cc & 0x10 != 0 {
        decode_compressed_u32(blob, &mut off)
    } else {
        0
    };
    let param_count = bounded_count(decode_compressed_u32(blob, &mut off), blob, off);
    let ret_type = decode_type(blob, &mut off);
    let mut param_types = Vec::with_capacity(param_count);
    for _ in 0..param_count {
        param_types.push(decode_type(blob, &mut off));
    }
    MethodSig {
        calling_convention: cc,
        generic_param_count,
        ret_type,
        param_types,
    }
}

pub fn decode_locals_sig(blob: &[u8]) -> Vec<ElementType> {
    if blob.is_empty() {
        return Vec::new();
    }
    let mut off = 0;
    let tag = blob.get(off).copied().unwrap_or(0);
    off += 1;
    if tag != 0x07 {
        return Vec::new();
    }
    let count = bounded_count(decode_compressed_u32(blob, &mut off), blob, off);
    let mut types = Vec::with_capacity(count);
    for _ in 0..count {
        types.push(decode_type(blob, &mut off));
    }
    types
}
