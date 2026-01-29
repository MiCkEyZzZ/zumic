use std::collections::HashMap;

use crate::database::{
    geo_distance::{calculate_distance, haversine_dist, DistanceMethod, DistanceUnit},
    geohash_ranges_for_bbox, BoundingBox, Geohash, GeohashPrecision, RTree, TreeStats,
};

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

#[derive(Debug, Clone)]
pub struct RadiusOptions {
    pub use_geohash: bool,
    pub geohash_precision: Option<GeohashPrecision>,
    pub include_neighbors: bool,
}

/// Множество географических точек с R-tree индексом для быстрого поиска.
#[derive(Debug)]
pub struct GeoSet {
    /// R-tree spatial index для эффективных queries
    rtree: RTree,
    /// HashMap для быстрого поиска по member name
    member_index: HashMap<String, GeoPoint>,
    /// Geohash index для approximate filtering
    geohash_index: HashMap<String, Vec<String>>,
    /// Флаг для отложенной пересборки индекса
    needs_rebuild: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct GeohashStats {
    pub bucket_count: usize,
    pub total_members: usize,
    pub avg_bucket_size: f64,
    pub max_bucket_size: usize,
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
            geohash_index: HashMap::new(),
            needs_rebuild: false,
        }
    }

    /// Создаёт GeoSet из вектора записей с bulk loading.
    pub fn from_entries(entries: Vec<GeoEntry>) -> Self {
        let mut member_index = HashMap::with_capacity(entries.len());
        let mut geohash_index: HashMap<String, Vec<String>> = HashMap::new();

        for entry in &entries {
            member_index.insert(entry.member.clone(), entry.point);

            // индексируемый по геохешу (точность 7 для баланса)
            let gh = Geohash::encode(entry.point, GeohashPrecision::High);
            geohash_index
                .entry(gh.as_str().to_string())
                .or_default()
                .push(entry.member.clone());
        }

        let rtree = RTree::bulk_load(entries);

        Self {
            rtree,
            member_index,
            geohash_index,
            needs_rebuild: false,
        }
    }

    /// Добавляет или обновляет точку по имени.
    pub fn add(
        &mut self,
        member: String,
        lon: f64,
        lat: f64,
    ) {
        if !Self::validate_coords(lon, lat) {
            return;
        }

        let point = GeoPoint { lon, lat };
        let score = encode_geohash_bits(lon, lat);

        // Проверяем, нужна ли пересборка
        if let Some(old_point) = self.member_index.insert(member.clone(), point) {
            if old_point != point {
                self.needs_rebuild = true;
            }
        } else {
            // Новая точка - добавляем в geohash_index
            let gh = Geohash::encode(point, GeohashPrecision::High);
            self.geohash_index
                .entry(gh.as_str().to_string())
                .or_default()
                .push(member.clone());
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

    /// Возвращает geohash точки по имени.
    pub fn get_geohash(
        &self,
        member: &str,
        precision: GeohashPrecision,
    ) -> Option<Geohash> {
        self.get(member).map(|p| Geohash::encode(p, precision))
    }

    /// Вычисляет расстояние между двумя точками (по умолчанию Haversine).
    pub fn dist(
        &self,
        m1: &str,
        m2: &str,
    ) -> Option<f64> {
        self.dist_with_method(m1, m2, DistanceMethod::Haversine)
    }

    /// Вычисляет расстояние с указанным методом.
    pub fn dist_with_method(
        &self,
        m1: &str,
        m2: &str,
        method: DistanceMethod,
    ) -> Option<f64> {
        let p1 = self.get(m1)?;
        let p2 = self.get(m2)?;
        Some(calculate_distance(p1, p2, method).distance_m)
    }

    /// Вычисляет расстояние в указанных единицах.
    pub fn dist_in_units(
        &self,
        m1: &str,
        m2: &str,
        method: DistanceMethod,
        unit: DistanceUnit,
    ) -> Option<f64> {
        let dist_m = self.dist_with_method(m1, m2, method)?;
        Some(unit.convert_from_meters(dist_m))
    }

    /// Возвращает всех членов в радиусе `radius_m` метров от точки (`lon`,
    /// `lat`). Использует R-tree для эффективного поиска.
    pub fn radius(
        &mut self,
        lon: f64,
        lat: f64,
        radius_m: f64,
    ) -> Vec<(String, f64)> {
        self.radius_with_options(lon, lat, radius_m, RadiusOptions::default())
    }

    pub fn radius_with_options(
        &mut self,
        lon: f64,
        lat: f64,
        radius_m: f64,
        options: RadiusOptions,
    ) -> Vec<(String, f64)> {
        if !Self::validate_coords(lon, lat) {
            return Vec::new();
        }

        // Пересобираем индекс
        if self.needs_rebuild {
            self.rebuild_index();
        }

        let center = GeoPoint { lon, lat };

        // Если включена фильтрация геохеша
        if options.use_geohash {
            return self.radius_with_geohash(center, radius_m, options);
        }

        // В качестве резервного варианта используется только R-дерево
        let bbox = Self::radius_to_bbox(center, radius_m);
        let candidates = self.rtree.range_query(&bbox);

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

    /// Запрос радиуса с настраиваемым методом дальности.
    pub fn radius_with_method(
        &mut self,
        lon: f64,
        lat: f64,
        radius: f64,
        radius_unit: DistanceUnit,
        options: RadiusOptions,
        method: DistanceMethod,
    ) -> Vec<(String, f64)> {
        if !Self::validate_coords(lon, lat) {
            return Vec::new();
        }

        if self.needs_rebuild {
            self.rebuild_index();
        }

        let center = GeoPoint { lon, lat };
        let radius_m = radius_unit.convert_to_meters(radius);
        let use_geohash = options.use_geohash;

        // Используем geohash или R-tree фильтрацию
        let candidates = if use_geohash {
            self.radius_candidates_geohash(center, radius_m, options)
        } else {
            let bbox = Self::radius_to_bbox(center, radius_m);
            self.rtree.range_query(&bbox)
        };

        // Точная фильтрация с выбранным методом
        candidates
            .into_iter()
            .filter_map(|entry| {
                let point = if use_geohash {
                    self.member_index.get(&entry.member)?
                } else {
                    &entry.point
                };

                let dist = calculate_distance(center, *point, method).distance_m;

                if dist <= radius_m {
                    Some((entry.member.clone(), dist))
                } else {
                    None
                }
            })
            .collect()
    }

    pub fn bbox_query(
        &self,
        bbox: &BoundingBox,
    ) -> Vec<String> {
        // используем диапазоны геохешей для оптимизации
        let precision = GeohashPrecision::Medium;
        let ranges = geohash_ranges_for_bbox(bbox, precision);

        let mut results = Vec::new();
        for range_prefix in ranges {
            // ищем все ячейки с этим префиксом
            for (gh, members) in &self.geohash_index {
                if gh.starts_with(&range_prefix) {
                    for member in members {
                        if let Some(point) = self.member_index.get(member) {
                            if bbox.contains_point(*point) {
                                results.push(member.clone());
                            }
                        }
                    }
                }
            }
        }

        results
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
            .map(|(entry, dist)| (entry.member.clone(), dist))
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

    /// k-NN с настраиваемым методом расстояния.
    pub fn nearest_with_method(
        &self,
        lon: f64,
        lat: f64,
        k: usize,
        method: DistanceMethod,
    ) -> Vec<(String, f64)> {
        if !Self::validate_coords(lon, lat) || k == 0 {
            return Vec::new();
        }

        let point = GeoPoint { lon, lat };

        // Используем R-дерево для получения кандидатов (с запасом)
        let candidates = self.rtree.knn(point, k * 2);

        // Пересчитываем расстояние с выбранным методом
        let mut results: Vec<(String, f64)> = candidates
            .into_iter()
            .map(|(entry, _)| {
                let dist = calculate_distance(point, entry.point, method).distance_m;
                (entry.member, dist)
            })
            .collect();

        // Сортируем и берём top k
        results.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
        results.truncate(k);
        results
    }

    fn radius_with_geohash(
        &self,
        center: GeoPoint,
        radius_m: f64,
        options: RadiusOptions,
    ) -> Vec<(String, f64)> {
        // выбираем оптимальную точность
        let precision = options
            .geohash_precision
            .unwrap_or_else(|| GeohashPrecision::from_radius(radius_m));

        let center_gh = Geohash::encode(center, precision);

        // собираем кандидатов из центраьной ячейки
        let mut candidate_members = Vec::new();
        if let Some(members) = self.geohash_index.get(center_gh.as_str()) {
            candidate_members.extend(members.iter().cloned());
        }

        // если нужно, добавляем соседние ячейки
        if options.include_neighbors {
            for neighbor in center_gh.all_neighbors() {
                if let Some(members) = self.geohash_index.get(neighbor.as_str()) {
                    candidate_members.extend(members.iter().cloned());
                }
            }
        }

        // точная фильтрация с использованием формулы гаверсинусов
        candidate_members
            .into_iter()
            .filter_map(|member| {
                let point = self.member_index.get(&member)?;
                let dist = haversine_distance(center, *point);
                if dist <= radius_m {
                    Some((member, dist))
                } else {
                    None
                }
            })
            .collect()
    }

    /// Получает кандидатов через geohash.
    fn radius_candidates_geohash(
        &self,
        center: GeoPoint,
        radius_m: f64,
        options: RadiusOptions,
    ) -> Vec<GeoEntry> {
        let precision = options
            .geohash_precision
            .unwrap_or_else(|| GeohashPrecision::from_radius(radius_m));

        let center_gh = Geohash::encode(center, precision);
        let mut cadidate_members = Vec::new();

        if let Some(members) = self.geohash_index.get(center_gh.as_str()) {
            cadidate_members.extend(members.iter().cloned());
        }

        if options.include_neighbors {
            for neighbor in center_gh.all_neighbors() {
                if let Some(members) = self.geohash_index.get(neighbor.as_str()) {
                    cadidate_members.extend(members.iter().cloned());
                }
            }
        }

        // Конвертирует member в GeoEntry
        cadidate_members
            .into_iter()
            .filter_map(|member| {
                let point = *self.member_index.get(&member)?;
                Some(GeoEntry {
                    member,
                    point,
                    score: 0,
                })
            })
            .collect()
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

        // Пересобираем индекс геохеша
        self.geohash_index.clear();
        for entry in &entries {
            let gh = Geohash::encode(entry.point, GeohashPrecision::High);
            self.geohash_index
                .entry(gh.as_str().to_string())
                .or_default()
                .push(entry.member.clone());
        }

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

    pub fn geohash_stats(&self) -> GeohashStats {
        let bucket_count = self.geohash_index.len();
        let total_members: usize = self.geohash_index.values().map(|v| v.len()).sum();
        let avg_bucket_size = if bucket_count > 0 {
            total_members as f64 / bucket_count as f64
        } else {
            0.0
        };

        let max_bucket_size = self
            .geohash_index
            .values()
            .map(|v| v.len())
            .max()
            .unwrap_or(0);

        GeohashStats {
            bucket_count,
            total_members,
            avg_bucket_size,
            max_bucket_size,
        }
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

impl Default for RadiusOptions {
    fn default() -> Self {
        Self {
            use_geohash: true,
            geohash_precision: None,
            include_neighbors: true,
        }
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
    haversine_dist(p1, p2)
}

////////////////////////////////////////////////////////////////////////////////
// Тесты
////////////////////////////////////////////////////////////////////////////////

#[cfg(test)]
mod tests {
    use super::*;

    /// Тест проверяет корректность работы ф-ии refine_bit: правильный выбор
    /// бита и корректность обновление границ диапазона.
    #[test]
    fn test_refine_bit() {
        let mut min = 0.0;
        let mut max = 100.0;
        let b = refine_bit(75.0, &mut min, &mut max);
        assert_eq!(b, 1);
        assert!((min - 50.0).abs() < 1e-9 && (max - 100.0).abs() < 1e-9);
    }

    /// Тест проверяет, что кодирование в geohash и обратное декодирование
    /// сохраняет координаты с допустимой погрешностью.
    #[test]
    fn test_encode_decode_roundtrip() {
        let lon = 13.361389;
        let lat = 38.115556;
        let hash = encode_geohash_bits(lon, lat);
        let (lon2, lat2) = decode_geohash_bits(hash);
        assert!((lon - lon2).abs() < 1e-3);
        assert!((lat - lat2).abs() < 1e-3);
    }

    /// Тест проверяет корректность вычисления расстояния между двумя
    /// географическими точками по формуле Гаверсина.
    #[test]
    fn test_haversine_distance() {
        let p1 = GeoPoint { lon: 0.0, lat: 0.0 };
        let p2 = GeoPoint { lon: 0.0, lat: 1.0 };
        let d = haversine_distance(p1, p2);
        assert!((d - 111_195.0).abs() < 100.0);
    }

    /// Тест проверяет добавление точек в GeoSet и корректность их извлечения по
    /// имени с использованием R-tree индекса.
    #[test]
    fn test_add_get_with_rtree() {
        let mut gs = GeoSet::new();
        gs.add("A".to_string(), 10.0, 20.0);
        gs.add("B".to_string(), -5.5, 42.1);
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

    /// Тест проверяет вычисление расстояния между двумя точками, добавленыыми в
    /// GeoSet, а также корректную обработку отсутствующих членов.
    #[test]
    fn test_dist_method() {
        let mut gs = GeoSet::new();
        gs.add("X".into(), 0.0, 0.0);
        gs.add("Y".into(), 0.0, 1.0);
        let d = gs.dist("X", "Y").unwrap();
        assert!((d - 111_195.0).abs() < 100.0);
        assert!(gs.dist("X", "Z").is_none());
    }

    /// Тест проверяет поиск точек в заданном радиусе и корректную фильтрацию
    /// кандидатов с использованием R-tree.
    #[test]
    fn test_radius_with_rtree() {
        let mut gs = GeoSet::new();
        gs.add("near".into(), 0.1, 0.0);
        gs.add("far".into(), 1.0, 0.0);

        let opts = RadiusOptions {
            use_geohash: false,
            geohash_precision: None,
            include_neighbors: false,
        };

        let res = gs.radius_with_options(0.0, 0.0, 20_000.0, opts);
        assert_eq!(res.len(), 1);
        assert_eq!(res[0].0, "near");
    }

    /// Тест проверяет поиск `к` ближайших соседей к заданной точке и
    /// корректность порядка результатов по расстоянию.
    #[test]
    fn test_nearest_neighbors() {
        let mut gs = GeoSet::new();
        gs.add("A".to_string(), 0.0, 0.0);
        gs.add("B".to_string(), 0.1, 0.0);
        gs.add("C".to_string(), 0.2, 0.0);
        gs.add("D".to_string(), 1.0, 0.0);

        let results = gs.nearest(0.0, 0.0, 2);
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].0, "A");
        assert_eq!(results[1].0, "B");
    }

    /// Тест проверяет корректность bulk-загрузки большого количества элементов
    /// и адекватную глубину R-tree индекса.
    #[test]
    fn test_bulk_load_performance() {
        let entries: Vec<GeoEntry> = (0..1000)
            .map(|i| {
                let lon = (i % 100) as f64 * 0.1;
                let lat = (i / 100) as f64 * 0.1;
                GeoEntry {
                    member: format!("P{i}"),
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

    /// Тест проверяет валидацию координат и то, что некорректные координаты не
    /// добавляются в GeoSet.
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

    /// Тест проверяет необходимость пересборки R-tree при обновлении
    /// существующей точки и корректную работу rebuild_index.
    #[test]
    fn test_index_rebuild() {
        let mut gs = GeoSet::new();
        gs.add("A".into(), 0.0, 0.0);
        gs.add("A".into(), 1.0, 1.0);
        assert!(gs.needs_rebuild);

        gs.rebuild_index();
        assert!(!gs.needs_rebuild);
    }

    #[test]
    fn test_geohash_integration() {
        let mut gs = GeoSet::new();
        gs.add("A".to_string(), 10.0, 20.0);
        gs.add("B".to_string(), 10.1, 20.1);

        let gh = gs.get_geohash("A", GeohashPrecision::High).unwrap();
        assert_eq!(gh.precision(), 7);
    }

    #[test]
    fn test_radius_with_geohash_filter() {
        let mut gs = GeoSet::new();

        for i in 0..100 {
            let lon = (i % 10) as f64 * 0.01;
            let lat = (i % 10) as f64 * 0.01;
            gs.add(format!("P{i}"), lon, lat);
        }

        let opts = RadiusOptions {
            use_geohash: true,
            geohash_precision: Some(GeohashPrecision::High),
            include_neighbors: true,
        };

        let results = gs.radius_with_options(0.0, 0.0, 5000.0, opts);
        assert!(!results.is_empty());
    }

    #[test]
    fn test_bbox_query() {
        let mut gs = GeoSet::new();
        gs.add("A".to_string(), 0.0, 0.0);
        gs.add("B".to_string(), 1.0, 1.0);
        gs.add("C".to_string(), 5.0, 5.0);

        let bbox = BoundingBox::new(-0.5, 1.5, -0.5, 1.5);
        let results = gs.bbox_query(&bbox);
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_geohash_stats() {
        let mut gs = GeoSet::new();

        for i in 0..50 {
            gs.add(format!("P{i}"), (i as f64) * 0.1, 0.0);
        }

        let stats = gs.geohash_stats();
        assert!(stats.bucket_count > 0);
        assert_eq!(stats.total_members, 50);
        assert!(stats.avg_bucket_size > 0.0);
    }

    #[test]
    fn test_radius_comparison() {
        let mut gs = GeoSet::new();

        for i in 0..1000 {
            let lon = (i % 100) as f64 * 0.01;
            let lat = (i / 100) as f64 * 0.01;
            gs.add(format!("P{i}"), lon, lat);
        }

        // C geohash filtering
        let opts_gh = RadiusOptions {
            use_geohash: true,
            geohash_precision: Some(GeohashPrecision::High),
            include_neighbors: true,
        };

        let results_gh = gs.radius_with_options(0.5, 0.5, 5000.0, opts_gh);

        // без geohash (только R-tree)
        let opts_rtree = RadiusOptions {
            use_geohash: false,
            geohash_precision: None,
            include_neighbors: false,
        };
        let results_rtree = gs.radius_with_options(0.5, 0.3, 5000.0, opts_rtree);

        // результаты должны быть одинаковыми
        assert_eq!(results_gh.len(), results_rtree.len());
    }

    #[test]
    fn test_dist_with_different_methods() {
        let mut gs = GeoSet::new();
        gs.add("A".into(), 0.0, 0.0);
        gs.add("B".into(), 1.0, 0.0);

        let haversine = gs
            .dist_with_method("A", "B", DistanceMethod::Haversine)
            .unwrap();
        let vincenty = gs
            .dist_with_method("A", "B", DistanceMethod::Vincenty)
            .unwrap();

        // Vincenty точнее, но оба должны быть одного порядка
        let diff = (haversine - vincenty).abs();

        assert!(diff < 300.0, "diff={diff}m");
        assert!(vincenty > 0.0);
    }

    #[test]
    fn test_dist_in_units() {
        let mut gs = GeoSet::new();
        gs.add("X".into(), 0.0, 0.0);
        gs.add("Y".into(), 0.0, 1.0); // ~111км

        let meters = gs
            .dist_in_units("X", "Y", DistanceMethod::Haversine, DistanceUnit::Meters)
            .unwrap();
        let km = gs
            .dist_in_units(
                "X",
                "Y",
                DistanceMethod::Haversine,
                DistanceUnit::Kilometers,
            )
            .unwrap();

        assert!((km * 1000.0 - meters).abs() < 0.1);
        assert!((km - 111.0).abs() < 0.5);
    }

    #[test]
    fn test_radius_with_vincenty() {
        let mut gs = GeoSet::new();

        for i in 0..100 {
            let lon = (i % 10) as f64 * 0.01;
            let lat = (i / 10) as f64 * 0.01;
            gs.add(format!("P{}", i), lon, lat);
        }

        let opts = RadiusOptions {
            use_geohash: false,
            geohash_precision: None,
            include_neighbors: false,
        };

        let results = gs.radius_with_method(
            0.0,
            0.0,
            0.5,
            DistanceUnit::Kilometers,
            opts,
            DistanceMethod::Vincenty,
        );

        assert!(!results.is_empty());

        // Все результаты должны быть в пределах 5км
        for (_member, dist) in &results {
            assert!(*dist <= 5000.0);
        }
    }

    #[test]
    fn test_nearest_with_manhattan() {
        let mut gs = GeoSet::new();
        gs.add("A".into(), 0.0, 0.0);
        gs.add("B".into(), 0.1, 0.0);
        gs.add("C".into(), 0.0, 0.1);
        gs.add("D".into(), 1.0, 1.0);

        let results = gs.nearest_with_method(0.0, 0.0, 2, DistanceMethod::Manhattan);

        assert_eq!(results.len(), 2);
        // A должен быть первым (расстояние 0)
        assert_eq!(results[0].0, "A");
    }
}
