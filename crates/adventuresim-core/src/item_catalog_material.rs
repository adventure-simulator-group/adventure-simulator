use crate::item_catalog_schema::EquipmentMaterial;

impl EquipmentMaterial {
    #[must_use]
    pub const fn is_metal(self) -> bool {
        matches!(
            self,
            Self::PolishedSteel
                | Self::RoughSteel
                | Self::OxidizedSteel
                | Self::MailSteel
                | Self::Lead
        )
    }
}
