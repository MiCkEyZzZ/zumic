use std::path::PathBuf;

use wasmtime::Engine;

use crate::{
    command_registry::CommandRegistry, db_context::DbContext, DynamicModule, Module, WasmPlugin,
};

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
    pub fn init(
        &mut self,
        registry: &mut CommandRegistry,
        ctx: &mut DbContext,
    ) -> Result<(), String> {
        match self {
            Plugin::Native(m) => m.init(registry, ctx),
            Plugin::Dynamic(d) => d.module_mut().init(registry, ctx),
            Plugin::Wasm(w) => w.init(registry, ctx),
        }
    }

    pub fn handle(
        &mut self,
        command: &str,
        data: &[u8],
        ctx: &mut DbContext,
    ) -> Result<Vec<u8>, String> {
        match self {
            Plugin::Native(m) => m.handle(command, data, ctx),
            Plugin::Dynamic(d) => d.module_mut().handle(command, data, ctx),
            Plugin::Wasm(w) => w.handle(command, data, ctx),
        }
    }

    pub fn on_load(
        &mut self,
        registry: &mut CommandRegistry,
        ctx: &mut DbContext,
    ) -> Result<(), String> {
        match self {
            Plugin::Native(m) => m.on_load(registry, ctx),
            Plugin::Dynamic(d) => d.module_mut().on_load(registry, ctx),
            Plugin::Wasm(w) => w.on_load(registry, ctx),
        }
    }

    pub fn on_unload(
        &mut self,
        ctx: &mut DbContext,
    ) -> Result<(), String> {
        match self {
            Plugin::Native(m) => m.on_unload(ctx),
            Plugin::Dynamic(d) => d.module_mut().on_unload(ctx),
            Plugin::Wasm(w) => w.on_unload(ctx),
        }
    }

    pub fn on_reload(
        &mut self,
        ctx: &mut DbContext,
    ) -> Result<(), String> {
        match self {
            Plugin::Native(m) => m.on_reload(ctx),
            Plugin::Dynamic(d) => d.module_mut().on_reload(ctx),
            Plugin::Wasm(w) => w.on_reload(ctx),
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
        module: Box<dyn Module>,
    ) {
        self.plugins.push(Plugin::Native(module));
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

    /// Инициализирует все плагины (`on_load` + `init`)
    pub fn init_all(
        &mut self,
        registry: &mut CommandRegistry,
        ctx: &mut DbContext,
    ) -> Result<(), String> {
        for plugin in &mut self.plugins {
            plugin.on_load(registry, ctx)?;
            plugin.init(registry, ctx)?;
        }
        Ok(())
    }

    /// Выгружает все плагины (`on_unload`)
    pub fn unload_all(
        &mut self,
        ctx: &mut DbContext,
    ) -> Result<(), String> {
        for plugin in &mut self.plugins {
            plugin.on_unload(ctx)?;
        }
        Ok(())
    }

    /// Перезагружает все плагины (`on_reload`)
    pub fn reload_all(
        &mut self,
        ctx: &mut DbContext,
    ) -> Result<(), String> {
        for plugin in &mut self.plugins {
            plugin.on_reload(ctx)?;
        }
        Ok(())
    }
}

impl Default for Manager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{command_registry::CommandRegistry, db_context::DbContext, Module};
    use std::sync::{Arc, Mutex};

    // Заглушка для Module для тестов
    struct DummyModule {
        init_called: Arc<Mutex<bool>>,
        handle_called: Arc<Mutex<bool>>,
    }

    impl DummyModule {
        fn new() -> Self {
            Self {
                init_called: Arc::new(Mutex::new(false)),
                handle_called: Arc::new(Mutex::new(false)),
            }
        }
    }

    impl Module for DummyModule {
        fn name(&self) -> &str {
            "dummy"
        }
        fn init(
            &mut self,
            _registry: &mut CommandRegistry,
            _ctx: &mut DbContext,
        ) -> Result<(), String> {
            *self.init_called.lock().unwrap() = true;
            Ok(())
        }
        fn handle(
            &mut self,
            _command: &str,
            _data: &[u8],
            _ctx: &mut DbContext,
        ) -> Result<Vec<u8>, String> {
            *self.handle_called.lock().unwrap() = true;
            Ok(b"ok".to_vec())
        }
        fn on_load(
            &mut self,
            _registry: &mut CommandRegistry,
            _ctx: &mut DbContext,
        ) -> Result<(), String> {
            Ok(())
        }
        fn on_unload(
            &mut self,
            _ctx: &mut DbContext,
        ) -> Result<(), String> {
            Ok(())
        }
        fn on_reload(
            &mut self,
            _ctx: &mut DbContext,
        ) -> Result<(), String> {
            Ok(())
        }
    }

    /// Тест проверяет, что модуль можно добавить в менеджер и корректно
    /// инициализировать через `init_all`.
    #[test]
    fn test_manager_add_and_init_native() {
        let mut manager = Manager::new();
        let mut registry = CommandRegistry::new(); // Предполагается, что есть метод new
        let mut ctx = DbContext::new_inmemory();

        let dummy = Box::new(DummyModule::new());
        manager.add_native(dummy);

        assert!(manager.init_all(&mut registry, &mut ctx).is_ok());

        // Можно проверить, что init был вызван, если у DummyModule флаги
        // Но для этого нужно вернуть объект DummyModule наружу или хранить Arc флаги в тесте
    }

    /// Тест проверяет, что вызов `handle` у модуля через менеджер работает
    /// корректно и возвращает ожидаемый результат.
    #[test]
    fn test_manager_handle_native() {
        let mut manager = Manager::new();
        let mut registry = CommandRegistry::new();
        let mut ctx = DbContext::new_inmemory();

        let dummy = Box::new(DummyModule::new());
        manager.add_native(dummy);

        manager.init_all(&mut registry, &mut ctx).unwrap();

        // Обращаемся к первому плагину (у нас только один)
        let res = manager.plugins[0].handle("test_cmd", b"data", &mut ctx);
        assert!(res.is_ok());
        assert_eq!(res.unwrap(), b"ok".to_vec());
    }

    /// Тест проверяет, что вызовы `unload_all` и `reload_all` работают без
    /// ошибок на базовом уровне.
    #[test]
    fn test_manager_unload_and_reload() {
        let mut manager = Manager::new();
        let mut ctx = DbContext::new_inmemory();

        let dummy = Box::new(DummyModule::new());
        manager.add_native(dummy);

        assert!(manager.unload_all(&mut ctx).is_ok());
        assert!(manager.reload_all(&mut ctx).is_ok());
    }
}
