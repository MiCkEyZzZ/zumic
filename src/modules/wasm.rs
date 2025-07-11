use wasmtime::{Engine, Instance, Module as WasmModule, Store};

use crate::{command_registry::CommandRegistry, db_context::DbContext, Module};

/// Обёртка для WASM-модулей, реализующих тот же интерфейс.
pub struct WasmPlugin {
    instance: Instance,
    store: Store<()>,
}

impl WasmPlugin {
    pub fn load(
        path: &str,
        engine: &Engine,
    ) -> Result<Self, String> {
        let module =
            WasmModule::from_file(engine, path).map_err(|e| format!("WASM load error: {e}"))?;
        let mut store = Store::new(engine, ());
        let instance = Instance::new(&mut store, &module, &[])
            .map_err(|e| format!("WASM instantiate error: {e}"))?;
        Ok(WasmPlugin { instance, store })
    }
}

impl Module for WasmPlugin {
    fn name(&self) -> &str {
        "WASM"
    }

    fn init(
        &mut self,
        _registry: &mut CommandRegistry,
        _ctx: &mut DbContext,
    ) -> Result<(), String> {
        if let Ok(init_fn) = self
            .instance
            .get_typed_func::<(), ()>(&mut self.store, "init")
        {
            init_fn
                .call(&mut self.store, ())
                .map_err(|e| format!("WASM init failed: {e}"))?;
        }
        Ok(())
    }

    fn handle(
        &mut self,
        _command: &str,
        _data: &[u8],
        _ctx: &mut DbContext,
    ) -> Result<Vec<u8>, String> {
        Err("WASM handle not implemented".into())
    }

    fn on_load(
        &mut self,
        _registry: &mut CommandRegistry,
        _ctx: &mut DbContext,
    ) -> Result<(), String> {
        if let Ok(f) = self
            .instance
            .get_typed_func::<(), ()>(&mut self.store, "on_load")
        {
            f.call(&mut self.store, ()).ok();
        }
        Ok(())
    }

    fn on_unload(
        &mut self,
        _ctx: &mut DbContext,
    ) -> Result<(), String> {
        if let Ok(f) = self
            .instance
            .get_typed_func::<(), ()>(&mut self.store, "on_unload")
        {
            f.call(&mut self.store, ()).ok();
        }
        Ok(())
    }

    fn on_reload(
        &mut self,
        _ctx: &mut DbContext,
    ) -> Result<(), String> {
        if let Ok(f) = self
            .instance
            .get_typed_func::<(), ()>(&mut self.store, "on_reload")
        {
            f.call(&mut self.store, ()).ok();
        }
        Ok(())
    }
}
