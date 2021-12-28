use super::InterpreterKind;
use crate::target::{Arch, Os};
use once_cell::sync::Lazy;
use std::collections::HashMap;

/// Some of the sysconfigdata of Python interpreter we care about
#[derive(Debug, Clone)]
pub struct Sysconfig {
    /// Python's major version
    pub major: usize,
    /// Python's minor version
    pub minor: usize,
    /// cpython or pypy
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

pub static WELL_KNOWN_SYSCONFIG: Lazy<HashMap<Os, HashMap<Arch, Sysconfig>>> = Lazy::new(|| {
    let mut linux = HashMap::new();
    linux.insert(
        Arch::X86_64,
        Sysconfig {
            major: 3,
            minor: 6,
            interpreter_kind: InterpreterKind::CPython,
            abiflags: "m".to_string(),
            ext_suffix: ".cpython-36m-x86_64-linux-gnu.so".to_string(),
            abi_tag: Some("36m".to_string()),
        },
    );
    let mut well_known = HashMap::new();
    well_known.insert(Os::Linux, linux);
    well_known
});
