//! The platform key of a system: `<os>-<arch>`, e.g. `linux-x86_64`,
//! `windows-x86_64`, `macos-aarch64` (Rust's `std::env::consts` names).
//!
//! *Plattform-Schlüssel `<os>-<arch>`.*

/// The key of the system this program runs on.
pub fn current() -> String {
    format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH)
}

/// Whether `key` looks like a platform key (`<os>-<arch>`, lower case).
pub fn is_valid(key: &str) -> bool {
    key.split_once('-').is_some_and(|(os, arch)| {
        let ok = |s: &str| {
            !s.is_empty()
                && s.chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
        };
        ok(os) && ok(arch)
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn keys() {
        assert!(super::is_valid(&super::current()));
        assert!(super::is_valid("macos-aarch64"));
        assert!(
            !super::is_valid("linux") && !super::is_valid("Linux-x86") && !super::is_valid("-x")
        );
    }
}
