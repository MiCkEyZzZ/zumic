/// Представление точки с долготой и широтой.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GeoPoint {
    pub lon: f64,
    pub lat: f64,
}

/// Элемент гео-набора.
#[derive(Debug, Clone, PartialEq)]
pub struct GeoEntry {
    pub member: String,
    pub point: GeoPoint,
    pub score: u64, // 52-битный interleaved hash
}

/// Основная структура для хранения GEO-объектов.
#[derive(Debug, Default)]
pub struct GeoSet {
    pub entries: Vec<GeoEntry>,
}

impl GeoSet {
    /// Создаём пустой GeoSet.
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    /// Добавить или обновить точку.
    /// Высчитываем score = encode_geohash(lon, lat).
    pub fn add(
        &mut self,
        member: String,
        lon: f64,
        lat: f64,
    ) {
        let point = GeoPoint { lon, lat };
        let score = encode_geohash_bits(lon, lat);
        if let Some(e) = self.entries.iter_mut().find(|e| e.member == member) {
            e.point = point;
            e.score = score;
        } else {
            self.entries.push(GeoEntry {
                member,
                point,
                score,
            });
            // Поддерживаем сортировку по score для будущих быстрых range-запросов
            self.entries.sort_unstable_by_key(|e| e.score);
        }
    }

    /// Получить точку по имени.
    pub fn get(
        &self,
        member: &str,
    ) -> Option<GeoPoint> {
        self.entries
            .iter()
            .find(|e| e.member == member)
            .map(|e| e.point)
    }

    /// Расстояние между двумя точками в метрах.
    pub fn dist(
        &self,
        m1: &str,
        m2: &str,
    ) -> Option<f64> {
        let p1 = self.get(m1)?;
        let p2 = self.get(m2)?;
        Some(haversine_distance(p1, p2))
    }

    /// Простой радиусный поиск:
    /// возвращает всех членов, чьё score попадает в [center_score - Δ, center_score + Δ]
    /// и чей фактический distance ≤ radius_meters.
    pub fn radius(
        &self,
        lon: f64,
        lat: f64,
        radius_m: f64,
    ) -> Vec<(String, f64)> {
        let center = GeoPoint { lon, lat };
        self.entries
            .iter()
            .filter_map(|e| {
                let d = haversine_distance(center, e.point);
                if d <= radius_m {
                    Some((e.member.clone(), d))
                } else {
                    None
                }
            })
            .collect()
    }

    /// Возвращает итератор по всем элементам: `(member, GeoPoint)`.
    ///
    /// Используется, например, для сериализации всего множества.
    pub fn iter(&self) -> impl Iterator<Item = (&String, &GeoPoint)> {
        self.entries.iter().map(|e| (&e.member, &e.point))
    }
}

/// refine_bit - сужает диапазон и возвращает 0/1.
fn refine_bit(
    v: f64,
    min: &mut f64,
    max: &mut f64,
) -> u64 {
    let mid = (*min + *max) * 0.5;
    if v >= mid {
        *min = mid;
        1
    } else {
        *max = mid;
        0
    }
}

/// encode в 52-битный hash (interleaved lon/lat).
fn encode_geohash_bits(
    lon: f64,
    lat: f64,
) -> u64 {
    let mut lon_min = -180.0;
    let mut lon_max = 180.0;
    let mut lat_min = -90.0;
    let mut lat_max = 90.0;

    let mut hash = 0u64;
    for _ in 0..26 {
        hash = (hash << 1) | refine_bit(lon, &mut lon_min, &mut lon_max);
        hash = (hash << 1) | refine_bit(lat, &mut lat_min, &mut lat_max);
    }
    hash
}

/// decode из hash обратно в центре диапазона (для отладки).
#[allow(dead_code)]
fn decode_geohash_bits(hash: u64) -> (f64, f64) {
    let mut lon_min = -180.0;
    let mut lon_max = 180.0;
    let mut lat_min = -90.0;
    let mut lat_max = 90.0;

    for i in 0..26 {
        // Извлекаем биты справа налево
        let bit_idx = 2 * (25 - i);
        let bit_lon = (hash >> (bit_idx + 1)) & 1;
        let bit_lat = (hash >> bit_idx) & 1;

        let mid_lon = (lon_min + lon_max) * 0.5;
        if bit_lon == 1 {
            lon_min = mid_lon;
        } else {
            lon_max = mid_lon;
        }

        let mid_lat = (lat_min + lat_max) * 0.5;
        if bit_lat == 1 {
            lat_min = mid_lat;
        } else {
            lat_max = mid_lat;
        }
    }

    ((lon_min + lon_max) * 0.5, (lat_min + lat_max) * 0.5)
}

/// Формула Haversine - расстояния в метрах.
fn haversine_distance(
    p1: GeoPoint,
    p2: GeoPoint,
) -> f64 {
    let to_rad = std::f64::consts::PI / 180.0;
    let dlat = (p2.lat - p1.lat) * to_rad;
    let dlon = (p2.lon - p1.lon) * to_rad;
    let lat1 = p1.lat * to_rad;
    let lat2 = p2.lat * to_rad;

    let a = (dlat * 0.5).sin().powi(2) + lat1.cos() * lat2.cos() * (dlon * 0.5).sin().powi(2);
    let c = 2.0 * a.sqrt().atan2((1.0 - a).sqrt());
    6_371_000.0 * c
}

/// Простейшая грубая оценка Δ по score для данного радиуса.
/// Чтобы получить точный набор квадрантов, нужен алгоритм
/// соседних геокварталов, но для примера возьмём константе
/// большое Δ.
#[allow(dead_code)]
fn max_delta_for_radius(_radius_m: f64) -> u64 {
    // TODO: реализовать точный подбор по 52-битам, сейчас просто большой диапазон.
    1 << 20
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Проверяет, как функция `refine_bit` уточняет биты диапазона:
    /// - первый вызов делит диапазон [0.0, 100.0] пополам и возвращает 1,
    ///   обновляя min до 50.0 и max до 100.0;
    /// - второй вызов уточняет нижнюю половину и возвращает 0,
    ///   оставляя min = 50.0 и устанавливая max = 75.0.
    #[test]
    fn test_refine_bit() {
        let mut min = 0.0;
        let mut max = 100.0;
        let b = refine_bit(75.0, &mut min, &mut max);
        assert_eq!(b, 1);
        assert!((min - 50.0).abs() < 1e-9 && (max - 100.0).abs() < 1e-9);
        let b2 = refine_bit(60.0, &mut min, &mut max);
        assert_eq!(b2, 0);
        assert!((min - 50.0).abs() < 1e-9 && (max - 75.0).abs() < 1e-9);
    }

    /// Проверяет корректность кодирования и декодирования геохэша:
    /// - `encode_geohash_bits` преобразует долготу и широту в битовый хэш;
    /// - `decode_geohash_bits` восстанавливает координаты с погрешностью < 0.001.
    #[test]
    fn test_encode_decode_roundtrip() {
        let lon = 13.361389;
        let lat = 38.115556;
        let hash = encode_geohash_bits(lon, lat);
        let (lon2, lat2) = decode_geohash_bits(hash);
        assert!((lon - lon2).abs() < 1e-3);
        assert!((lat - lat2).abs() < 1e-3);
    }

    /// Проверяет вычисление расстояния по формуле гаверсина:
    /// расстояние между (0°,0°) и (0°,1°) должно быть примерно 111 195 м ± 100 м.
    #[test]
    fn test_haversine_distance() {
        let p1 = GeoPoint { lon: 0.0, lat: 0.0 };
        let p2 = GeoPoint { lon: 0.0, lat: 1.0 };
        let d = haversine_distance(p1, p2);
        assert!((d - 111_195.0).abs() < 100.0);
    }

    /// Проверяет базовые методы GeoSet:
    /// - добавление точек через `add`;
    /// - получение точек через `get` по ключу;
    /// - отсутствие точки при запросе несуществующего ключа.
    #[test]
    fn test_add_get() {
        let mut gs = GeoSet::new();
        gs.add("A".into(), 10.0, 20.0);
        gs.add("B".into(), -5.5, 42.1);
        assert_eq!(
            gs.get("A").unwrap(),
            GeoPoint {
                lon: 10.0,
                lat: 20.0,
            }
        );
        assert_eq!(
            gs.get("B").unwrap(),
            GeoPoint {
                lon: -5.5,
                lat: 42.1,
            }
        );
        assert!(gs.get("C").is_none());
    }

    /// Проверяет метод `dist` у GeoSet:
    /// - правильное вычисление расстояния между существующими точками;
    /// - возвращение `None` для несуществующего ключа.
    #[test]
    fn test_dist_method() {
        let mut gs = GeoSet::new();
        gs.add("X".into(), 0.0, 0.0);
        gs.add("Y".into(), 0.0, 1.0);
        let d = gs.dist("X", "Y").unwrap();
        assert!((d - 111_195.0).abs() < 100.0);
        assert!(gs.dist("X", "Z").is_none());
    }

    /// Проверяет метод `radius` у GeoSet:
    /// - возвращает только те точки, что находятся в пределах заданного радиуса;
    /// - корректно обрабатывает разные радиусы (20 000 м и 200 000 м).
    #[test]
    fn test_radius() {
        let mut gs = GeoSet::new();
        gs.add("near".into(), 0.1, 0.0);
        gs.add("far".into(), 1.0, 0.0);
        let res = gs.radius(0.0, 0.0, 20_000.0);
        assert_eq!(res.len(), 1);
        assert_eq!(res[0].0, "near");
        assert!(res[0].1 < 20_000.0);
        let res2 = gs.radius(0.0, 0.0, 200_000.0);
        let members: Vec<_> = res2.into_iter().map(|(m, _)| m).collect();
        assert!(members.contains(&"near".to_string()));
        assert!(members.contains(&"far".to_string()));
    }

    /// Проверяет, что декодированные координаты попадают в исходный диапазон:
    /// разница координат меньше ~0.001 градуса.
    #[test]
    fn test_decode_in_bounds() {
        let lon = 77.1;
        let lat = 55.7;
        let hash = encode_geohash_bits(lon, lat);
        let (d_lon, d_lat) = decode_geohash_bits(hash);
        // Должно попадать в последний интервал: разница < (360/2^26) ≈ 5e-6
        assert!((d_lon - lon).abs() < 1e-3);
        assert!((d_lat - lat).abs() < 1e-3);
    }
}
