pub(crate) fn is_equipment_material_name(value: Option<&str>) -> bool {
    value.is_some_and(|value| {
        matches!(
            value,
            "polished_steel"
                | "rough_steel"
                | "oxidized_steel"
                | "mail_steel"
                | "vegetable_tanned_leather"
                | "linen"
                | "wool"
                | "quilted_textile"
                | "hardwood"
                | "lead"
        )
    })
}
