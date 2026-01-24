use crate::{database::BoundingBox, GeoPoint};

const BASE32: &[u8; 32] = b"0123456789bcdefghjkmnpqrstuvwxyz";
const BASE32_REV: [i8; 128] = build_base32_rev();

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    North,
    South,
    East,
    West,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GeohashPrecision {
    VeryLow = 4,
    Low = 5,
    Medium = 6,
    High = 7,
    VeryHigh = 8,
    UltraHigh = 9,
    Precise = 10,
    VeryPrecise = 11,
    UltraPrecise = 12,
}

// Строка выбрана с учетом читаемости;
// уровень хранения может использовать упакованную форму.
// NOTE: в будущем будет заменён на SmallVec<[u8; 12]> или [u8; 12] + len
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Geohash {
    hash: String,
    precision: usize,
}

impl GeohashPrecision {
    pub fn cell_size_meters(&self) -> f64 {
        match self {
            GeohashPrecision::VeryLow => 20_000.0,
            GeohashPrecision::Low => 4_900.0,
            GeohashPrecision::Medium => 1_200.0,
            GeohashPrecision::High => 153.0,
            GeohashPrecision::VeryHigh => 38.0,
            GeohashPrecision::UltraHigh => 4.8,
            GeohashPrecision::Precise => 1.2,
            GeohashPrecision::VeryPrecise => 0.149,
            GeohashPrecision::UltraPrecise => 0.037,
        }
    }

    pub fn from_radius(radius_m: f64) -> Self {
        if radius_m > 10_000.0 {
            GeohashPrecision::VeryLow
        } else if radius_m > 2_500.0 {
            GeohashPrecision::Low
        } else if radius_m > 600.0 {
            GeohashPrecision::Medium
        } else if radius_m > 80.0 {
            GeohashPrecision::High
        } else if radius_m > 20.0 {
            GeohashPrecision::VeryHigh
        } else if radius_m > 2.5 {
            GeohashPrecision::UltraHigh
        } else if radius_m > 0.6 {
            GeohashPrecision::Precise
        } else if radius_m > 0.075 {
            GeohashPrecision::VeryPrecise
        } else {
            GeohashPrecision::UltraPrecise
        }
    }
}

impl Geohash {
    pub fn encode(
        point: GeoPoint,
        precision: GeohashPrecision,
    ) -> Self {
        Self::encode_with_chars(point, precision as usize)
    }

    pub fn encode_with_chars(
        point: GeoPoint,
        chars: usize,
    ) -> Self {
        let chars = chars.clamp(4, 12);
        let hash = encode_base32(point.lon, point.lat, chars);
        Self {
            hash,
            precision: chars,
        }
    }

    pub fn decode(&self) -> GeoPoint {
        decode_base32(&self.hash)
    }

    pub fn decode_bbox(&self) -> BoundingBox {
        decode_bbox(&self.hash)
    }

    pub fn as_str(&self) -> &str {
        &self.hash
    }

    pub fn precision(&self) -> usize {
        self.precision
    }

    pub fn neighbor(
        &self,
        direction: Direction,
    ) -> Self {
        let bbox = self.decode_bbox();

        // Ширина и высота ячейки
        let cell_width = bbox.max_lon - bbox.min_lon;
        let cell_height = bbox.max_lat - bbox.min_lat;

        let center = GeoPoint {
            lon: (bbox.min_lon + bbox.max_lon) * 0.5,
            lat: (bbox.min_lat + bbox.max_lat) * 0.5,
        };

        let new_point = match direction {
            Direction::North => GeoPoint {
                lon: center.lon,
                lat: center.lat + cell_height,
            },
            Direction::South => GeoPoint {
                lon: center.lon,
                lat: center.lat - cell_height,
            },
            Direction::East => GeoPoint {
                lon: center.lon + cell_width,
                lat: center.lat,
            },
            Direction::West => GeoPoint {
                lon: center.lon - cell_width,
                lat: center.lat,
            },
        };

        Geohash::encode_with_chars(new_point, self.precision)
    }

    // Возвращает 4 основных соседа (север, юг, восток, запад).
    // Соседи по диагонали пока намеренно исключены.
    // При необходимости можно расширить до 8-направленной смежности.
    pub fn all_neighbors(&self) -> Vec<Geohash> {
        let n = self.neighbor(Direction::North);
        let s = self.neighbor(Direction::South);
        let e = self.neighbor(Direction::East);
        let w = self.neighbor(Direction::West);

        vec![n, s, e, w]
    }

    pub fn has_prefix(
        &self,
        prefix: &str,
    ) -> bool {
        self.hash.starts_with(prefix)
    }

    pub fn prefix(
        &self,
        len: usize,
    ) -> String {
        self.hash.chars().take(len.min(self.precision)).collect()
    }

    pub fn parent(&self) -> Option<Geohash> {
        if self.precision <= 1 {
            return None;
        }
        Some(Geohash {
            hash: self.prefix(self.precision - 1),
            precision: self.precision - 1,
        })
    }

    pub fn children(&self) -> Vec<Geohash> {
        if self.precision >= 12 {
            return vec![];
        }

        (0..32)
            .map(|i| {
                let mut child_hash = self.hash.clone();
                child_hash.push(BASE32[i] as char);
                Geohash {
                    hash: child_hash,
                    precision: self.precision + 1,
                }
            })
            .collect()
    }
}

pub fn geohash_ranges_for_bbox(
    bbox: &BoundingBox,
    precision: GeohashPrecision,
) -> Vec<String> {
    let chars = precision as usize;

    // кодируем углы bbox
    let sw = encode_base32(bbox.min_lon, bbox.min_lat, chars);
    let ne = encode_base32(bbox.max_lon, bbox.max_lat, chars);

    // если это одна ячейка, возвращаем её
    if sw == ne {
        return vec![sw];
    }

    // находим общий префикс
    let common_prefix_len = sw
        .chars()
        .zip(ne.chars())
        .take_while(|(a, b)| a == b)
        .count();

    if common_prefix_len == 0 {
        // нет общего префикса - покрываем весь мир (редкий случай)
        return vec!["".to_string()];
    }

    // генерируем все возможные хэши с общим префиксом
    let mut ranges = Vec::new();
    generate_ranges_recursive(&sw[..common_prefix_len], chars, &mut ranges);
    ranges
}

// NOTE: может генерировать множество диапазонов для больших ограничивающих
// прямоугольников;
// ожидается, что вызывающая сторона ограничит точность.
fn generate_ranges_recursive(
    prefix: &str,
    target_len: usize,
    results: &mut Vec<String>,
) {
    if prefix.len() >= target_len {
        results.push(prefix.to_string());
        return;
    }

    // Генерация всех 32-х вариантов следующего символа
    for &ch in BASE32.iter() {
        let mut next = prefix.to_string();
        next.push(ch as char);
        generate_ranges_recursive(&next, target_len, results);
    }
}

fn encode_base32(
    lon: f64,
    lat: f64,
    chars: usize,
) -> String {
    let mut lon_min = -180.0;
    let mut lon_max = 180.0;
    let mut lat_min = -90.0;
    let mut lat_max = 90.0;

    let mut hash = String::with_capacity(chars);
    let mut bits = 0u8;
    let mut bit_count = 0;

    // Производим чередование битов долготы/широты
    for _ in 0..chars * 5 {
        let is_lon = bit_count % 2 == 0;

        let (v, min, max) = if is_lon {
            (lon, &mut lon_min, &mut lon_max)
        } else {
            (lat, &mut lat_min, &mut lat_max)
        };

        let mid = (*min + *max) * 0.5;
        if v >= mid {
            bits |= 1 << (4 - (bit_count % 5));
            *min = mid;
        } else {
            *max = mid;
        }

        bit_count += 1;
        if bit_count % 5 == 0 {
            hash.push(BASE32[bits as usize] as char);
            bits = 0;
        }
    }
    hash
}

fn decode_base32(hash: &str) -> GeoPoint {
    let bbox = decode_bbox(hash);
    GeoPoint {
        lon: (bbox.min_lon + bbox.max_lon) * 0.5,
        lat: (bbox.min_lat + bbox.max_lat) * 0.5,
    }
}

fn decode_bbox(hash: &str) -> BoundingBox {
    let mut lon_min = -180.0;
    let mut lon_max = 180.0;
    let mut lat_min = -90.0;
    let mut lat_max = 90.0;

    let mut bit_index = 0;

    for ch in hash.chars() {
        let idx = BASE32_REV[ch as usize];
        if idx < 0 {
            break;
        }

        for i in 0..5 {
            let bit = (idx >> (4 - i)) & 1;
            let is_lon = bit_index % 2 == 0;

            let (min, max) = if is_lon {
                (&mut lon_min, &mut lon_max)
            } else {
                (&mut lat_min, &mut lat_max)
            };

            let mid = (*min + *max) * 0.5;
            if bit == 1 {
                *min = mid;
            } else {
                *max = mid;
            }

            bit_index += 1;
        }
    }

    BoundingBox {
        min_lon: lon_min,
        max_lon: lon_max,
        min_lat: lat_min,
        max_lat: lat_max,
    }
}

const fn build_base32_rev() -> [i8; 128] {
    let mut table = [-1i8; 128];
    let alphabet = b"0123456789bcdefghjkmnpqrstuvwxyz";

    let mut i = 0;
    while i < alphabet.len() {
        let c = alphabet[i] as usize;
        table[c] = i as i8;
        table[(alphabet[i] & !0x20) as usize] = i as i8; // верхний регистр
        i += 1;
    }
    table
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encode_decode_roundtrip() {
        let point = GeoPoint {
            lon: 13.361389,
            lat: 38.115556,
        };
        let gh = Geohash::encode(point, GeohashPrecision::High);
        let decoded = gh.decode();

        // Точность ~153м для 7 символов
        assert!((point.lon - decoded.lon).abs() < 0.01);
        assert!((point.lat - decoded.lat).abs() < 0.01);
    }

    #[test]
    fn test_precision_selection() {
        assert_eq!(GeohashPrecision::from_radius(100.0), GeohashPrecision::High,);
        assert_eq!(GeohashPrecision::from_radius(5000.0), GeohashPrecision::Low);
    }

    #[test]
    fn test_neighbors() {
        let gh = Geohash::encode(GeoPoint { lon: 0.0, lat: 0.0 }, GeohashPrecision::Medium);
        let neighbors = gh.all_neighbors();
        assert_eq!(neighbors.len(), 4);

        // Все соседи должны быть уникальными
        let unique: std::collections::HashSet<_> = neighbors.iter().map(|n| &n.hash).collect();
        assert_eq!(unique.len(), 4);
    }

    #[test]
    fn test_parent_child() {
        let gh = Geohash::encode(
            GeoPoint {
                lon: 13.4,
                lat: 52.5,
            },
            GeohashPrecision::High,
        );

        let parent = gh.parent().unwrap();
        assert_eq!(parent.precision(), gh.precision() - 1);
        assert!(gh.has_prefix(parent.as_str()));

        let children = parent.children();
        assert_eq!(children.len(), 32);
        assert!(children.iter().any(|c| c.hash == gh.hash));
    }

    #[test]
    fn test_bbox_ranges() {
        let bbox = BoundingBox::new(-0.5, 0.5, -0.5, 0.5);
        let ranges = geohash_ranges_for_bbox(&bbox, GeohashPrecision::Medium);
        assert!(!ranges.is_empty());
    }

    #[test]
    fn test_neighbor_cardinal_directions() {
        let gh = Geohash::encode(GeoPoint { lon: 0.0, lat: 0.0 }, GeohashPrecision::Medium);

        let north = gh.neighbor(Direction::North);
        let south = gh.neighbor(Direction::South);
        let east = gh.neighbor(Direction::East);
        let west = gh.neighbor(Direction::West);

        // North должен быть севернее
        assert!(north.decode().lat > gh.decode().lat);
        // South должен быть южнее
        assert!(south.decode().lat < gh.decode().lat);
        // East должен быть восточнее
        assert!(east.decode().lon > gh.decode().lon);
        // West должен быть западнее
        assert!(west.decode().lon < gh.decode().lon);
    }

    #[test]
    fn test_prefix_matching() {
        let gh1 = Geohash::encode(
            GeoPoint {
                lon: 13.4,
                lat: 52.5,
            },
            GeohashPrecision::High,
        );
        let gh2 = Geohash::encode(
            GeoPoint {
                lon: 13.41,
                lat: 52.51,
            },
            GeohashPrecision::High,
        );

        let prefix = gh1.prefix(5);
        assert!(gh1.has_prefix(&prefix));

        // Близкие точки должны иметь общий префикс
        let common_len = gh1
            .as_str()
            .chars()
            .zip(gh2.as_str().chars())
            .take_while(|(a, b)| a == b)
            .count();
        assert!(common_len >= 4);
    }

    #[test]
    fn test_cell_size() {
        assert!(GeohashPrecision::VeryLow.cell_size_meters() > 10_000.0);
        assert!(GeohashPrecision::UltraPrecise.cell_size_meters() < 0.1);
    }
}
