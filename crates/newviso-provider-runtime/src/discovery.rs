use libloading::{Library, Symbol};
use newviso_compat_abi::signature::{
    BootstrapPhase, ProviderKind, ProviderSignatureV1, RootSymbol, DESCRIPTOR_V2_SYMBOL,
    LEGACY_ROOT_SYMBOL, ROOT_SYMBOL, SIGNATURE_SYMBOL,
};
use std::{
    fmt, fs,
    path::{Path, PathBuf},
};

type SignatureFn = unsafe extern "C" fn() -> ProviderSignatureV1;

#[derive(Clone, Debug)]
pub struct ProviderInfo {
    pub path: PathBuf,
    pub id: String,
    pub name: String,
    pub version: String,
    pub kind: ProviderKind,
    pub bootstrap_phase: BootstrapPhase,
    pub root_symbol: RootSymbol,
    pub has_descriptor_v2: bool,
}

#[derive(Debug)]
pub enum ProbeError {
    Io(std::io::Error),
    DynamicLibrary(libloading::Error),
    MissingSignature(PathBuf),
}

impl fmt::Display for ProbeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(f, "I/O error: {error}"),
            Self::DynamicLibrary(error) => write!(f, "dynamic library error: {error}"),
            Self::MissingSignature(path) => {
                write!(f, "provider has no signature export: {}", path.display())
            }
        }
    }
}
impl std::error::Error for ProbeError {}
impl From<std::io::Error> for ProbeError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}
impl From<libloading::Error> for ProbeError {
    fn from(value: libloading::Error) -> Self {
        Self::DynamicLibrary(value)
    }
}

pub fn probe_directory(directory: impl AsRef<Path>) -> Result<Vec<ProviderInfo>, ProbeError> {
    let directory = directory.as_ref();
    if !directory.exists() {
        return Ok(Vec::new());
    }
    let mut paths = fs::read_dir(directory)?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| is_dynamic_library(path))
        .collect::<Vec<_>>();
    paths.sort();

    let mut providers = Vec::new();
    for path in paths {
        match probe_file(&path) {
            Ok(info) => providers.push(info),
            Err(ProbeError::MissingSignature(_)) => {}
            Err(error) => return Err(error),
        }
    }
    providers.sort_by(|a, b| {
        phase_order(a.bootstrap_phase)
            .cmp(&phase_order(b.bootstrap_phase))
            .then_with(|| a.id.cmp(&b.id))
    });
    Ok(providers)
}

pub fn probe_file(path: impl AsRef<Path>) -> Result<ProviderInfo, ProbeError> {
    let path = path.as_ref();
    let library = unsafe { Library::new(path)? };
    let signature_fn: Symbol<SignatureFn> = unsafe {
        library
            .get(SIGNATURE_SYMBOL)
            .map_err(|_| ProbeError::MissingSignature(path.to_path_buf()))?
    };
    let signature = unsafe { signature_fn() };

    let root_symbol = if unsafe { library.get::<*const ()>(ROOT_SYMBOL) }.is_ok() {
        RootSymbol::Current
    } else if unsafe { library.get::<*const ()>(LEGACY_ROOT_SYMBOL) }.is_ok() {
        RootSymbol::Legacy
    } else {
        RootSymbol::Missing
    };
    let has_descriptor_v2 = unsafe { library.get::<*const ()>(DESCRIPTOR_V2_SYMBOL) }.is_ok();

    Ok(ProviderInfo {
        path: path.to_path_buf(),
        id: signature.id.to_string(),
        name: signature.name.to_string(),
        version: signature.version.to_string(),
        kind: signature.kind,
        bootstrap_phase: signature.bootstrap_phase,
        root_symbol,
        has_descriptor_v2,
    })
}

fn is_dynamic_library(path: &Path) -> bool {
    path.extension()
        .and_then(|value| value.to_str())
        .map(|value| value.eq_ignore_ascii_case(std::env::consts::DLL_EXTENSION))
        .unwrap_or(false)
}
fn phase_order(phase: BootstrapPhase) -> u8 {
    match phase {
        BootstrapPhase::Bootstrap => 0,
        BootstrapPhase::Platform => 1,
        BootstrapPhase::Engine => 2,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn missing_directory_is_empty() {
        let path = std::env::temp_dir().join("newviso-provider-runtime-does-not-exist");
        assert!(probe_directory(path).unwrap().is_empty());
    }
}
