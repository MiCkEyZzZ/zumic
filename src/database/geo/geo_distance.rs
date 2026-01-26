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
        let london = GeoPoint {
            lon: -0.1278,
            lat: 51.5074,
        };
        let paris = GeoPoint {
            lon: 2.3522,
            lat: 48.8566,
        };

        let dist = haversine_distance(london, paris);
        assert!((dist - 343_500.0).abs() < 5000.0); // +- 5км допуск
    }
}
