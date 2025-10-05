use std::path::Path;

use libloading::{Library, Symbol};

use crate::Module;

pub struct DynamicModule {
    _lib: Library,
    instance: Box<dyn Module>,
}

impl DynamicModule {
    /// Загружает .so/.dll и достаёт символы `create_module` и `destroy_module`.
    ///
    /// # Safety
    ///
    /// This function is `unsafe` because it loads and executes code from an
    /// external dynamic library and converts raw pointers into a `Box<dyn
    /// Module>`. The caller must ensure that:
    ///
    /// - The library at `path` was compiled with a compatible Rust toolchain
    ///   and ABI so that `create_module` returns a valid pointer to a `Box<dyn
    ///   Module>`.
    /// - The library exports a symbol `create_module` with signature `extern
    ///   "C" fn() -> *mut dyn Module` (or the exact signature you expect).
    /// - The returned pointer is non-null and points to a heap allocation that
    ///   can be safely converted into `Box<dyn Module>` and later dropped by
    ///   the host program.
    /// - Any allocator/ABI incompatibilities between host and plugin are
    ///   handled (preferably build host and plugin with the same Rust version
    ///   and settings).
    /// - Any thread-safety / synchronization invariants required by the module
    ///   implementation are respected by the caller.
    ///
    /// Failure to satisfy these conditions is undefined behavior.
    pub unsafe fn load<P: AsRef<Path>>(path: P) -> Result<Self, String> {
        let lib = Library::new(path.as_ref()).map_err(|e| format!("Failed to open lib: {e}"))?;
        let constructor: Symbol<unsafe fn() -> *mut dyn Module> = lib
            .get(b"create_module\0")
            .map_err(|e| format!("Symbol not found: {e}"))?;
        let raw = constructor();
        if raw.is_null() {
            return Err("Constructor returned null".into());
        }
        let instance = Box::from_raw(raw);

        Ok(DynamicModule {
            _lib: lib,
            instance,
        })
    }

    pub fn module(&self) -> &dyn Module {
        self.instance.as_ref()
    }

    pub fn module_mut(&mut self) -> &mut dyn Module {
        self.instance.as_mut()
    }
}
