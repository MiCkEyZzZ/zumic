/// Едицинцы измерения расстояния.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DistanceUnit {
    Meters,
    Kilometers,
    Miles,
    Feet,
    NauticalMiles,
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

    /// Конветирует из единицы в метры.
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
