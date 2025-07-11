use std::path::PathBuf;

use wasmtime::Engine;

use crate::{DynamicModule, Module, WasmPlugin};

/// Тип плагина: либо встроенный Rust, либо динамический .so/.dll, либо WASM
pub enum Plugin {
    Native(Box<dyn Module>),
    Dynamic(DynamicModule),
    Wasm(WasmPlugin),
}

/// Менеджер плагинов: загрузка, инициализация и рассылка команд.
pub struct Manager {
    plugins: Vec<Plugin>,
}

impl Plugin {
    pub fn init(&mut self) -> Result<(), String> {
        match self {
            Plugin::Native(m) => m.init(),
            Plugin::Dynamic(d) => d.module_mut().init(),
            Plugin::Wasm(w) => w.init(),
        }
    }

    pub fn handle(
        &mut self,
        cmd: &str,
        data: &[u8],
    ) -> Result<Vec<u8>, String> {
        match self {
            Plugin::Native(m) => m.handle(cmd, data),
            Plugin::Dynamic(d) => d.module_mut().handle(cmd, data),
            Plugin::Wasm(w) => w.handle(cmd, data),
        }
    }
}

impl Manager {
    /// Создаёт пустой менеджер.
    pub fn new() -> Self {
        Self {
            plugins: Vec::new(),
        }
    }

    /// Добавляет встроенный (статический скомпилированный) модуль.
    pub fn add_native(
        &mut self,
        m: Box<dyn Module>,
    ) {
        self.plugins.push(Plugin::Native(m));
    }

    /// Добавляет динамический плагин из .so/.dll.
    ///
    /// # Safety
    ///
    /// Вызывающий должен гарантировать, что:
    /// - Загружаемая библиотека безопасна.
    /// - Она экспортирует корректную функцию `create_module()` и использует совместимую ABI.
    /// - Модуль не вызывает неопределённого поведения при инициализации или вызове `handle`.
    pub unsafe fn add_dynamic(
        &mut self,
        path: PathBuf,
    ) -> Result<(), String> {
        let dm = DynamicModule::load(path)?;
        self.plugins.push(Plugin::Dynamic(dm));
        Ok(())
    }

    /// Добавляет WASM-плагин.
    pub fn add_wasm(
        &mut self,
        path: &str,
        engine: &Engine,
    ) -> Result<(), String> {
        let wp = WasmPlugin::load(path, engine)?;
        self.plugins.push(Plugin::Wasm(wp));
        Ok(())
    }

    /// Инициализация всех загруженных модулей.
    pub fn init_all(&mut self) -> Result<(), String> {
        for plugin in &mut self.plugins {
            plugin.init()?;
        }
        Ok(())
    }

    /// Передаёт комнду всем плагинам.
    pub fn broadcast(
        &mut self,
        cmd: &str,
        data: &[u8],
    ) {
        for plugin in &mut self.plugins {
            let _ = plugin.handle(cmd, data);
        }
    }

    pub fn load_module(
        &mut self,
        module: &mut dyn Module,
    ) -> Result<(), String> {
        module.on_load()?;
        module.init()?;
        Ok(())
    }

    pub fn unload_module(
        &mut self,
        module: &mut dyn Module,
    ) -> Result<(), String> {
        module.on_unload()?;
        Ok(())
    }

    pub fn reload_module(
        &mut self,
        module: &mut dyn Module,
    ) -> Result<(), String> {
        module.on_reload()?;
        Ok(())
    }
}

impl Default for Manager {
    fn default() -> Self {
        Self::new()
    }
}
