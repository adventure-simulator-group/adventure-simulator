//! Macro-free coordinate policy used by investigation route validation.

use crate::strategic::coordinate_distance_e7_m;

const AREA_RADIUS_TOLERANCE_M: u64 = 1;

pub(super) fn coordinate_area_contains_e7(
    center_longitude_e7: i32,
    center_latitude_e7: i32,
    radius_m: u32,
    area_coordinates_are_geographic: bool,
    longitude_e7: i32,
    latitude_e7: i32,
    point_coordinates_are_geographic: bool,
) -> bool {
    if area_coordinates_are_geographic != point_coordinates_are_geographic {
        return false;
    }
    coordinate_distance_e7_m(
        center_longitude_e7,
        center_latitude_e7,
        longitude_e7,
        latitude_e7,
        area_coordinates_are_geographic,
    )
    .is_some_and(|distance_m| {
        distance_m <= u64::from(radius_m).saturating_add(AREA_RADIUS_TOLERANCE_M)
    })
}
