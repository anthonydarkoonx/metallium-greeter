#![no_std]
//! The greeter's side of the Metallium-OS greeter contract (metallium-os `docs/GREETER-INTERFACE.md`).
//!
//! The screen task calls [`metallium_greeter_run`] while the data volume is locked, before anything else; it returns once nothing is left to ask. The screens and what they
//! do are in `greeter-core` (host-tested); this crate draws them with the OS's font, reads the keyboard driver's character stream and talks to the data store and the
//! session service.
extern crate alloc;

mod backend;
mod draw;

use greeter_core::{Greeter, Key, Outcome};
use metallium_driver_rt::proto::gpu::{GPU_FLUSH2, GPU_INFO};
use metallium_driver_rt::proto::input::KBD_POLL;
use metallium_driver_rt::{call, println, sleep};

/// Run the greeter. `gpu`, `input`, `store` and `sessions` are capability slots (the display driver, the input driver, the data store, the session service or 0);
/// `fb_handle` is where the scanout buffer is mapped (the screen task did it).
pub fn metallium_greeter_run(gpu: u64, input: u64, store: u64, sessions: u64, _framebuffer: u64, fb_handle: usize) {
    // This crate's copy of `metallium-driver-rt` has its own statics: like every boot service, it lets the kernel choose mapping addresses.
    metallium_driver_rt::auto_maps();
    let (st, r) = call(gpu, [GPU_INFO, 0, 0]);
    let (w, h) = if st == 0 && r[0] != 0 { ((r[0] & 0xFFFF_FFFF) as usize, (r[0] >> 32) as usize) } else { (1024, 768) };
    // SAFETY: `fb_handle` is the scanout buffer the screen task mapped read-write: `w * h` 32-bit words (the display driver's `GPU_INFO` size); the greeter is the only
    // code that draws while it runs (the screen task is waiting for this call to return).
    let fb = unsafe { core::slice::from_raw_parts_mut(fb_handle as *mut u32, w * h) };
    let mut be = backend::OsBackend { store, sessions };
    let mut g = Greeter::new(&mut be);
    println!("[greeter] up ({} screen)", g.view().title);
    let mut shown = None;
    'outer: loop {
        if g.poll(&mut be) == Outcome::Done {
            break;
        }
        let v = g.view();
        if shown.as_ref() != Some(&v) {
            draw::draw(fb, w, h, &v);
            let _ = call(gpu, [GPU_FLUSH2, (w as u64) << 32 | (h as u64) << 48, 0]);
            shown = Some(v);
        }
        let (st, r) = call(input, [KBD_POLL, 0, 0]);
        let n = if st == 0 { r[0].min(16) } else { 0 };
        if n == 0 {
            sleep(1);
            continue;
        }
        for i in 0..n {
            let byte = (r[1 + i as usize / 8] >> (8 * (i % 8))) as u8;
            if let Some(k) = Key::from_byte(byte) {
                if g.key(k, &mut be) == Outcome::Done {
                    break 'outer;
                }
            }
        }
    }
    // leave a clean screen for whatever comes next
    fb.fill(0x101820);
    let _ = call(gpu, [GPU_FLUSH2, (w as u64) << 32 | (h as u64) << 48, 0]);
    println!("[greeter] done");
}
