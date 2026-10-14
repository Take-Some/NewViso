//! Stable semantic identifiers from the supplied PedType.h reference.
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
#[serde(try_from = "u8", into = "u8")]
pub enum PedType {
    Player0 = 0,
    Player1 = 1,
    NetworkPlayer = 2,
    Player2 = 3,
    CivilianMale = 4,
    CivilianFemale = 5,
    Cop = 6,
    GangAlbanian = 7,
    GangBiker1 = 8,
    GangBiker2 = 9,
    GangItalian = 10,
    GangRussian = 11,
    GangRussian2 = 12,
    GangIrish = 13,
    GangJamaican = 14,
    GangAfricanAmerican = 15,
    GangKorean = 16,
    GangChineseJapanese = 17,
    GangPuertoRican = 18,
    Dealer = 19,
    Medic = 20,
    Fire = 21,
    Criminal = 22,
    Bum = 23,
    Prostitute = 24,
    Special = 25,
    Mission = 26,
    Swat = 27,
    Animal = 28,
    Army = 29,
}

impl PedType {
    pub fn code(self) -> u8 {
        self as u8
    }
    pub fn is_player(self) -> bool {
        self.code() <= 3
    }
    pub fn is_female(self) -> bool {
        matches!(self, Self::CivilianFemale | Self::Prostitute)
    }
    pub fn is_civilian(self) -> bool {
        matches!(self, Self::CivilianMale | Self::CivilianFemale)
    }
    pub fn is_law_enforcement(self) -> bool {
        matches!(self, Self::Cop | Self::Swat | Self::Army)
    }
    pub fn is_emergency(self) -> bool {
        matches!(self, Self::Medic | Self::Fire)
    }
    pub fn is_gang(self) -> bool {
        (7..=18).contains(&self.code())
    }
}

impl Default for PedType {
    fn default() -> Self {
        Self::CivilianMale
    }
}
impl From<PedType> for u8 {
    fn from(value: PedType) -> Self {
        value.code()
    }
}
impl TryFrom<u8> for PedType {
    type Error = String;
    fn try_from(value: u8) -> Result<Self, Self::Error> {
        const TYPES: [PedType; 30] = [
            PedType::Player0,
            PedType::Player1,
            PedType::NetworkPlayer,
            PedType::Player2,
            PedType::CivilianMale,
            PedType::CivilianFemale,
            PedType::Cop,
            PedType::GangAlbanian,
            PedType::GangBiker1,
            PedType::GangBiker2,
            PedType::GangItalian,
            PedType::GangRussian,
            PedType::GangRussian2,
            PedType::GangIrish,
            PedType::GangJamaican,
            PedType::GangAfricanAmerican,
            PedType::GangKorean,
            PedType::GangChineseJapanese,
            PedType::GangPuertoRican,
            PedType::Dealer,
            PedType::Medic,
            PedType::Fire,
            PedType::Criminal,
            PedType::Bum,
            PedType::Prostitute,
            PedType::Special,
            PedType::Mission,
            PedType::Swat,
            PedType::Animal,
            PedType::Army,
        ];
        TYPES
            .get(usize::from(value))
            .copied()
            .ok_or_else(|| format!("invalid ped type {value}; expected 0..29"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn all_source_codes_roundtrip_and_keep_type_classification() {
        for code in 0..30 {
            let kind = PedType::try_from(code).unwrap();
            assert_eq!(serde_json::to_value(kind).unwrap(), code);
            assert_eq!(
                serde_json::from_value::<PedType>(serde_json::json!(code)).unwrap(),
                kind
            );
        }
        assert!(PedType::Cop.is_law_enforcement());
        assert!(PedType::Army.is_law_enforcement());
        assert!(PedType::Prostitute.is_female());
        assert!(PedType::Medic.is_emergency());
        assert!(serde_json::from_value::<PedType>(serde_json::json!(255)).is_err());
    }
}
