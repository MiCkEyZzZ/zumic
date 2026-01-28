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

/// Тип вычисления расстояния.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DistanceMethod {
    Haversine,
    Vincenty,
    GreatCircle,
    Manhattan,
    Euclidean,
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
        a: 6_378_137.0,
        b: 6_356_752.314_140,
    };

    /// Сфера со средним радиусом Земли
    pub const SPHERE: Self = Self {
        a: 6_371_000.0,
        b: 6_371_000.0,
    };
}

/// Формула Гаверсина.
pub fn haversine_distance(
    p1: GeoPoint,
    p2: GeoPoint,
) -> f64 {
    haversine_distance_sphere(p1, p2, Ellipsoid::SPHERE)
}

/// Флормула Гаверсина (сферическая).
/// ПРИМЕЧАНИЕ: Эллипсоид аппроксимируется сферой с radius = a
pub fn haversine_distance_sphere(
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

/// Формула Винсенти
pub fn vincenty_distance(
    p1: GeoPoint,
    p2: GeoPoint,
) -> Option<f64> {
    vincenty_distance_ellipsoid(p1, p2, Ellipsoid::WGS84)
}

/// Формула Винсенти (сферическая).
pub fn vincenty_distance_ellipsoid(
    p1: GeoPoint,
    p2: GeoPoint,
    ellipsoid: Ellipsoid,
) -> Option<f64> {
    let a = ellipsoid.a;
    let b = ellipsoid.b;
    let f = ellipsoid.f();

    let to_rad = PI / 180.0;
    let lat1 = p1.lat * to_rad;
    let lat2 = p2.lat * to_rad;
    let lon1 = p1.lon * to_rad;
    let lon2 = p2.lon * to_rad;

    let l = lon2 - lon1;

    let u1 = ((1.0 - f) * lat1.tan()).atan();
    let u2 = ((1.0 - f) * lat2.tan()).atan();

    let sin_u1 = u1.sin();
    let cos_u1 = u1.cos();
    let sin_u2 = u2.sin();
    let cos_u2 = u2.cos();

    let mut lambda = l;
    let mut lambda_prev;
    let mut iter_limit = 100;

    let (
        mut sin_sigma,
        mut cos_sigma,
        mut sigma,
        mut sin_alpha,
        mut cos_sq_alpha,
        mut cos_2sigma_m,
    );

    loop {
        let sin_lambda = lambda.sin();
        let cos_lambda = lambda.cos();

        let sin_sq_sigma = (cos_u2 * sin_lambda).powi(2)
            + (cos_u1 * sin_u2 - sin_u1 * cos_u2 * cos_lambda).powi(2);

        if sin_sq_sigma.abs() < 1e-24 {
            return Some(0.0); // Совпадающие точки
        }

        sin_sigma = sin_sq_sigma.sqrt();
        cos_sigma = sin_u1 * sin_u2 + cos_u1 * cos_u2 * cos_lambda;
        sigma = sin_sigma.atan2(cos_sigma);

        sin_alpha = cos_u1 * cos_u2 * sin_lambda / sin_sigma;
        cos_sq_alpha = 1.0 - sin_alpha * sin_alpha;

        cos_2sigma_m = if cos_sq_alpha != 0.0 {
            cos_sigma - 2.0 * sin_u1 * sin_u2 / cos_sq_alpha
        } else {
            0.0
        };

        let c = f / 16.0 * cos_sq_alpha * (4.0 + f * (4.0 - 3.0 * cos_sq_alpha));

        lambda_prev = lambda;
        lambda = l
            + (1.0 - c)
                * f
                * sin_alpha
                * (sigma
                    + c * sin_sigma
                        * (cos_2sigma_m + c * cos_sigma * (-1.0 + 2.0 * cos_2sigma_m.powi(2))));

        if (lambda - lambda_prev).abs() < 1e-12 {
            break;
        }

        iter_limit -= 1;
        if iter_limit == 0 {
            return None; // Не сошлось (обычно для антиподальных точек)
        }
    }

    let u_sq = cos_sq_alpha * (a * a - b * b) / (b * b);
    let big_a = 1.0 + u_sq / 16384.0 * (4096.0 + u_sq * (-768.0 + u_sq * (320.0 - 175.0 * u_sq)));
    let big_b = u_sq / 1024.0 * (256.0 + u_sq * (-128.0 + u_sq * (74.0 - 47.0 * u_sq)));

    let delta_sigma = big_b
        * sin_sigma
        * (cos_2sigma_m
            + big_b / 4.0
                * (cos_sigma * (-1.0 + 2.0 * cos_2sigma_m.powi(2))
                    - big_b / 6.0
                        * cos_2sigma_m
                        * (-3.0 + 4.0 * sin_sigma.powi(2))
                        * (-3.0 + 4.0 * cos_2sigma_m.powi(2))));

    let s = b * big_a * (sigma - delta_sigma);

    Some(s)
}

/// Расстояние по большому кругу
pub fn greate_circle_distance(
    p1: GeoPoint,
    p2: GeoPoint,
) -> f64 {
    let to_rad = PI / 180.0;
    let lat1 = p1.lat * to_rad;
    let lat2 = p2.lat * to_rad;
    let dlon = (p2.lon - p1.lon) * to_rad;

    let cos_c = lat1.sin() * lat2.sin() + lat1.cos() * lat2.cos() * dlon.cos();

    let central_angle = cos_c.clamp(-1.0, 1.0).acos();
    Ellipsoid::SPHERE.a * central_angle
}

#[cfg(test)]
mod tests {
    use super::*;

    const EPSILON_M: f64 = 0.01; // 1см

    #[test]
    fn test_haversine_known_distance() {
        // Кунгур до Перми: ~76 925.28 м по текущей реализации (SPHERE a = 6_371_000 m)
        let kungur = GeoPoint {
            lon: 56.9514,
            lat: 57.4342,
        };
        let perm = GeoPoint {
            lon: 56.2347,
            lat: 58.0105,
        };

        let dist = haversine_distance(kungur, perm);
        assert!((dist - 76_925.28266576723).abs() < 100.0); // ±100 м допуск
    }

    #[test]
    fn test_vincenty_high_distance() {
        // Близкие точки для точности
        let p1 = GeoPoint {
            lon: 13.4,
            lat: 52.5,
        };
        let p2 = GeoPoint {
            lon: 13.401,
            lat: 52.501,
        };

        let vincenty = vincenty_distance(p1, p2).unwrap();
        let haversine = haversine_distance(p1, p2);

        // Винсент должен быть точнее
        assert!((vincenty - haversine).abs() < 1.0); // <1м разница
    }

    #[test]
    fn test_vincenty_convergence() {
        // Почти антиподальные точки
        let p1 = GeoPoint { lon: 0.0, lat: 0.0 };
        let p2 = GeoPoint {
            lon: 179.0,
            lat: 0.0,
        };

        let resilt = vincenty_distance(p1, p2);
        assert!(resilt.is_some());
    }

    #[test]
    fn test_unit_conversions() {
        let meters = 1000.0;

        assert!((DistanceUnit::Kilometers.convert_from_meters(meters) - 1.0).abs() < EPSILON_M);
        assert!((DistanceUnit::Miles.convert_from_meters(meters) - 0.621_371).abs() < 0.001);
        assert!((DistanceUnit::Feet.convert_from_meters(meters) - 3280.84).abs() < 0.1);

        // Round-trip
        let km = DistanceUnit::Kilometers.convert_from_meters(meters);
        let back = DistanceUnit::Kilometers.convert_to_meters(km);
        assert!((back - meters).abs() < EPSILON_M);
    }

    #[test]
    fn test_cross_meridian() {
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
        assert!(dist.abs() < EPSILON_M);
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

        assert!((d1 - d2).abs() < EPSILON_M);
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

        let d_sphere = haversine_distance_sphere(kungur, perm, Ellipsoid::SPHERE);
        let d_wgs84 = haversine_distance_sphere(kungur, perm, Ellipsoid::WGS84);
        let d_grs80 = haversine_distance_sphere(kungur, perm, Ellipsoid::GRS80);

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

    #[test]
    fn test_vincenty_symmetry() {
        let p1 = GeoPoint {
            lat: 57.4342,
            lon: 56.9514,
        }; // Кунгур
        let p2 = GeoPoint {
            lat: 58.0105,
            lon: 56.2347,
        }; // Пермь

        let d1 = vincenty_distance(p1, p2).unwrap();
        let d2 = vincenty_distance(p2, p1).unwrap();

        assert!((d1 - d2).abs() < 1e-6);
    }

    #[test]
    fn test_vincenty_zero_distance() {
        let p = GeoPoint {
            lat: 57.4342,
            lon: 56.9514,
        };
        let dist = vincenty_distance(p, p).unwrap();
        assert!(dist.abs() < 1e-6);
    }

    #[test]
    fn test_vincenty_poles() {
        let kungur = GeoPoint {
            lat: 57.4342,
            lon: 56.9514,
        };
        let south_pole = GeoPoint {
            lat: -90.0,
            lon: 0.0,
        };

        let dist = vincenty_distance(kungur, south_pole).unwrap();
        assert!(dist > 0.0);
        assert!(dist < 20_000_000.0);
    }

    #[test]
    fn test_great_circle_known_distance() {
        // Кунгур -> Пермь
        let kungur = GeoPoint {
            lon: 56.9514,
            lat: 57.4342,
        };
        let perm = GeoPoint {
            lon: 56.2347,
            lat: 58.0105,
        };

        let d_gc = greate_circle_distance(kungur, perm);
        let d_hav = haversine_distance(kungur, perm);

        // Great-circle и haversine на сфере должны совпадать
        assert!((d_gc - d_hav).abs() < 1.0);
    }

    #[test]
    fn test_great_circle_symmetry() {
        let p1 = GeoPoint {
            lon: 20.0,
            lat: 10.0,
        };
        let p2 = GeoPoint {
            lon: 80.0,
            lat: -30.0,
        };

        let d1 = greate_circle_distance(p1, p2);
        let d2 = greate_circle_distance(p2, p1);

        assert!((d1 - d2).abs() < EPSILON_M);
    }

    #[test]
    fn test_great_circle_zero_distance() {
        let p = GeoPoint {
            lon: 90.0,
            lat: 45.0,
        };

        let d = greate_circle_distance(p, p);

        assert!(d.abs() < EPSILON_M);
    }

    #[test]
    fn test_great_circle_poles() {
        let north_pole = GeoPoint {
            lat: 90.0,
            lon: 0.0,
        };
        let south_pole = GeoPoint {
            lat: -90.0,
            lon: 0.0,
        };

        let d = greate_circle_distance(north_pole, south_pole);

        // Полуокружность Земли
        let expected = PI * Ellipsoid::SPHERE.a;
        assert!((d - expected).abs() < 1_000.0);
    }

    #[test]
    fn test_great_circle_equator_quarter() {
        // 90° по экватору
        let p1 = GeoPoint { lat: 0.0, lon: 0.0 };
        let p2 = GeoPoint {
            lat: 0.0,
            lon: 90.0,
        };

        let d = greate_circle_distance(p1, p2);
        let expected = PI * Ellipsoid::SPHERE.a / 2.0;

        assert!((d - expected).abs() < 1_000.0);
    }

    #[test]
    fn test_great_circle_vs_vincenty_reasonable_error() {
        let p1 = GeoPoint {
            lat: 52.5,
            lon: 13.4,
        };
        let p2 = GeoPoint {
            lat: 48.9,
            lon: 2.3,
        };

        let d_gc = greate_circle_distance(p1, p2);
        let d_vin = vincenty_distance(p1, p2).unwrap();

        let diff = (d_gc - d_vin).abs();

        // Для сферической модели ошибка в несколько км допустима
        assert!(diff < 10_000.0, "diff = {} m", diff);
    }

    #[test]
    fn test_great_circle_matches_haversine() {
        let p1 = GeoPoint {
            lat: 52.5,
            lon: 13.4,
        };
        let p2 = GeoPoint {
            lat: 48.9,
            lon: 2.3,
        };

        let d_gc = greate_circle_distance(p1, p2);
        let d_hav = haversine_distance(p1, p2);

        assert!((d_gc - d_hav).abs() < 1.0);
    }

    #[test]
    fn test_great_circle_near_points_precision() {
        // Очень близкие точки — потенциальная проблема acos
        let p1 = GeoPoint {
            lat: 52.0,
            lon: 13.0,
        };
        let p2 = GeoPoint {
            lat: 52.000001,
            lon: 13.000001,
        };

        let d_gc = greate_circle_distance(p1, p2);
        let d_hav = haversine_distance(p1, p2);

        // Здесь haversine численно устойчивее
        assert!((d_gc - d_hav).abs() < 0.5);
    }
}
