//! Overlapping collections: a venue can belong to more than one racing story.
//! Historical membership and layout caveats are recorded in the endurance notes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum Collection {
    #[default]
    All,
    Dhh2014,
    Endurance,
    GrandPrix,
    Heritage,
}

impl Collection {
    pub(super) const ALL: [Self; 5] = [
        Self::All,
        Self::Dhh2014,
        Self::Endurance,
        Self::GrandPrix,
        Self::Heritage,
    ];

    pub(super) fn name(self) -> &'static str {
        crate::text::t(match self {
            Self::All => "collection.all",
            Self::Dhh2014 => "collection.dhh",
            Self::Endurance => "collection.endurance",
            Self::GrandPrix => "collection.grand_prix",
            Self::Heritage => "collection.heritage",
        })
    }

    pub(super) fn description(self) -> &'static str {
        crate::text::t(match self {
            Self::All => "menu.circuits_sub",
            Self::Dhh2014 => "collection.dhh_about",
            Self::Endurance => "collection.endurance_about",
            Self::GrandPrix => "collection.grand_prix_about",
            Self::Heritage => "collection.heritage_about",
        })
    }

    pub(super) fn contains(self, id: &str) -> bool {
        match self {
            Self::All => true,
            Self::Dhh2014 => DHH_2014.contains(&id),
            Self::Endurance => ENDURANCE.contains(&id),
            Self::GrandPrix => GRAND_PRIX.contains(&id),
            Self::Heritage => HERITAGE.contains(&id),
        }
    }

    pub(super) fn step(self, by: i32) -> Self {
        let at = Self::ALL.iter().position(|c| *c == self).unwrap() as i32;
        Self::ALL[(at + by).rem_euclid(Self::ALL.len() as i32) as usize]
    }
}

/// Final 2014 WEC calendar, in race order. The menu does not alter track IDs,
/// saved times or the separately frozen weekly challenge roster.
pub(super) const DHH_2014: &[&str] = &[
    "silverstone",
    "spa-francorchamps",
    "le-mans",
    "americas",
    "fuji",
    "shanghai",
    "bahrain",
    "interlagos",
];

const ENDURANCE: &[&str] = &[
    "silverstone",
    "spa-francorchamps",
    "le-mans",
    "americas",
    "fuji",
    "shanghai",
    "bahrain",
    "interlagos",
    "sebring",
    "nurburgring",
    "hermanos-rodriguez",
    "algarve",
    "monza",
    "losail",
    "imola",
    "laguna-seca",
    "lime-rock",
    "long-beach",
    "mid-ohio",
    "mosport",
    "road-america",
    "road-atlanta",
    "virginia-international-raceway",
];

const GRAND_PRIX: &[&str] = &[
    "albert-park",
    "algarve",
    "americas",
    "bahrain",
    "baku",
    "barcelona-catalunya",
    "buenos-aires",
    "estoril",
    "fuji",
    "gilles-villeneuve",
    "hermanos-rodriguez",
    "hockenheim",
    "hungaroring",
    "imola",
    "indianapolis",
    "interlagos",
    "istanbul-park",
    "jacarepagua",
    "jeddah",
    "kyalami",
    "las-vegas",
    "losail",
    "madring",
    "magny-cours",
    "marina-bay",
    "miami",
    "monaco",
    "monza",
    "mugello",
    "nurburgring",
    "paul-ricard",
    "red-bull-ring",
    "sepang",
    "shanghai",
    "silverstone",
    "sochi",
    "spa-francorchamps",
    "suzuka",
    "watkins-glen",
    "yas-marina",
    "zandvoort",
];

/// Classic venues, not a claim that the game recreates their period layouts.
const HERITAGE: &[&str] = &[
    "buenos-aires",
    "estoril",
    "hockenheim",
    "indianapolis",
    "jacarepagua",
    "kyalami",
    "magny-cours",
    "nurburgring",
    "watkins-glen",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collections_reference_unique_available_circuits_and_can_overlap() {
        let circuits = crate::track::all_circuits();
        for ids in [DHH_2014, ENDURANCE, GRAND_PRIX, HERITAGE] {
            let mut seen = std::collections::HashSet::new();
            for &id in ids {
                assert!(seen.insert(id), "duplicate collection entry: {id}");
                assert!(circuits.iter().any(|c| c.id == id), "missing circuit: {id}");
            }
        }
        for &id in DHH_2014 {
            assert!(Collection::Endurance.contains(id));
        }
        assert!(Collection::GrandPrix.contains("spa-francorchamps"));
        assert!(Collection::Endurance.contains("spa-francorchamps"));
        assert!(!Collection::GrandPrix.contains("le-mans"));
    }
}
