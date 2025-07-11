use crate::{command_registry::CommandRegistry, db_context::DbContext};

/// Определяем общий интерфейс для всех модулей
pub trait Module {
    /// Уникальное имя модуля
    fn name(&self) -> &str;
    /// Инициализация (вызывается один раз при загрузке)
    fn init(
        &mut self,
        registry: &mut CommandRegistry,
        ctx: &mut DbContext,
    ) -> Result<(), String>;
    /// Обработка входящего сообщения или команды
    fn handle(
        &mut self,
        command: &str,
        data: &[u8],
        ctx: &mut DbContext,
    ) -> Result<Vec<u8>, String>;

    /// Жизненный цикл: вызывается при загрузке модуля
    fn on_load(
        &mut self,
        _registry: &mut CommandRegistry,
        _ctx: &mut DbContext,
    ) -> Result<(), String> {
        Ok(())
    }

    /// Жизненный цикл: вызывается при выгрузке модуля
    fn on_unload(
        &mut self,
        _ctx: &mut DbContext,
    ) -> Result<(), String> {
        Ok(())
    }

    /// Жизненный цикл: вызывается при перезагрузке модуля
    fn on_reload(
        &mut self,
        _ctx: &mut DbContext,
    ) -> Result<(), String> {
        Ok(())
    }
}
