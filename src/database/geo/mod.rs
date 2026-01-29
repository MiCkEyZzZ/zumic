pub mod geo_base;
pub mod geo_distance;
pub mod geo_hash;
pub mod geo_rtree;

// Publicly re-export all error types and functions from the submodules to
// simplify access from external code.
pub use geo_base::*;
pub use geo_hash::*;
pub use geo_rtree::*;

pub const GEO_VERSION: &str = "0.2.0";

#[derive(Debug, Clone)]
pub struct GeoModuleStats {
    pub point_count: usize,
    pub rtree_stats: TreeStats,
    pub geohash_stats: GeohashStats,
    pub version: String,
}

impl GeoSet {
    pub fn module_stats(&self) -> GeoModuleStats {
        GeoModuleStats {
            point_count: self.len(),
            rtree_stats: self.index_stats(),
            geohash_stats: self.geohash_stats(),
            version: GEO_VERSION.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::geo_distance::{DistanceMethod, DistanceUnit, Ellipsoid};

    #[test]
    fn test_module_exports() {
        // Проверяем, что все основные типы доступны
        let _gs = GeoSet::new();
        let _point = GeoPoint { lon: 0.0, lat: 0.0 };
        let _gh = Geohash::encode(_point, GeohashPrecision::High);
        let _bbox = BoundingBox::new(-1.0, 1.0, -1.0, 1.0);
        let _rtree = RTree::new();
        let _method = DistanceMethod::Vincenty;
        let _unit = DistanceUnit::Kilometers;
        let _ellipsoid = Ellipsoid::WGS84;
    }

    #[test]
    fn test_version_tracking() {
        let gs = GeoSet::new();
        let stats = gs.module_stats();
        assert_eq!(stats.version, "0.2.0");
    }
}
