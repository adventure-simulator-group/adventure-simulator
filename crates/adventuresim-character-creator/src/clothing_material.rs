use crate::item_catalog_schema::EquipmentMaterial;

pub(crate) fn pbr(material: EquipmentMaterial) -> ([f32; 4], f32, f32) {
    match material {
        EquipmentMaterial::PolishedSteel => ([0.769, 0.776, 0.776, 1.0], 1.0, 0.20),
        EquipmentMaterial::RoughSteel => ([0.769, 0.776, 0.776, 1.0], 1.0, 0.58),
        EquipmentMaterial::OxidizedSteel => ([0.420, 0.275, 0.196, 1.0], 0.0, 0.82),
        EquipmentMaterial::MailSteel => ([0.769, 0.776, 0.776, 1.0], 1.0, 0.42),
        EquipmentMaterial::VegetableTannedLeather => ([0.502, 0.353, 0.231, 1.0], 0.0, 0.58),
        EquipmentMaterial::Linen => ([0.722, 0.663, 0.510, 1.0], 0.0, 0.88),
        EquipmentMaterial::Wool => ([0.561, 0.510, 0.408, 1.0], 0.0, 0.92),
        EquipmentMaterial::QuiltedTextile => ([0.459, 0.416, 0.314, 1.0], 0.0, 0.90),
        EquipmentMaterial::Hardwood => ([0.235, 0.118, 0.047, 1.0], 0.0, 0.72),
        EquipmentMaterial::Lead => ([0.310, 0.322, 0.337, 1.0], 1.0, 0.68),
    }
}
