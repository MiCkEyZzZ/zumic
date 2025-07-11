/// Определяем общий интерфейс для всех модулей
pub trait Module {
    /// Уникальное имя модуля
    fn name(&self) -> &str;
    /// Инициализация (вызывается один раз при загрузке)
    fn init(&mut self) -> Result<(), String>;
    /// Обработка входящего сообщения или команды
    fn handle(
        &mut self,
        command: &str,
        data: &[u8],
    ) -> Result<Vec<u8>, String>;

    /// Вызывается при загрузке модуля (жизненный цикл)
    fn on_load(&mut self) -> Result<(), String> {
        Ok(())
    }

    /// Вызывается при выгрузке модуля (жизненный цикл)
    fn on_unload(&mut self) -> Result<(), String> {
        Ok(())
    }

    /// Вызывается при перезагрузке модуля (жизненный цикл)
    fn on_reload(&mut self) -> Result<(), String> {
        Ok(())
    }
}
