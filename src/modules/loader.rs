use std::path::Path;

use libloading::{Library, Symbol};

use crate::Module;

pub struct DynamicModule {
    _lib: Library,
    instance: Box<dyn Module>,
}

impl DynamicModule {
    /// Загружает .so/.dll и достаёт символы `create_module` и `destroy_module`
    ///
    /// # Safety
    ///
    /// Вызывающий должен гарантировать, что библиотека безопасна:
    /// - Экспортирует корректную функцию `create_module() -> *mut dyn Module`.
    /// - Возвращаемый указатель живой и будет корректно уничтожен при завершении.
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
