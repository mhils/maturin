use super::InterpreterKind;
use crate::target::{Arch, Os};
use once_cell::sync::Lazy;
use serde::Deserialize;
use std::collections::HashMap;

/// Some of the sysconfigdata of Python interpreter we care about
#[derive(Debug, Clone, Deserialize)]
pub struct Sysconfig {
    /// Python's major version
    pub major: usize,
    /// Python's minor version
    pub minor: usize,
    /// cpython or pypy
    #[serde(rename = "interpreter")]
    pub interpreter_kind: InterpreterKind,
    /// For linux and mac, this contains the value of the abiflags, e.g. "m"
    /// for python3.7m or "dm" for python3.6dm. Since python3.8, the value is
    /// empty. On windows, the value was always "".
    ///
    /// See PEP 261 and PEP 393 for details
    pub abiflags: String,
    /// Suffix to use for extension modules as given by sysconfig.
    pub ext_suffix: String,
    /// Part of sysconfig's SOABI specifying {major}{minor}{abiflags}
    ///
    /// Note that this always `None` on windows
    pub abi_tag: Option<String>,
}

pub static WELL_KNOWN_SYSCONFIG: Lazy<HashMap<Os, HashMap<Arch, Vec<Sysconfig>>>> =
    Lazy::new(|| {
        let sysconfig = serde_json::from_slice(include_bytes!("sysconfig.json"))
            .expect("invalid sysconfig.json");
        sysconfig
    });

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_load_sysconfig() {
        let linux_sysconfig = WELL_KNOWN_SYSCONFIG.get(&Os::Linux).unwrap();
        assert!(linux_sysconfig.contains_key(&Arch::X86_64));
    }
}
