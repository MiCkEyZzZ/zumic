use crate::{
    database::{ArcBytes, Value},
    error::StoreResult,
};

/// Трейт `Storage` задаёт интерфейс для бэкендов хранилищ "ключ-значение".
/// Все методы могут завершиться с ошибкой и возвращают результат типа `StoreResult`.
pub trait Storage {
    /// Устанавливает значение для заданного ключа.
    /// При этом перезаписываются все существующие значения.
    fn set(&mut self, key: ArcBytes, value: Value) -> StoreResult<()>;

    /// Возвращает значение для заданного ключа, или `None`, если ключ отсутствует.
    fn get(&mut self, key: ArcBytes) -> StoreResult<Option<Value>>;

    /// Удаляет ключ из хранилища.
    /// Возвращает `1`, если ключ был удалён, или `0`, если его не существовало.
    fn del(&self, key: ArcBytes) -> StoreResult<i64>;

    /// Устанавливает несколько пар "ключ-значение" в рамках единой операции.
    fn mset(&mut self, entries: Vec<(ArcBytes, Value)>) -> StoreResult<()>;

    /// Возвращает значения для списка ключей.
    /// Если значение для какого-либо ключа отсутствует, возвращается `None`.
    fn mget(&self, keys: &[ArcBytes]) -> StoreResult<Vec<Option<Value>>>;

    /// Переименовывает ключ в новое имя.
    /// Если исходный ключ отсутствует, возвращается ошибка.
    fn rename(&mut self, from: ArcBytes, to: ArcBytes) -> StoreResult<()>;

    /// Переименовывает ключ в новое имя только в том случае, если новый ключ ещё не существует.
    /// Возвращает `true`, если переименование произошло, и `false`, если целевой ключ уже существует.
    fn renamenx(&mut self, from: ArcBytes, to: ArcBytes) -> StoreResult<bool>;

    /// Очищает базу данных, удаляя все ключи.
    fn flushdb(&mut self) -> StoreResult<()>;
}
