use super::{INPUT_LIMIT, inspect_bytes, request};
use alloc::{string::String, vec::Vec};
use core::alloc::{GlobalAlloc, Layout};
use wasi::filesystem::types::{DescriptorFlags, OpenFlags, PathFlags};

wit_bindgen::generate!({ path: "wit", world: "child", generate_all });
struct Child;
export!(Child);

impl exports::patina::repo_report::report::Guest for Child {
    fn inspect(input: String) -> Result<String, String> {
        request(&input).map_err(String::from)?;
        let root = wasi::filesystem::preopens::get_directories()
            .into_iter()
            .find(|(_, path)| path == "/project")
            .ok_or("input unavailable")?
            .0;
        let file = root
            .open_at(
                PathFlags::empty(),
                "Cargo.lock",
                OpenFlags::empty(),
                DescriptorFlags::READ,
            )
            .map_err(|_| "input unavailable")?;
        let mut bytes = Vec::new();
        loop {
            let remaining = INPUT_LIMIT + 1 - bytes.len();
            if remaining == 0 {
                return Err("input too large".into());
            }
            let (chunk, eof) = file
                .read(remaining.min(65536) as u64, bytes.len() as u64)
                .map_err(|_| "input unavailable")?;
            if chunk.len() > remaining || (chunk.is_empty() && !eof) {
                return Err("input unavailable".into());
            }
            bytes.extend_from_slice(&chunk);
            if bytes.len() > INPUT_LIMIT {
                return Err("input too large".into());
            }
            if eof {
                break;
            }
        }
        inspect_bytes(&input, &bytes).map_err(String::from)
    }
}

// The component is single-threaded; allocation is invocation-local in the host.
struct PageAlloc;
static mut NEXT: usize = 0;
static mut END: usize = 0;
unsafe impl GlobalAlloc for PageAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        unsafe {
            let Some(mut start) = NEXT
                .checked_add(layout.align() - 1)
                .map(|n| n & !(layout.align() - 1))
            else {
                return core::ptr::null_mut();
            };
            let Some(mut end) = start.checked_add(layout.size()) else {
                return core::ptr::null_mut();
            };
            if start == 0 || end > END {
                let Some(size) = layout
                    .size()
                    .checked_add(layout.align())
                    .and_then(|n| n.checked_add(65535))
                else {
                    return core::ptr::null_mut();
                };
                let pages = (size / 65536).max(1);
                let page = core::arch::wasm32::memory_grow::<0>(pages);
                if page == usize::MAX {
                    return core::ptr::null_mut();
                }
                start = (page * 65536 + layout.align() - 1) & !(layout.align() - 1);
                end = start + layout.size();
                END = (page + pages) * 65536;
            }
            NEXT = end;
            start as *mut u8
        }
    }
    unsafe fn dealloc(&self, _: *mut u8, _: Layout) {}
}
#[global_allocator]
static ALLOCATOR: PageAlloc = PageAlloc;
#[panic_handler]
fn panic(_: &core::panic::PanicInfo<'_>) -> ! {
    core::arch::wasm32::unreachable()
}
#[unsafe(no_mangle)]
unsafe extern "C" fn memcmp(a: *const u8, b: *const u8, n: usize) -> i32 {
    for i in 0..n {
        let (a, b) = unsafe { (*a.add(i), *b.add(i)) };
        if a != b {
            return i32::from(a) - i32::from(b);
        }
    }
    0
}
