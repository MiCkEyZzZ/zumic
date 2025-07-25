//! Команды для работы с географическими данными (GEO*) в Zumic.
//!
//! Реализует команды GEOADD, GEODIST, GEOPOS, GEORADIUS, GEORADIUSBYMEMBER для
//! хранения, поиска и вычисления расстояний между точками.
//! Каждая команда реализует трейт [`CommandExecute`].

use super::CommandExecute;
use crate::{GeoPoint, Sds, StorageEngine, StoreError, Value};

/// Команда GEOADD — добавляет одну или несколько точек в гео-набор.
///
/// Формат: `GEOADD key lon lat member [lon lat member ...]`
///
/// # Поля
/// * `key` — ключ гео-набора.
/// * `points` — вектор кортежей (долгота, широта, имя точки).
///
/// # Возвращает
/// Количество новых добавленных элементов.
#[derive(Debug)]
pub struct GeoAddCommand {
    pub key: String,
    /// Срез трёх кортежей (lon, lat, member)
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

/// Команда GEODIST — вычисляет расстояние между двумя точками.
///
/// Формат: `GEODIST key member1 member2 [unit]`
///
/// # Поля
/// * `key` — ключ гео-набора.
/// * `member1` — имя первой точки.
/// * `member2` — имя второй точки.
/// * `unit` — единицы измерения (`m`, `km`, `mi`, `ft`), по умолчанию метры.
///
/// # Возвращает
/// Расстояние между точками или `Null`, если одна из точек не найдена.
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

/// Команда GEOPOS — возвращает координаты для указанных точек.
///
/// Формат: `GEOPOS key member [member ...]`
///
/// # Поля
/// * `key` — ключ гео-набора.
/// * `members` — список имён точек.
///
/// # Возвращает
/// Для каждого member — координаты `[lon, lat]` или `Null`, если не найден.
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

/// Команда GEORADIUS — ищет точки в радиусе вокруг координат.
///
/// Формат: `GEORADIUS key lon lat radius [unit]`
///
/// # Поля
/// * `key` — ключ гео-набора.
/// * `lon` — долгота.
/// * `lat` — широта.
/// * `radius` — радиус поиска.
/// * `unit` — единицы измерения (`m`, `km`, `mi`, `ft`), по умолчанию метры.
///
/// # Возвращает
/// Массив найденных точек (может включать координаты и расстояния).
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

/// Команда GEORADIUSBYMEMBER — ищет точки в радиусе вокруг другой точки.
///
/// Формат: `GEORADIUSBYMEMBER key member radius [unit]`
///
/// # Поля
/// * `key` — ключ гео-набора.
/// * `member` — имя точки-центра.
/// * `radius` — радиус поиска.
/// * `unit` — единицы измерения (`m`, `km`, `mi`, `ft`), по умолчанию метры.
///
/// # Возвращает
/// Массив найденных точек (может включать координаты и расстояния).
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

#[cfg(test)]
mod tests {
    use crate::InMemoryStore;

    use super::*;

    /// Вспомогалка: создаём память и заполняем точками.
    fn setup_store() -> StorageEngine {
        let engine = StorageEngine::Memory(InMemoryStore::new());
        let key = "places";
        engine
            .geo_add(&Sds::from_str(key), 0.0, 0.0, &Sds::from_str("origin"))
            .unwrap();
        engine
            .geo_add(&Sds::from_str(key), 0.001, 0.0, &Sds::from_str("east"))
            .unwrap();
        engine
            .geo_add(&Sds::from_str(key), 0.0, 0.001, &Sds::from_str("north"))
            .unwrap();
        engine
    }

    /// Тест проверяет, что команда `GEOADD`:
    /// - добавляет новые точки и возвращает число добавленных
    /// - при повторной вставке тех же точек возвращает 0
    #[test]
    fn test_geoadd_command() {
        let mut engine = StorageEngine::Memory(InMemoryStore::new());
        let cmd = GeoAddCommand {
            key: "cities".into(),
            points: vec![
                (2.3522, 48.8566, "paris".into()),
                (13.4050, 52.5200, "berlin".into()),
            ],
        };
        let res = cmd.execute(&mut engine).unwrap();
        assert_eq!(res, Value::Int(2));
        // повторная вставка ни к чему не добавит
        let res2 = cmd.execute(&mut engine).unwrap();
        assert_eq!(res2, Value::Int(0));
    }

    /// Тест проверяет, что команда `GEODIST`:
    /// - правильно считает расстояние между двумя точками
    /// - возвращает `Null`, если один из участников не найден
    #[test]
    fn test_geodist_command() {
        let mut engine = setup_store();
        let cmd = GetDistCommand {
            key: "places".into(),
            member1: "origin".into(),
            member2: "east".into(),
            unit: Some("m".into()),
        };
        let res = cmd.execute(&mut engine).unwrap();
        if let Value::Float(d) = res {
            // около 111 метров
            assert!((d - 111.0).abs() < 10.0);
        } else {
            panic!("Expected Float");
        }

        // непонятный член -> Null
        let cmd2 = GetDistCommand {
            key: "places".into(),
            member1: "origin".into(),
            member2: "missing".into(),
            unit: None,
        };
        assert_eq!(cmd2.execute(&mut engine).unwrap(), Value::Null);
    }

    /// Тест проверяет, что команда `GEOPOS`:
    /// - возвращает координаты существующего участника
    /// - возвращает `Null`, если участник не найден
    #[test]
    fn test_geopos_command() {
        let mut engine = setup_store();
        let cmd = GeoPosCommand {
            key: "places".into(),
            members: vec!["origin".into(), "missing".into()],
        };
        let res = cmd.execute(&mut engine).unwrap();
        if let Value::Array(arr) = res {
            // первый элемент — [0.0,0.0], второй — Null
            assert_eq!(
                arr[0],
                Value::Array(vec![Value::Float(0.0), Value::Float(0.0)])
            );
            assert_eq!(arr[1], Value::Null);
        } else {
            panic!("Expected Array");
        }
    }

    /// Тест проверяет, что команда `GEORADIUS`:
    /// - возвращает всех участников в радиусе 200м от заданной точки
    /// - включены origin, east, north
    #[test]
    fn test_georadius_command() {
        let mut engine = setup_store();
        let cmd = GeoRadiusCommand {
            key: "places".into(),
            lon: 0.0,
            lat: 0.0,
            radius: 200.0, // метров
            unit: Some("m".into()),
        };
        let res = cmd.execute(&mut engine).unwrap();
        if let Value::Array(arr) = res {
            // origin (0m) и east (~111m) и north (~111m) войдут
            let members: Vec<String> = arr
                .into_iter()
                .map(|item| {
                    if let Value::Array(inner) = item {
                        if let Value::Str(s) = &inner[0] {
                            s.to_string()
                        } else {
                            panic!()
                        }
                    } else {
                        panic!()
                    }
                })
                .collect();
            assert!(members.contains(&"origin".to_string()));
            assert!(members.contains(&"east".to_string()));
            assert!(members.contains(&"north".to_string()));
        } else {
            panic!("Expected Array");
        }
    }

    /// Тест проверяет, что команда `GEORADIUSBYMEMBER`:
    /// - использует координаты указанного участника (origin)
    /// - возвращает участников в радиусе 200м от него
    #[test]
    fn test_georadiusbymember_command() {
        let mut engine = setup_store();
        let cmd = GeoRadiusByMemberCommand {
            key: "places".into(),
            member: "origin".into(),
            radius: 200.0,
            unit: None, // по умолчанию метры
        };
        let res = cmd.execute(&mut engine).unwrap();
        if let Value::Array(arr) = res {
            let members: Vec<String> = arr
                .into_iter()
                .map(|item| {
                    if let Value::Array(inner) = item {
                        if let Value::Str(s) = &inner[0] {
                            s.to_string()
                        } else {
                            panic!()
                        }
                    } else {
                        panic!()
                    }
                })
                .collect();
            assert!(members.contains(&"origin".to_string()));
            assert!(members.contains(&"east".to_string()));
            assert!(members.contains(&"north".to_string()));
        } else {
            panic!("Expected Array");
        }
    }
}
