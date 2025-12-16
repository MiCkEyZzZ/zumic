use std::collections::HashMap;

use crate::database::{BoundingBox, RTree, TreeStats};

/// Географическая точка (долгота и широта).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GeoPoint {
    pub lon: f64,
    pub lat: f64,
}

/// Элемент гео-набора: имя, координаты, geohash.
#[derive(Debug, Clone, PartialEq)]
pub struct GeoEntry {
    pub member: String,
    pub point: GeoPoint,
    pub score: u64, // 52-битный interleaved hash
}

/// Множество географических точек с R-tree индексом для быстрого поиска.
#[derive(Debug)]
pub struct GeoSet {
    /// R-tree spatial index для эффективных queries
    rtree: RTree,
    /// HashMap для быстрого поиска по member name
    member_index: HashMap<String, GeoPoint>,
    /// Флаг для отложенной пересборки индекса
    needs_rebuild: bool,
}

////////////////////////////////////////////////////////////////////////////////
// Собственные методы
////////////////////////////////////////////////////////////////////////////////

impl GeoSet {
    /// Создаёт пустое гео-множество.
    pub fn new() -> Self {
        Self {
            rtree: RTree::new(),
            member_index: HashMap::new(),
            needs_rebuild: false,
        }
    }

    /// Создаёт GeoSet из вектора записей с bulk loading.
    /// Эффективнее последовательных add для больших datasets.
    pub fn from_entries(entries: Vec<GeoEntry>) -> Self {
        let mut member_index = HashMap::with_capacity(entries.len());
        for entry in &entries {
            member_index.insert(entry.member.clone(), entry.point);
        }

        let rtree = RTree::bulk_load(entries);

        Self {
            rtree,
            member_index,
            needs_rebuild: false,
        }
    }

    /// Добавляет или обновляет точку по имени.
    ///
    /// # Аргументы
    /// * `member` — имя точки.
    /// * `lon` — долгота (-180 до 180).
    /// * `lat` — широта (-90 до 90).
    pub fn add(
        &mut self,
        member: String,
        lon: f64,
        lat: f64,
    ) {
        // Валидация координат
        if !Self::validate_coords(lon, lat) {
            return;
        }

        let point = GeoPoint { lon, lat };
        let score = encode_geohash_bits(lon, lat);

        // Обновление member_index
        if let Some(old_point) = self.member_index.insert(member.clone(), point) {
            // Точка уже существовала - нужна пересборка для удаления старой
            if old_point != point {
                self.needs_rebuild = true;
            }
        }

        // Вставка в R-tree
        self.rtree.insert(GeoEntry {
            member,
            point,
            score,
        });
    }

    /// Валидирует координаты.
    fn validate_coords(
        lon: f64,
        lat: f64,
    ) -> bool {
        (-180.0..=180.0).contains(&lon) && (-90.0..=90.0).contains(&lat)
    }

    /// Получает координаты точки по имени.
    pub fn get(
        &self,
        member: &str,
    ) -> Option<GeoPoint> {
        self.member_index.get(member).copied()
    }

    /// Вычисляет расстояние между двумя точками по их именам (в метрах).
    pub fn dist(
        &self,
        m1: &str,
        m2: &str,
    ) -> Option<f64> {
        let p1 = self.get(m1)?;
        let p2 = self.get(m2)?;
        Some(haversine_distance(p1, p2))
    }

    /// Возвращает всех членов в радиусе `radius_m` метров от точки (`lon`,
    /// `lat`). Использует R-tree для эффективного поиска.
    pub fn radius(
        &self,
        lon: f64,
        lat: f64,
        radius_m: f64,
    ) -> Vec<(String, f64)> {
        if !Self::validate_coords(lon, lat) {
            return Vec::new();
        }

        let center = GeoPoint { lon, lat };

        // Вычисляем bounding box для approximate поиска
        let bbox = Self::radius_to_bbox(center, radius_m);

        // Range query через R-tree
        let candidates = self.rtree.range_query(&bbox);

        // Точная фильтрация с haversine
        candidates
            .into_iter()
            .filter_map(|entry| {
                let dist = haversine_distance(center, entry.point);
                if dist <= radius_m {
                    Some((entry.member, dist))
                } else {
                    None
                }
            })
            .collect()
    }

    /// Поиск k ближайших соседей к точке.
    pub fn nearest(
        &self,
        lon: f64,
        lat: f64,
        k: usize,
    ) -> Vec<(String, f64)> {
        if !Self::validate_coords(lon, lat) || k == 0 {
            return Vec::new();
        }

        let point = GeoPoint { lon, lat };
        self.rtree
            .knn(point, k)
            .into_iter()
            .map(|(entry, dist)| (entry.member, dist))
            .collect()
    }

    /// Поиск k ближайших соседей к существующему члену.
    pub fn nearest_by_member(
        &self,
        member: &str,
        k: usize,
    ) -> Option<Vec<(String, f64)>> {
        let point = self.get(member)?;

        let mut results = self.nearest(point.lon, point.lat, k + 1);

        // Удаляем сам member из результатов
        results.retain(|(m, _)| m != member);
        results.truncate(k);

        Some(results)
    }

    /// Преобразует радиус в метрах в bounding box (приблизительно).
    fn radius_to_bbox(
        center: GeoPoint,
        radius_m: f64,
    ) -> BoundingBox {
        // Примерное преобразование: 1 градус ≈ 111km на экваторе
        let lat_delta = radius_m / 111_000.0;
        let lon_delta = radius_m / (111_000.0 * center.lat.to_radians().cos().abs().max(0.01));

        BoundingBox::new(
            (center.lon - lon_delta).max(-180.0),
            (center.lon + lon_delta).min(180.0),
            (center.lat - lat_delta).max(-90.0),
            (center.lat + lat_delta).min(90.0),
        )
    }

    /// Пересобирает R-tree (если были обновления существующих точек).
    pub fn rebuild_index(&mut self) {
        if !self.needs_rebuild {
            return;
        }

        let entries: Vec<GeoEntry> = self
            .member_index
            .iter()
            .map(|(member, point)| GeoEntry {
                member: member.clone(),
                point: *point,
                score: encode_geohash_bits(point.lon, point.lat),
            })
            .collect();

        self.rtree = RTree::bulk_load(entries);
        self.needs_rebuild = false;
    }

    /// Итератор по всем элементам: (member, GeoPoint).
    pub fn iter(&self) -> impl Iterator<Item = (&String, &GeoPoint)> {
        self.member_index.iter()
    }

    /// Возвращает количество точек.
    pub fn len(&self) -> usize {
        self.member_index.len()
    }

    /// Проверяет, пусто ли множество.
    pub fn is_empty(&self) -> bool {
        self.member_index.is_empty()
    }

    /// Статистика R-tree индекса.
    pub fn index_stats(&self) -> TreeStats {
        self.rtree.stats()
    }
}

////////////////////////////////////////////////////////////////////////////////
// Общие реализации трейтов для GeoSet
////////////////////////////////////////////////////////////////////////////////

impl Default for GeoSet {
    fn default() -> Self {
        Self::new()
    }
}

////////////////////////////////////////////////////////////////////////////////
// Внутренние методы и функции
////////////////////////////////////////////////////////////////////////////////

/// Сужает диапазон и возвращает бит (0 или 1) для geohash.
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

/// Кодирует координаты в 52-битный geohash (interleaved lon/lat).
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

/// Декодирует 52-битный geohash обратно в координаты (центр ячейки).
#[allow(dead_code)]
fn decode_geohash_bits(hash: u64) -> (f64, f64) {
    let mut lon_min = -180.0;
    let mut lon_max = 180.0;
    let mut lat_min = -90.0;
    let mut lat_max = 90.0;
    for i in 0..26 {
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

/// Вычисляет расстояние между двумя точками на сфере (метры, формула
/// гаверсина).
pub fn haversine_distance(
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

////////////////////////////////////////////////////////////////////////////////
// Тесты
////////////////////////////////////////////////////////////////////////////////

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_refine_bit() {
        let mut min = 0.0;
        let mut max = 100.0;
        let b = refine_bit(75.0, &mut min, &mut max);
        assert_eq!(b, 1);
        assert!((min - 50.0).abs() < 1e-9 && (max - 100.0).abs() < 1e-9);
    }

    #[test]
    fn test_encode_decode_roundtrip() {
        let lon = 13.361389;
        let lat = 38.115556;
        let hash = encode_geohash_bits(lon, lat);
        let (lon2, lat2) = decode_geohash_bits(hash);
        assert!((lon - lon2).abs() < 1e-3);
        assert!((lat - lat2).abs() < 1e-3);
    }

    #[test]
    fn test_haversine_distance() {
        let p1 = GeoPoint { lon: 0.0, lat: 0.0 };
        let p2 = GeoPoint { lon: 0.0, lat: 1.0 };
        let d = haversine_distance(p1, p2);
        assert!((d - 111_195.0).abs() < 100.0);
    }

    #[test]
    fn test_add_get_with_rtree() {
        let mut gs = GeoSet::new();
        gs.add("A".into(), 10.0, 20.0);
        gs.add("B".into(), -5.5, 42.1);
        assert_eq!(
            gs.get("A").unwrap(),
            GeoPoint {
                lon: 10.0,
                lat: 20.0
            }
        );
        assert_eq!(
            gs.get("B").unwrap(),
            GeoPoint {
                lon: -5.5,
                lat: 42.1
            }
        );
        assert!(gs.get("C").is_none());
    }

    #[test]
    fn test_dist_method() {
        let mut gs = GeoSet::new();
        gs.add("X".into(), 0.0, 0.0);
        gs.add("Y".into(), 0.0, 1.0);
        let d = gs.dist("X", "Y").unwrap();
        assert!((d - 111_195.0).abs() < 100.0);
        assert!(gs.dist("X", "Z").is_none());
    }

    #[test]
    fn test_radius_with_rtree() {
        let mut gs = GeoSet::new();
        gs.add("near".into(), 0.1, 0.0);
        gs.add("far".into(), 1.0, 0.0);
        let res = gs.radius(0.0, 0.0, 20_000.0);
        assert_eq!(res.len(), 1);
        assert_eq!(res[0].0, "near");
    }

    #[test]
    fn test_nearest_neighbors() {
        let mut gs = GeoSet::new();
        gs.add("A".into(), 0.0, 0.0);
        gs.add("B".into(), 0.1, 0.0);
        gs.add("C".into(), 0.2, 0.0);
        gs.add("D".into(), 1.0, 0.0);

        let results = gs.nearest(0.0, 0.0, 2);
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].0, "A");
        assert_eq!(results[1].0, "B");
    }

    #[test]
    fn test_bulk_load_performance() {
        let entries: Vec<GeoEntry> = (0..1000)
            .map(|i| {
                let lon = (i % 100) as f64 * 0.1;
                let lat = (i / 100) as f64 * 0.1;
                GeoEntry {
                    member: format!("P{}", i),
                    point: GeoPoint { lon, lat },
                    score: encode_geohash_bits(lon, lat),
                }
            })
            .collect();

        let gs = GeoSet::from_entries(entries);
        assert_eq!(gs.len(), 1000);

        let stats = gs.index_stats();
        assert!(stats.depth < 10);
    }

    #[test]
    fn test_coordinate_validation() {
        let mut gs = GeoSet::new();
        gs.add("valid".into(), 0.0, 0.0);
        assert!(gs.get("valid").is_some());

        let initial_len = gs.len();
        gs.add("invalid_lon".into(), 200.0, 0.0);
        gs.add("invalid_lat".into(), 0.0, 100.0);
        assert_eq!(gs.len(), initial_len);
    }

    #[test]
    fn test_index_rebuild() {
        let mut gs = GeoSet::new();
        gs.add("A".into(), 0.0, 0.0);
        gs.add("A".into(), 1.0, 1.0);
        assert!(gs.needs_rebuild);

        gs.rebuild_index();
        assert!(!gs.needs_rebuild);
    }
}
