//! Git Toy boundary test Child: explicit WASI 0.2.3 filesystem and WIT Git imports.
//! Built as a core module on wasm32-wasip1 and wrapped with `wasm-tools component new`
//! so the component imports exact WASI 0.2.3 interfaces rather than Rust 1.96's 0.2.6 adapter.
#![no_std]
extern crate alloc;

use alloc::{format, string::String, vec::Vec};
use core::alloc::{GlobalAlloc, Layout};
use wasi::filesystem::types::{Descriptor, DescriptorFlags, OpenFlags, PathFlags};
wit_bindgen::generate!({ path: "wit", world: "fixture", generate_all });

struct Fixture;
export!(Fixture);

fn read(root: &Descriptor, path: &str) -> Result<Vec<u8>, String> {
    let file = root
        .open_at(
            PathFlags::empty(),
            path,
            OpenFlags::empty(),
            DescriptorFlags::READ,
        )
        .map_err(|e| format!("open: {e:?}"))?;
    let (bytes, _eof) = file.read(65536, 0).map_err(|e| format!("read: {e:?}"))?;
    if bytes.len() >= 65536 {
        return Err("fixture input too large".into());
    }
    Ok(bytes)
}

fn write(root: &Descriptor, path: &str, bytes: &[u8]) -> Result<(), String> {
    let file = root
        .open_at(
            PathFlags::empty(),
            path,
            OpenFlags::CREATE | OpenFlags::TRUNCATE,
            DescriptorFlags::WRITE,
        )
        .map_err(|e| format!("open: {e:?}"))?;
    let mut offset = 0;
    while offset < bytes.len() {
        let n = file
            .write(&bytes[offset..], offset as u64)
            .map_err(|e| format!("write: {e:?}"))? as usize;
        if n == 0 || n > bytes.len() - offset {
            return Err("invalid write length".into());
        }
        offset += n;
    }
    Ok(())
}

impl exports::patina::demo::control::Guest for Fixture {
    fn run(mode: String) -> Result<String, String> {
        if !matches!(mode.as_str(), "ordinary" | "clean" | "process") {
            return Err("unknown fixture mode".into());
        }
        let root = wasi::filesystem::preopens::get_directories()
            .into_iter()
            .find(|(_, name)| name == "/project")
            .ok_or("no project delegation")?
            .0;
        if mode != "ordinary" {
            let targets =
                String::from_utf8(read(&root, "marker-paths")?).map_err(|_| "invalid paths")?;
            let mut markers = targets.lines();
            let inside = markers.next().ok_or("missing inside marker")?;
            let outside = markers.next().ok_or("missing outside marker")?;
            // Verify the delegated WASI filesystem preopen cannot directly open paths
            // outside the repository root; only an unconfined native helper could.
            if root
                .open_at(
                    PathFlags::empty(),
                    outside,
                    OpenFlags::CREATE,
                    DescriptorFlags::WRITE,
                )
                .is_ok()
            {
                return Err("unexpected direct outside access".into());
            }
            let quote = |p: &str| p.replace('\\', "\\\\").replace('"', "\\\"");
            let filter_cmd = format!(
                "/bin/sh -c 'printf executed > \"{}\"; printf executed > \"{}\"; exit 1'",
                quote(inside),
                quote(outside)
            );
            let mut config = read(&root, ".git/config")?;
            config.extend_from_slice(
                format!(
                    "\n[filter \"test-filter\"]\n\t{} = \"{}\"\n\trequired = true\n",
                    mode,
                    filter_cmd.replace('\\', "\\\\").replace('"', "\\\"")
                )
                .as_bytes(),
            );
            write(&root, ".git/config", &config)?;
            write(&root, ".gitattributes", b"README.md filter=test-filter\n")?;
        }
        write(&root, "README.md", b"written by authorized WASM Child\n")?;
        patina::git::git::add_paths(&["README.md".into()])?;
        patina::git::git::commit("commit from authorized WASM Child")
    }
}

struct PageAlloc;

unsafe impl GlobalAlloc for PageAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let Some(padded) = layout
            .size()
            .checked_add(layout.align() - 1)
            .and_then(|n| n.checked_add(65535))
        else {
            return core::ptr::null_mut();
        };
        let pages = (padded / 65536).max(1);
        let start = core::arch::wasm32::memory_grow::<0>(pages);
        if start == usize::MAX {
            return core::ptr::null_mut();
        }
        let base = start * 65536;
        ((base + layout.align() - 1) & !(layout.align() - 1)) as *mut u8
    }

    unsafe fn dealloc(&self, _ptr: *mut u8, _layout: Layout) {}
}

#[global_allocator]
static ALLOCATOR: PageAlloc = PageAlloc;

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo<'_>) -> ! {
    core::arch::wasm32::unreachable()
}

#[unsafe(no_mangle)]
unsafe extern "C" fn memcmp(a: *const u8, b: *const u8, n: usize) -> i32 {
    let mut i = 0;
    while i < n {
        let av = unsafe { *a.add(i) };
        let bv = unsafe { *b.add(i) };
        if av != bv {
            return (av as i32) - (bv as i32);
        }
        i += 1;
    }
    0
}
