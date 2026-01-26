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
