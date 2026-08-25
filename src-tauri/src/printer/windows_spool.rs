//! Windows printer transport: the Print Spooler ("winspool"), not raw USB.
//!
//! Root cause this replaces: a thermal printer installed the normal Windows
//! way (driver installed, visible in "Devices and Printers", works from
//! every other application) is *not* reliably reachable as a raw USB
//! Printer-class (0x07) device once that driver is installed — Windows
//! binds its own printer class driver/service (`usbprint.sys`) to the
//! device's interface, and that binding blocks libusb/WinUSB from ever
//! claiming it. That's exactly why `printer::escpos`'s old
//! `send_to_printer_usb` (still used on macOS/Linux, where no such binding
//! exists) found nothing on the affected Windows machine even though the
//! printer worked fine everywhere else: it was looking for a USB endpoint a
//! properly-installed Windows printer driver had already claimed for
//! itself.
//!
//! The fix is to stop competing with the driver and go through it instead —
//! the same Print Spooler API (`winspool.drv`) every other Windows
//! application uses to print, just with the "RAW" datatype so the spooler
//! forwards our ESC/POS bytes straight to the printer's port unmodified
//! rather than trying to interpret them as a normal document. This module
//! is the only place that talks to `winspool` directly:
//! [`list_printers`] backs Settings' printer picker (mirrors
//! `android_bt::list_bonded_devices`'s role for Bluetooth), and [`send`]
//! is the transport `escpos::send_to_printer_dispatch` calls into on
//! Windows once a printer name has been selected there — parallel to how
//! `android_bt::send` is the Android transport for an already-selected
//! Bluetooth device.
//!
//! Uses the official `windows` crate (Microsoft's own generated Win32
//! bindings) rather than hand-written FFI or a smaller community crate —
//! it's the actively-maintained, canonical way to reach a raw Win32 API
//! like this from Rust, and Tauri's own Windows build already pulls it in
//! transitively, so this adds no new dependency *tree*, just the
//! `Win32_Graphics_Printing`/`Win32_Graphics_Gdi` features on top of it
//! (`OpenPrinterW`'s binding is itself gated on `Win32_Graphics_Gdi` in this
//! crate's generated bindings — an artifact of how the Win32 metadata
//! groups it, not anything to do with GDI drawing).

use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;

use windows::core::{Error as WinError, PCWSTR, PWSTR};
use windows::Win32::Graphics::Printing::{
    ClosePrinter, EndDocPrinter, EndPagePrinter, EnumPrintersW, OpenPrinterW, StartDocPrinterW,
    StartPagePrinter, WritePrinter, DOC_INFO_1W, PRINTER_ENUM_CONNECTIONS, PRINTER_ENUM_LOCAL,
    PRINTER_HANDLE, PRINTER_INFO_4W,
};

use super::escpos::PrinterError;

/// One printer Windows currently knows about (a local queue, or one
/// connected via a print server) — the candidate list for Settings' printer
/// picker. The Windows equivalent of `db::config::BluetoothDeviceOption`.
#[derive(Debug, Clone)]
pub struct WindowsPrinterInfo {
    pub name: String,
}

/// UTF-16, NUL-terminated — every winspool string parameter needs one of
/// these, never a Rust `&str` directly.
fn to_wide(s: &str) -> Vec<u16> {
    OsStr::new(s).encode_wide().chain(std::iter::once(0)).collect()
}

fn io_error(context: &str, err: WinError) -> PrinterError {
    PrinterError::Io(format!("{context}: {err}"))
}

/// Every printer installed on this machine — the same set "Devices and
/// Printers" shows — via `EnumPrintersW` at info level 4. Level 4
/// (`PRINTER_INFO_4W`: just a name, a server name, and attribute flags) is
/// used deliberately instead of the fuller level 2 — level 2 additionally
/// requires enough access to open/query the driver and port for every
/// single printer just to list names, which is both slower and more prone
/// to failing on a locked-down machine; naming is all Settings' picker
/// needs.
pub fn list_printers() -> Result<Vec<WindowsPrinterInfo>, PrinterError> {
    // PRINTER_ENUM_LOCAL: queues physically installed on this machine.
    // PRINTER_ENUM_CONNECTIONS: queues this machine has connected to on a
    // print server. Together, "every printer this Windows install would
    // show you in its own print dialog."
    let flags = PRINTER_ENUM_LOCAL | PRINTER_ENUM_CONNECTIONS;

    unsafe {
        // First pass with no buffer: winspool doesn't tell you the buffer
        // size any other way, so this call is *expected* to fail with
        // "insufficient buffer" — `needed`, its out-param, is the real
        // answer, not an error to bail out on.
        let mut needed: u32 = 0;
        let mut returned: u32 = 0;
        let _ = EnumPrintersW(flags, PCWSTR::null(), 4, None, &mut needed, &mut returned);

        if needed == 0 {
            return Ok(Vec::new());
        }

        let mut buffer = vec![0u8; needed as usize];
        EnumPrintersW(flags, PCWSTR::null(), 4, Some(&mut buffer), &mut needed, &mut returned)
            .map_err(|e| io_error("could not list Windows printers", e))?;

        // winspool packs `returned` fixed-size PRINTER_INFO_4W records at
        // the front of the buffer, with their string fields pointing at
        // variable-length data appended after them in the same allocation
        // — reading them back this way (rather than copying) is exactly
        // what the Win32 API contract for this call expects callers to do.
        let entries = std::slice::from_raw_parts(buffer.as_ptr().cast::<PRINTER_INFO_4W>(), returned as usize);

        Ok(entries
            .iter()
            .filter_map(|entry| {
                if entry.pPrinterName.is_null() {
                    None
                } else {
                    entry.pPrinterName.to_string().ok()
                }
            })
            .map(|name| WindowsPrinterInfo { name })
            .collect())
    }
}

/// Sends `bytes` to `printer_name` as one RAW print job through the Print
/// Spooler — the standard way a thermal receipt printer is driven on
/// Windows: `StartDocPrinterW`'s datatype is `"RAW"`, which tells the
/// spooler to hand our bytes straight to the printer's port as-is, with no
/// attempt to render them as a normal document the way a `.docx`/PDF job
/// would be. The ESC/POS byte content itself (item table, totals, logo
/// raster, pre-cut feed padding — every print-quality fix already applied
/// in `escpos.rs`) is unchanged; only the transport differs from
/// macOS/Linux's raw-USB path.
pub fn send(printer_name: &str, bytes: &[u8]) -> Result<(), PrinterError> {
    let printer_name_w = to_wide(printer_name);
    let mut doc_name_w = to_wide("POS Receipt");
    let mut datatype_w = to_wide("RAW");

    unsafe {
        let mut handle = PRINTER_HANDLE::default();
        OpenPrinterW(PCWSTR(printer_name_w.as_ptr()), &mut handle, None)
            .map_err(|e| io_error(&format!("could not open Windows printer \"{printer_name}\""), e))?;

        // Every exit path below must still reach `ClosePrinter` — there's no
        // Drop-based RAII wrapper here, so this closure-and-early-return
        // shape (rather than repeating the close call at every `?`) is what
        // keeps that guarantee without a handle leak on an error midway
        // through the job.
        let result = (|| -> Result<(), PrinterError> {
            let doc_info = DOC_INFO_1W {
                pDocName: PWSTR(doc_name_w.as_mut_ptr()),
                pOutputFile: PWSTR::null(),
                pDatatype: PWSTR(datatype_w.as_mut_ptr()),
            };

            let job_id = StartDocPrinterW(handle, 1, &doc_info);
            if job_id == 0 {
                return Err(io_error("could not start Windows print job", WinError::from_win32()));
            }

            let page_and_write_result = (|| -> Result<(), PrinterError> {
                StartPagePrinter(handle).ok().map_err(|e| io_error("could not start printer page", e))?;

                let mut written: u32 = 0;
                WritePrinter(handle, bytes.as_ptr().cast(), bytes.len() as u32, &mut written)
                    .ok()
                    .map_err(|e| io_error("could not write to Windows printer", e))?;
                if written as usize != bytes.len() {
                    return Err(PrinterError::Io(format!(
                        "Windows printer only accepted {written} of {} bytes",
                        bytes.len()
                    )));
                }

                EndPagePrinter(handle).ok().map_err(|e| io_error("could not end printer page", e))
            })();

            // `EndDocPrinter` closes out the job either way — an error
            // starting/writing the page must not leave a half-open job
            // sitting in the spooler queue.
            let end_doc_result =
                EndDocPrinter(handle).ok().map_err(|e| io_error("could not end Windows print job", e));

            page_and_write_result.and(end_doc_result)
        })();

        let _ = ClosePrinter(handle);
        result
    }
}
