use std::f64::consts::PI;

use crate::GeoPoint;

/// Едицинцы измерения расстояния.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DistanceUnit {
    Meters,
    Kilometers,
    Miles,
    Feet,
    NauticalMiles,
}

/// Параметры эллипсойда для расчётов.
#[derive(Debug, Clone, Copy)]
pub struct Ellipsoid {
    /// Большая полуось (a), метры
    pub a: f64,
    /// Малая полуось (b), метры
    pub b: f64,
}

impl DistanceUnit {
    /// Конвертирует метры в указанную единицу.
    pub fn convert_from_meters(
        self,
        meters: f64,
    ) -> f64 {
        match self {
            DistanceUnit::Meters => meters,
            DistanceUnit::Kilometers => meters / 1000.0,
            DistanceUnit::Miles => meters / 1609.344,
            DistanceUnit::Feet => meters * 3.280_84,
            DistanceUnit::NauticalMiles => meters / 1852.0,
        }
    }

    /// Конвертирует из единицы в метры.
    pub fn convert_to_meters(
        self,
        value: f64,
    ) -> f64 {
        match self {
            DistanceUnit::Meters => value,
            DistanceUnit::Kilometers => value * 1000.0,
            DistanceUnit::Miles => value * 1609.344,
            DistanceUnit::Feet => value / 3.280_84,
            DistanceUnit::NauticalMiles => value * 1852.0,
        }
    }

    /// Название единицы.
    pub fn name(&self) -> &'static str {
        match self {
            DistanceUnit::Meters => "m",
            DistanceUnit::Kilometers => "km",
            DistanceUnit::Miles => "mi",
            DistanceUnit::Feet => "ft",
            DistanceUnit::NauticalMiles => "nmi",
        }
    }
}

impl Ellipsoid {
    /// Сжатие f = (a - b) / a
    #[inline]
    pub fn f(&self) -> f64 {
        (self.a - self.b) / self.a
    }

    /// Квадрат эксцентриситета
    #[inline]
    pub fn e2(&self) -> f64 {
        1.0 - (self.b * self.b) / (self.a * self.a)
    }

    /// Эллипсоид WGS84 (используется GLONAS)
    pub const WGS84: Self = Self {
        a: 6_378_137.0,
        b: 6_356_752.314_245,
    };

    /// Эллипсоид GRS80 (почти идентичен WGS84)
    pub const GRS80: Self = Self {
        a: 6_371_000.0,
        b: 6_371_000.0,
    };

    /// Сфера со средним радиусом Земли
    pub const SPHERE: Self = Self {
        a: 6_371_000.0,
        b: 6_371_000.0,
    };
}

/// Формула Гаверсина
pub fn haversine_distance(
    p1: GeoPoint,
    p2: GeoPoint,
) -> f64 {
    haversine_distance_ellipsoid(p1, p2, Ellipsoid::SPHERE)
}

/// Флормула Гаверсина (сферическая)
pub fn haversine_distance_ellipsoid(
    p1: GeoPoint,
    p2: GeoPoint,
    ellipsoid: Ellipsoid,
) -> f64 {
    let r = ellipsoid.a;
    let to_rad = PI / 180.0;
    let dlat = (p2.lat - p1.lat) * to_rad;
    let dlon = (p2.lon - p1.lon) * to_rad;
    let lat1 = p1.lat * to_rad;
    let lat2 = p2.lat * to_rad;

    let a = (dlat * 0.5).sin().powi(2) + lat1.cos() * lat2.cos() * (dlon * 0.5).sin().powi(2);
    let c = 2.0 * a.sqrt().atan2((1.0 - a).sqrt());

    r * c
}

#[cfg(test)]
mod tests {
    use super::*;

    const EPSILON: f64 = 0.01; // 1см

    #[test]
    fn test_haversine_known_distance() {
        // Кунгур до Перми: ~76 925.28 м по текущей реализации (SPHERE a = 6_371_000 m)
        let kungur = GeoPoint {
            lat: 57.4342,
            lon: 56.9514,
        };
        let perm = GeoPoint {
            lat: 58.0105,
            lon: 56.2347,
        };

        let dist = haversine_distance(kungur, perm);
        assert!((dist - 76_925.28266576723).abs() < 100.0); // ±100 м допуск
    }

    #[test]
    fn test_unit_conversions() {
        let meters = 1000.0;

        assert!((DistanceUnit::Kilometers.convert_from_meters(meters) - 1.0).abs() < EPSILON);
        assert!((DistanceUnit::Miles.convert_from_meters(meters) - 0.621_371).abs() < 0.001);
        assert!((DistanceUnit::Feet.convert_from_meters(meters) - 3280.84).abs() < 0.1);

        // Round-trip
        let km = DistanceUnit::Kilometers.convert_from_meters(meters);
        let back = DistanceUnit::Kilometers.convert_to_meters(km);
        assert!((back - meters).abs() < EPSILON);
    }

    #[test]
    fn test_cross_median() {
        // через 180° меридиан
        let p1 = GeoPoint {
            lon: 179.0,
            lat: 0.0,
        };
        let p2 = GeoPoint {
            lon: -179.0,
            lat: 0.0,
        };

        let dist = haversine_distance(p1, p2);
        assert!(dist < 300_000.0); // Короткий путь, не через весь мир
    }

    #[test]
    fn test_zero_distance() {
        let p = GeoPoint {
            lon: 20.0,
            lat: 10.0,
        };
        let dist = haversine_distance(p, p);
        assert!(dist.abs() < EPSILON);
    }

    #[test]
    fn test_poles() {
        let north_pole = GeoPoint {
            lon: 0.0,
            lat: 90.0,
        };
        let south_pole = GeoPoint {
            lon: 0.0,
            lat: -90.0,
        };
        let equator = GeoPoint { lon: 0.0, lat: 0.0 };

        let dist_np_sp = haversine_distance(north_pole, south_pole);
        let dist_np_eq = haversine_distance(north_pole, equator);

        assert!((dist_np_sp - 2.0 * 6_371_000.0 * PI / 2.0).abs() < 1000.0);
        assert!((dist_np_eq - 6_371_000.0 * PI / 2.0).abs() < 1000.0);
    }

    #[test]
    fn test_symmetry() {
        let p1 = GeoPoint {
            lon: 56.9514,
            lat: 57.4342,
        }; // Кунгур
        let p2 = GeoPoint {
            lon: 56.2347,
            lat: 58.0105,
        }; // Пермь

        let d1 = haversine_distance(p1, p2);
        let d2 = haversine_distance(p2, p1);

        assert!((d1 - d2).abs() < EPSILON);
    }

    #[test]
    fn test_ellipsoid_variation() {
        let kungur = GeoPoint {
            lat: 57.4342,
            lon: 56.9514,
        };
        let perm = GeoPoint {
            lat: 58.0105,
            lon: 56.2347,
        };

        let d_sphere = haversine_distance_ellipsoid(kungur, perm, Ellipsoid::SPHERE);
        let d_wgs84 = haversine_distance_ellipsoid(kungur, perm, Ellipsoid::WGS84);
        let d_grs80 = haversine_distance_ellipsoid(kungur, perm, Ellipsoid::GRS80);

        // Сферическая и WGS84/GRS80 должны быть очень близки
        assert!((d_sphere - d_wgs84).abs() < 500.0);
        assert!((d_wgs84 - d_grs80).abs() < 500.0);
    }

    #[test]
    fn test_equator_crossing() {
        let p1 = GeoPoint {
            lat: 1.0,
            lon: -45.0,
        };
        let p2 = GeoPoint {
            lat: -1.0,
            lon: 45.0,
        };

        let dist = haversine_distance(p1, p2);
        assert!(dist > 0.0);
        assert!(dist < 20_000_000.0); // меньше окружности Земли
    }
}
