use wasmtime::{Engine, Instance, Module as WasmModule, Store};

use crate::Module;

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
        // Здесь просто возвращаем статическое имя,
        // чтобы не требовать &mut для name()
        "WASM"
    }

    fn init(&mut self) -> Result<(), String> {
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
    ) -> Result<Vec<u8>, String> {
        // Если нужно брать мутабельный store, делаем handle(&mut self)
        // Здесь можете вызывать, например, функцию `handle` из WASM:
        // let func = self.instance
        //     .get_typed_func::<(i32,i32), i32>(&mut self.store, "handle")
        //     .map_err(|e| format!("WASM handle not found: {}", e))?;
        // ... и конвертировать command/data в память WASM ...
        Err("WASM handle not implemented".into())
    }
}
