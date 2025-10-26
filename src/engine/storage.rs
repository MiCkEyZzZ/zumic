use async_trait::async_trait;

use crate::{GeoPoint, Sds, StoreResult, Value};

/// Трейт `Storage` определяет интерфейс для реализаций хранилища
/// ключ-значение.
/// Все методы могут возвращать ошибку и используют `StoreResult`
/// как тип результата.
pub trait Storage {
    /// Устанавливает значение по заданному ключу.
    /// Если значение по ключу уже существует, оно будет перезаписано.
    fn set(
        &self,
        key: &Sds,
        value: Value,
    ) -> StoreResult<()>;

    /// Возвращает значение по заданному ключу, либо `None`, если ключ не
    /// существует.
    fn get(
        &self,
        key: &Sds,
    ) -> StoreResult<Option<Value>>;

    /// Удаляет ключ из хранилища.
    /// Возвращает `true`, если ключ был удалён, или `false`, если его не
    /// существовало.
    fn del(
        &self,
        key: &Sds,
    ) -> StoreResult<bool>;

    /// Устанавливает несколько пар ключ-значение за одну операцию.
    fn mset(
        &self,
        entries: Vec<(&Sds, Value)>,
    ) -> StoreResult<()>;

    /// Возвращает значения для списка ключей.
    /// Если для какого-либо ключа значение отсутствует, на его месте будет
    /// `None`.
    fn mget(
        &self,
        keys: &[&Sds],
    ) -> StoreResult<Vec<Option<Value>>>;

    /// Переименовывает ключ.
    /// Возвращает ошибку, если исходный ключ не существует.
    fn rename(
        &self,
        from: &Sds,
        to: &Sds,
    ) -> StoreResult<()>;

    /// Переименовывает ключ только в том случае, если новый ключ ещё не
    /// существует. Возвращает `true`, если переименование удалось, `false`
    /// — если целевой ключ уже существует.
    fn renamenx(
        &self,
        from: &Sds,
        to: &Sds,
    ) -> StoreResult<bool>;

    /// Очищает базу данных, удаляя все ключи.
    fn flushdb(&self) -> StoreResult<()>;

    /// Очищает базу данных, удаляя все ключи.
    fn dbsize(&self) -> StoreResult<usize>;

    /// Сохраняет состояние базы данных на диск (для персистентных хранилищ).
    /// Для in-memory хранилищ может быть пустой реализацией или возвращать
    /// ошибку.
    fn save(&self) -> StoreResult<()>;

    /// Добавляет точку в гео-множество.
    /// Возвращает `Ok(true)`, если member новый, иначе `Ok(false)`.
    fn geo_add(
        &self,
        key: &Sds,
        lon: f64,
        lat: f64,
        member: &Sds,
    ) -> StoreResult<bool>;

    /// Расстояние между двумя членами множества.
    /// Возвращает `Ok(Some(d))` если оба есть, иначе `Ok(None)`.
    fn geo_dist(
        &self,
        key: &Sds,
        member1: &Sds,
        member2: &Sds,
        unit: &str,
    ) -> StoreResult<Option<f64>>;

    /// Возвращает координаты члена, либо `None`.
    fn geo_pos(
        &self,
        key: &Sds,
        member: &Sds,
    ) -> StoreResult<Option<GeoPoint>>;

    /// Ищет по радиусу от произвольной точки.
    /// Возвращает вектор `(member, distance, GeoPoint)`.
    fn geo_radius(
        &self,
        key: &Sds,
        lon: f64,
        lat: f64,
        radius: f64,
        unit: &str,
    ) -> StoreResult<Vec<(String, f64, GeoPoint)>>;

    /// Ищет по радиусу от координат существующего member.
    fn geo_radius_by_member(
        &self,
        key: &Sds,
        member: &Sds,
        radius: f64,
        unit: &str,
    ) -> StoreResult<Vec<(String, f64, GeoPoint)>>;

    fn sadd(
        &self,
        key: &Sds,
        members: &[Sds],
    ) -> StoreResult<usize>;

    fn smembers(
        &self,
        key: &Sds,
    ) -> StoreResult<Vec<Sds>>;

    fn scard(
        &self,
        key: &Sds,
    ) -> StoreResult<usize>;

    fn sismember(
        &self,
        key: &Sds,
        member: &Sds,
    ) -> StoreResult<bool>;

    fn srem(
        &self,
        key: &Sds,
        members: &[Sds],
    ) -> StoreResult<usize>;

    fn srandmember(
        &self,
        key: &Sds,
        count: isize,
    ) -> StoreResult<Vec<Sds>>;

    fn spop(
        &self,
        key: &Sds,
        count: isize,
    ) -> StoreResult<Vec<Sds>>;
}

/// Трейт `AsyncStorage` определяет интерфейс для реализаций хранилища
/// ключ-значение.
/// Все методы могут возвращать ошибку и используют `StoreResult`
/// как тип результата.
#[async_trait]
pub trait AsyncStorage: Send + Sync + 'static {
    async fn set(
        &self,
        key: &Sds,
        value: Value,
    ) -> StoreResult<()>;

    async fn get(
        &self,
        key: &Sds,
    ) -> StoreResult<Option<Value>>;

    async fn del(
        &self,
        key: &Sds,
    ) -> StoreResult<bool>;

    async fn mset(
        &self,
        entries: Vec<(Sds, Value)>,
    ) -> StoreResult<()>;

    async fn mget(
        &self,
        keys: Vec<Sds>,
    ) -> StoreResult<Vec<Option<Value>>>;

    async fn rename(
        &self,
        from: &Sds,
        to: &Sds,
    ) -> StoreResult<()>;

    async fn renamenx(
        &self,
        from: &Sds,
        to: &Sds,
    ) -> StoreResult<bool>;

    async fn flushdb(&self) -> StoreResult<()>;

    async fn dbsize(&self) -> StoreResult<usize>;

    async fn save(&self) -> StoreResult<()>;

    // GEO методы
    async fn geo_add(
        &self,
        key: &Sds,
        lon: f64,
        lat: f64,
        member: &Sds,
    ) -> StoreResult<bool>;

    async fn geo_dist(
        &self,
        key: &Sds,
        member1: &Sds,
        member2: &Sds,
        unit: &str,
    ) -> StoreResult<Option<f64>>;

    async fn geo_pos(
        &self,
        key: &Sds,
        member: &Sds,
    ) -> StoreResult<Option<GeoPoint>>;

    async fn geo_radius(
        &self,
        key: &Sds,
        lon: f64,
        lat: f64,
        radius: f64,
        unit: &str,
    ) -> StoreResult<Vec<(String, f64, GeoPoint)>>;

    async fn geo_radius_by_member(
        &self,
        key: &Sds,
        member: &Sds,
        radius: f64,
        unit: &str,
    ) -> StoreResult<Vec<(String, f64, GeoPoint)>>;

    async fn sadd(
        &self,
        key: &Sds,
        members: Vec<Sds>,
    ) -> StoreResult<usize>;
    async fn smembers(
        &self,
        key: &Sds,
    ) -> StoreResult<Vec<Sds>>;
    async fn scard(
        &self,
        key: &Sds,
    ) -> StoreResult<usize>;
    async fn sismember(
        &self,
        key: &Sds,
        member: &Sds,
    ) -> StoreResult<bool>;
    async fn srem(
        &self,
        key: &Sds,
        members: Vec<Sds>,
    ) -> StoreResult<usize>;
    async fn srandmember(
        &self,
        key: &Sds,
        count: isize,
    ) -> StoreResult<Vec<Sds>>;
    async fn spop(
        &self,
        key: &Sds,
        count: isize,
    ) -> StoreResult<Vec<Sds>>;
}
