//! Embeds the Windows resources in the executable: the app icon (what
//! Explorer, shortcuts and the Start menu show) and version info (the name in
//! Task Manager and the file's Properties).
//!
//! Rather than depending on `rc.exe` / `windres` being installed, this writes
//! the COFF object holding the `.rsrc` section directly and passes it to the
//! linker, which works the same with the GNU and MSVC toolchains.

use std::env;
use std::path::PathBuf;

const ICON: &str = "assets/CleanPDF.ico";
const NAME: &str = "CleanPDF";
const LANG_EN_US: u32 = 0x0409;

const RT_ICON: u32 = 3;
const RT_GROUP_ICON: u32 = 14;
const RT_VERSION: u32 = 16;

fn main() {
    println!("cargo:rerun-if-changed={ICON}");
    println!("cargo:rerun-if-changed=build.rs");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let machine: u16 = match env::var("CARGO_CFG_TARGET_ARCH").unwrap().as_str() {
        "x86_64" => 0x8664,
        "x86" => 0x014c,
        "aarch64" => 0xaa64,
        arch => {
            println!("cargo:warning=no Windows resources for {arch}");
            return;
        }
    };

    let mut resources: Vec<(u32, u32, Vec<u8>)> = Vec::new();
    match std::fs::read(ICON) {
        Ok(ico) => {
            let (group, icons) = split_ico(&ico);
            resources.extend(icons.into_iter().enumerate().map(|(i, image)| (RT_ICON, i as u32 + 1, image)));
            resources.push((RT_GROUP_ICON, 1, group));
        }
        // Lets the generator (`app_icon::tests::write_exe_icon`) build
        // before the icon exists.
        Err(e) => println!("cargo:warning=no exe icon ({ICON}: {e})"),
    }
    resources.push((RT_VERSION, 1, version_info()));

    let out = PathBuf::from(env::var("OUT_DIR").unwrap()).join("resources.o");
    std::fs::write(&out, coff_object(machine, &resources)).unwrap();
    println!("cargo:rustc-link-arg-bins={}", out.display());
}

/// Splits an .ico file into the RT_GROUP_ICON directory (which refers to the
/// images by resource ID 1..n) and the RT_ICON images themselves.
fn split_ico(ico: &[u8]) -> (Vec<u8>, Vec<Vec<u8>>) {
    let u16_at = |i: usize| u16::from_le_bytes([ico[i], ico[i + 1]]);
    let u32_at = |i: usize| u32::from_le_bytes(ico[i..i + 4].try_into().unwrap());
    assert!(u16_at(0) == 0 && u16_at(2) == 1, "{ICON} is not an icon file");
    let count = u16_at(4) as usize;
    let mut group = ico[..6].to_vec();
    let mut images = Vec::with_capacity(count);
    for i in 0..count {
        let entry = 6 + 16 * i;
        let (len, offset) = (u32_at(entry + 8) as usize, u32_at(entry + 12) as usize);
        images.push(ico[offset..offset + len].to_vec());
        // GRPICONDIRENTRY: the ICONDIRENTRY with the file offset replaced by
        // a 16-bit resource ID.
        group.extend(&ico[entry..entry + 12]);
        group.extend((i as u16 + 1).to_le_bytes());
    }
    (group, images)
}

/// A VS_VERSIONINFO resource built from the package version.
fn version_info() -> Vec<u8> {
    let version = env::var("CARGO_PKG_VERSION").unwrap();
    let part = |name: &str| env::var(name).unwrap().parse::<u32>().unwrap_or(0).min(0xffff);
    let (major, minor, patch) = (part("CARGO_PKG_VERSION_MAJOR"), part("CARGO_PKG_VERSION_MINOR"), part("CARGO_PKG_VERSION_PATCH"));
    let (ms, ls) = (major << 16 | minor, patch << 16);
    let fixed: Vec<u8> = [
        0xfeef04bd, // signature
        0x0001_0000,
        ms,
        ls, // file version
        ms,
        ls,      // product version
        0x3f,    // flags mask
        0,       // flags
        0x40004, // VOS_NT_WINDOWS32
        1,       // VFT_APP
        0,
        0,
        0,
    ]
    .iter()
    .flat_map(|v: &u32| v.to_le_bytes())
    .collect();

    let file_name = format!("{NAME}.exe");
    let strings = [
        ("FileDescription", NAME),
        ("FileVersion", &version),
        ("InternalName", NAME),
        ("OriginalFilename", &file_name),
        ("ProductName", NAME),
        ("ProductVersion", &version),
    ];
    let table = strings.iter().map(|(k, v)| node(k, Value::Text(v), vec![])).collect();
    let string_info = node("StringFileInfo", Value::None, vec![node("040904B0", Value::None, table)]);
    let translation = [LANG_EN_US as u16, 1200].iter().flat_map(|v| v.to_le_bytes()).collect();
    let var_info = node("VarFileInfo", Value::None, vec![node("Translation", Value::Binary(translation), vec![])]);
    node("VS_VERSION_INFO", Value::Binary(fixed), vec![string_info, var_info])
}

enum Value<'a> {
    None,
    Binary(Vec<u8>),
    Text(&'a str),
}

/// One version-info block: length, value length, type, UTF-16 key, then the
/// value and children, each aligned to 32 bits.
fn node(key: &str, value: Value, children: Vec<Vec<u8>>) -> Vec<u8> {
    let utf16 = |s: &str| s.encode_utf16().chain([0]).flat_map(u16::to_le_bytes).collect::<Vec<u8>>();
    let align = |b: &mut Vec<u8>| b.resize(b.len().next_multiple_of(4), 0);
    let (value_len, is_text, value) = match value {
        Value::None => (0, 0, vec![]),
        Value::Binary(b) => (b.len(), 0, b),
        Value::Text(s) => (s.encode_utf16().count() + 1, 1, utf16(s)),
    };
    let mut b = vec![0, 0];
    b.extend((value_len as u16).to_le_bytes());
    b.extend((is_text as u16).to_le_bytes());
    b.extend(utf16(key));
    align(&mut b);
    b.extend(value);
    for child in children {
        align(&mut b);
        b.extend(child);
    }
    let len = b.len() as u16;
    b[..2].copy_from_slice(&len.to_le_bytes());
    b
}

/// A COFF object with a single `.rsrc` section holding `resources`, given as
/// (type, id, data) and laid out as the standard three-level type / name /
/// language directory tree.
fn coff_object(machine: u16, resources: &[(u32, u32, Vec<u8>)]) -> Vec<u8> {
    let mut types: Vec<u32> = resources.iter().map(|r| r.0).collect();
    types.dedup();
    let ids_of = |t: u32| resources.iter().filter(move |r| r.0 == t);

    // Layout: root directory, one name directory per type, one language
    // directory per resource, the data entries, then the data itself.
    let dir_size = |entries: usize| 16 + 8 * entries;
    let mut name_dirs = Vec::new();
    let mut at = dir_size(types.len());
    for &t in &types {
        name_dirs.push(at);
        at += dir_size(ids_of(t).count());
    }
    let lang_dirs: Vec<usize> = (0..resources.len()).map(|i| at + i * dir_size(1)).collect();
    at += resources.len() * dir_size(1);
    let data_entries: Vec<usize> = (0..resources.len()).map(|i| at + i * 16).collect();
    at += resources.len() * 16;
    let mut data_offsets = Vec::new();
    for r in resources {
        at = at.next_multiple_of(8);
        data_offsets.push(at);
        at += r.2.len();
    }

    let mut rsrc = vec![0u8; at.next_multiple_of(4)];
    let put = |buf: &mut Vec<u8>, at: usize, v: u32| buf[at..at + 4].copy_from_slice(&v.to_le_bytes());
    let write_dir = |buf: &mut Vec<u8>, at: usize, entries: &[(u32, u32)]| {
        // Characteristics, timestamp, version: zero; then 0 named, N id entries.
        buf[at + 14..at + 16].copy_from_slice(&(entries.len() as u16).to_le_bytes());
        for (i, &(id, target)) in entries.iter().enumerate() {
            put(buf, at + 16 + 8 * i, id);
            put(buf, at + 20 + 8 * i, target);
        }
    };
    const SUBDIR: u32 = 0x8000_0000;
    let root: Vec<(u32, u32)> = types.iter().zip(&name_dirs).map(|(&t, &d)| (t, SUBDIR | d as u32)).collect();
    write_dir(&mut rsrc, 0, &root);
    let mut index = 0;
    for (&t, &dir) in types.iter().zip(&name_dirs) {
        let mut names = Vec::new();
        for r in ids_of(t) {
            names.push((r.1, SUBDIR | lang_dirs[index] as u32));
            index += 1;
        }
        write_dir(&mut rsrc, dir, &names);
    }
    let mut relocations = Vec::new();
    for (i, r) in resources.iter().enumerate() {
        write_dir(&mut rsrc, lang_dirs[i], &[(LANG_EN_US, data_entries[i] as u32)]);
        // The data entry holds the data's RVA: store the section offset and
        // let the linker add the section's address via a relocation.
        put(&mut rsrc, data_entries[i], data_offsets[i] as u32);
        put(&mut rsrc, data_entries[i] + 4, r.2.len() as u32);
        relocations.push(data_entries[i] as u32);
        rsrc[data_offsets[i]..data_offsets[i] + r.2.len()].copy_from_slice(&r.2);
    }

    let reloc_type: u16 = match machine {
        0x8664 => 0x0003, // IMAGE_REL_AMD64_ADDR32NB
        0x014c => 0x0007, // IMAGE_REL_I386_DIR32NB
        _ => 0x0002,      // IMAGE_REL_ARM64_ADDR32NB
    };
    let header_size = 20 + 40;
    let reloc_at = header_size + rsrc.len();
    let symbols_at = reloc_at + 10 * relocations.len();

    let mut obj = Vec::new();
    // File header.
    obj.extend(machine.to_le_bytes());
    obj.extend(1u16.to_le_bytes()); // sections
    obj.extend(0u32.to_le_bytes()); // timestamp: keep builds reproducible
    obj.extend((symbols_at as u32).to_le_bytes());
    obj.extend(1u32.to_le_bytes()); // symbols
    obj.extend(0u16.to_le_bytes()); // optional header size
    obj.extend(0u16.to_le_bytes()); // characteristics
    // Section header.
    obj.extend(b".rsrc\0\0\0");
    obj.extend(0u32.to_le_bytes()); // virtual size
    obj.extend(0u32.to_le_bytes()); // virtual address
    obj.extend((rsrc.len() as u32).to_le_bytes());
    obj.extend((header_size as u32).to_le_bytes());
    obj.extend((reloc_at as u32).to_le_bytes());
    obj.extend(0u32.to_le_bytes()); // line numbers
    obj.extend((relocations.len() as u16).to_le_bytes());
    obj.extend(0u16.to_le_bytes());
    // Initialized data, 4-byte aligned, readable.
    obj.extend((0x0000_0040u32 | 0x0030_0000 | 0x4000_0000).to_le_bytes());
    obj.extend(&rsrc);
    for offset in relocations {
        obj.extend(offset.to_le_bytes());
        obj.extend(0u32.to_le_bytes()); // symbol 0: the section
        obj.extend(reloc_type.to_le_bytes());
    }
    // Symbol table: the section symbol the relocations refer to.
    obj.extend(b".rsrc\0\0\0");
    obj.extend(0u32.to_le_bytes()); // value
    obj.extend(1u16.to_le_bytes()); // section number
    obj.extend(0u16.to_le_bytes()); // type
    obj.push(3); // IMAGE_SYM_CLASS_STATIC
    obj.push(0); // aux symbols
    obj.extend(4u32.to_le_bytes()); // empty string table
    obj
}
