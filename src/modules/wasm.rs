use std::{
    collections::HashSet,
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};

use wasmtime::{Engine, Instance, Memory, Module as WasmModule, Store, TypedFunc};

use crate::{command_registry::CommandRegistry, db_context::DbContext, Module};

pub struct WasmPlugin {
    instance: Instance,
    store: Store<()>,
    engine: Engine,
    allocated: HashSet<i32>,
}

impl WasmPlugin {
    /// Загружает и инициализирует WASM-модуль.
    pub fn load(
        path: &str,
        engine: &Engine,
    ) -> Result<Self, String> {
        let module =
            WasmModule::from_file(engine, path).map_err(|e| format!("WASM load error: {e}"))?;
        let mut store = Store::new(engine, ());
        store.set_epoch_deadline(1); // прерывание при первой проверке
        let instance = Instance::new(&mut store, &module, &[])
            .map_err(|e| format!("WASM instantiate error: {e}"))?;
        Ok(Self {
            instance,
            store,
            engine: engine.clone(),
            allocated: HashSet::new(),
        })
    }

    /// Получает экспортированную память WASM-модуля.
    fn memory(&mut self) -> Result<Memory, String> {
        self.instance
            .get_memory(&mut self.store, "memory")
            .ok_or_else(|| "WASM memory not found".into())
    }

    /// Вызывает `malloc` внутри WASM, возвращает указатель.
    fn malloc(
        &mut self,
        size: i32,
    ) -> Result<i32, String> {
        let f: TypedFunc<i32, i32> = self
            .instance
            .get_typed_func(&mut self.store, "malloc")
            .map_err(|_| "WASM malloc not found".to_string())?;
        f.call(&mut self.store, size)
            .map_err(|e| format!("malloc failed: {e}"))
    }

    /// Вызывает `free` внутри WASM, предотвращает двойное освобождение.
    fn free(
        &mut self,
        ptr: i32,
        len: i32,
    ) -> Result<(), String> {
        let f: TypedFunc<(i32, i32), ()> = self
            .instance
            .get_typed_func(&mut self.store, "free")
            .map_err(|_| "WASM free not found".to_string())?;
        if !self.allocated.remove(&ptr) {
            eprintln!("Warning: double free attempt at pointer {ptr}");
            return Ok(());
        }
        f.call(&mut self.store, (ptr, len))
            .map_err(|e| format!("free failed: {e}"))
    }

    /// Записывает данные в память WASM-модуля. Возвращает (ptr, len).
    fn write_raw(
        &mut self,
        data: &[u8],
    ) -> Result<(i32, i32), String> {
        if data.len() > 1 << 20 {
            return Err("Input too large".into());
        }
        let mem = self.memory()?;
        let ptr = self.malloc(data.len() as i32)?;
        let buf = mem.data_mut(&mut self.store);
        let end = ptr as usize + data.len();
        if end > buf.len() {
            return Err("WASM memory overflow".into());
        }
        buf[ptr as usize..end].copy_from_slice(data);
        self.allocated.insert(ptr);
        Ok((ptr, data.len() as i32))
    }

    /// Читает данные из памяти WASM-модуля.
    fn read_raw(
        &mut self,
        ptr: i32,
        len: i32,
    ) -> Result<Vec<u8>, String> {
        let mem = self.memory()?;
        let buf = mem.data(&self.store);
        let end = ptr as usize + len as usize;
        if end > buf.len() {
            return Err("WASM memory overflow".into());
        }
        Ok(buf[ptr as usize..end].to_vec())
    }

    /// Вызывает WASM-функцию с прерыванием по таймауту.
    fn call_with_timeout(
        &mut self,
        func: TypedFunc<(i32, i32), i64>,
        args: (i32, i32),
        timeout: Duration,
    ) -> Result<i64, String> {
        let done = Arc::new(Mutex::new(false));
        let done_clone = Arc::clone(&done);
        let engine = self.engine.clone();

        let handle = thread::spawn(move || {
            thread::sleep(timeout);
            if !*done_clone.lock().unwrap() {
                engine.increment_epoch();
            }
        });

        let result = func
            .call(&mut self.store, args)
            .map_err(|e| format!("WASM handle call failed: {e}"))?;

        *done.lock().unwrap() = true;
        let _ = handle.join();
        Ok(result)
    }
}

impl Module for WasmPlugin {
    fn name(&self) -> &str {
        "WASM"
    }

    /// Инициализация модуля (вызывает опциональный экспорт `init`).
    fn init(
        &mut self,
        _registry: &mut CommandRegistry,
        _ctx: &mut DbContext,
    ) -> Result<(), String> {
        if let Ok(f) = self
            .instance
            .get_typed_func::<(), ()>(&mut self.store, "init")
        {
            f.call(&mut self.store, ())
                .map_err(|e| format!("WASM init failed: {e}"))?;
        }
        Ok(())
    }

    /// Обрабатывает команду, передавая её в `handle`.
    fn handle(
        &mut self,
        command: &str,
        data: &[u8],
        _ctx: &mut DbContext,
    ) -> Result<Vec<u8>, String> {
        let mut input = Vec::new();
        serde_cbor::to_writer(&mut input, &(command, data)).map_err(|e| e.to_string())?;

        let (in_ptr, in_len) = self.write_raw(&input)?;
        let f: TypedFunc<(i32, i32), i64> = self
            .instance
            .get_typed_func(&mut self.store, "handle")
            .map_err(|_| "WASM handle not found".to_string())?;
        let ret = self.call_with_timeout(f, (in_ptr, in_len), Duration::from_millis(100))?;

        let out_ptr = (ret as u32) as i32;
        let out_len = ((ret >> 32) as u32) as i32;
        let output = self.read_raw(out_ptr, out_len)?;

        self.free(in_ptr, in_len)?;
        self.free(out_ptr, out_len)?;
        Ok(output)
    }

    /// Вызывается при загрузке модуля (опциональный экспорт `on_load`).
    fn on_load(
        &mut self,
        _reg: &mut CommandRegistry,
        _ctx: &mut DbContext,
    ) -> Result<(), String> {
        if let Ok(f) = self
            .instance
            .get_typed_func::<(), ()>(&mut self.store, "on_load")
        {
            let _ = f.call(&mut self.store, ());
        }
        Ok(())
    }

    /// Вызывается при выгрузке модуля (опциональный экспорт `on_unload`).
    fn on_unload(
        &mut self,
        _ctx: &mut DbContext,
    ) -> Result<(), String> {
        if let Ok(f) = self
            .instance
            .get_typed_func::<(), ()>(&mut self.store, "on_unload")
        {
            let _ = f.call(&mut self.store, ());
        }
        Ok(())
    }

    /// Вызывается при перезагрузке модуля (опциональный экспорт `on_reload`).
    fn on_reload(
        &mut self,
        _ctx: &mut DbContext,
    ) -> Result<(), String> {
        if let Ok(f) = self
            .instance
            .get_typed_func::<(), ()>(&mut self.store, "on_reload")
        {
            let _ = f.call(&mut self.store, ());
        }
        Ok(())
    }
}
