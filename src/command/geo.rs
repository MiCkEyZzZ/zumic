use super::CommandExecute;
use crate::{GeoPoint, Sds, StorageEngine, StoreError, Value};

/// Комманда `GEOADD key lon lat member [lon lat member ...]`
///
/// Добавляет одну или несколько точек в гео-набор под ключом
/// `key`. Для каждого `member` вычисляет geohash и сохраняет
/// в множестве.
/// Возвращает общее число новых добавленных элементов.
#[derive(Debug)]
pub struct GeoAddCommand {
    pub key: String,
    /// Срез трёхкортежей (lon, lat, member)
    pub points: Vec<(f64, f64, String)>,
}

impl CommandExecute for GeoAddCommand {
    fn execute(
        &self,
        store: &mut StorageEngine,
    ) -> Result<Value, StoreError> {
        let key = Sds::from_str(&self.key);
        let mut added = 0;
        for (lon, lat, member) in &self.points {
            let member_sds = Sds::from_str(member);
            if store.geo_add(&key, *lon, *lat, &member_sds)? {
                added += 1;
            }
        }
        Ok(Value::Int(added))
    }
}

/// Команда `GEODIST key member1 member2 [unit]`
///
/// Вовзращает расстояние между `member1` и `member2`
/// в единицах `unit` (`m`, `km`, `mi`, `ft`), по умолчанию
/// метры.
/// Если один из членов не найден — возвращает `Null`.
#[derive(Debug)]
pub struct GetDistCommand {
    pub key: String,
    pub member1: String,
    pub member2: String,
    pub unit: Option<String>,
}

impl CommandExecute for GetDistCommand {
    fn execute(
        &self,
        store: &mut StorageEngine,
    ) -> Result<Value, StoreError> {
        let key = Sds::from_str(&self.key);
        let unit = self.unit.as_deref().unwrap_or("m");

        let member1 = Sds::from_str(&self.member1);
        let member2 = Sds::from_str(&self.member2);

        match store.geo_dist(&key, &member1, &member2, unit)? {
            Some(distance) => Ok(Value::Float(distance)),
            None => Ok(Value::Null),
        }
    }
}

/// Команда `GEOPOS key member [member ...]`
///
/// Вовзращает для каждого `member` координаты
/// `[lon, lat]`, или `Null`, если  не найден.
#[derive(Debug)]
pub struct GeoPosCommand {
    pub key: String,
    pub members: Vec<String>,
}

impl CommandExecute for GeoPosCommand {
    fn execute(
        &self,
        store: &mut StorageEngine,
    ) -> Result<Value, StoreError> {
        let key = Sds::from_str(&self.key);
        let mut result = Vec::with_capacity(self.members.len());
        for mem in &self.members {
            let mem_sds = Sds::from_str(mem);
            match store.geo_pos(&key, &mem_sds)? {
                Some(GeoPoint { lon, lat }) => {
                    result.push(Value::Array(vec![Value::Float(lon), Value::Float(lat)]));
                }
                None => {
                    result.push(Value::Null);
                }
            }
        }
        Ok(Value::Array(result))
    }
}

/// Команда `GEORADIUS key lon lat radius [uint]`
///
/// Ищет всех членов в радиусе `radius` вокруг точки `(lon, lat)`.
/// Возврвщает массив членов, опционально можно расширить до `[member, dist, lon, lat]`.
#[derive(Debug)]
pub struct GeoRadiusCommand {
    pub key: String,
    pub lon: f64,
    pub lat: f64,
    pub radius: f64,
    pub unit: Option<String>,
}

impl CommandExecute for GeoRadiusCommand {
    fn execute(
        &self,
        store: &mut StorageEngine,
    ) -> Result<Value, StoreError> {
        let key = Sds::from_str(&self.key);
        let unit = self.unit.as_deref().unwrap_or("m");

        let members = store.geo_radius(&key, self.lon, self.lat, self.radius, unit)?;

        // members: Vec<(String, f64, GeoPoint)>
        let result = members
            .into_iter()
            .map(|(member, dist, GeoPoint { lon, lat })| {
                Value::Array(vec![
                    Value::Str(Sds::from_str(&member)),
                    Value::Float(dist),
                    Value::Float(lon),
                    Value::Float(lat),
                ])
            })
            .collect();

        Ok(Value::Array(result))
    }
}

/// Команда `GEORADIUSBYMEMBER key member radius [unit]`
///
/// То же, что GEORADIUS, но определяется координатами
/// `member`.
#[derive(Debug)]
pub struct GeoRadiusByMemberCommand {
    pub key: String,
    pub member: String,
    pub radius: f64,
    pub unit: Option<String>,
}

impl CommandExecute for GeoRadiusByMemberCommand {
    fn execute(
        &self,
        store: &mut StorageEngine,
    ) -> Result<Value, StoreError> {
        let key = Sds::from_str(&self.key);
        let unit = self.unit.as_deref().unwrap_or("m");
        let member = Sds::from_str(&self.member);

        let pos = match store.geo_pos(&key, &member)? {
            Some(pos) => pos,
            None => return Ok(Value::Array(vec![])), // member не найден
        };

        let members = store.geo_radius(&key, pos.lon, pos.lat, self.radius, unit)?;

        let result = members
            .into_iter()
            .map(|(member, dist, GeoPoint { lon, lat })| {
                Value::Array(vec![
                    Value::Str(Sds::from_str(&member)),
                    Value::Float(dist),
                    Value::Float(lon),
                    Value::Float(lat),
                ])
            })
            .collect();

        Ok(Value::Array(result))
    }
}
