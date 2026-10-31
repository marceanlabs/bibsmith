#![no_main]

//! The control file: XML that LaTeX writes and a user can damage.

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Ok(src) = std::str::from_utf8(data) else { return };
    let _ = bibsmith::bcf::ControlFile::parse(src);
});
