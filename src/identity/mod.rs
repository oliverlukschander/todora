//! Shared player identity rules and the country catalogue.
pub(crate) mod countries;

/// Ignore case and separators when protecting a public handle. Full-width
/// ASCII is folded too; the original spelling is retained for display.
pub(crate) fn name_key(name: &str) -> String {
    name.chars()
        .map(|c| {
            if ('\u{ff01}'..='\u{ff5e}').contains(&c) {
                char::from_u32(c as u32 - 0xfee0).unwrap()
            } else {
                c
            }
        })
        .flat_map(char::to_lowercase)
        .filter(|c| c.is_alphanumeric())
        .collect()
}

pub(crate) fn reserved(name: &str) -> bool {
    let key = name_key(name);
    include_str!("reserved.txt")
        .lines()
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .any(|line| name_key(line) == key)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn handles_cannot_be_claimed_by_changing_case_spacing_or_width() {
        for name in [
            "DHH",
            "olukschander",
            "d.h.h",
            "D H H",
            "ＤＨＨ",
            "IAMdothash",
            "acelogic",
            "Hancore-Linux",
        ] {
            assert!(reserved(name), "{name}");
        }
        assert!(!reserved("Tobias"));
        assert!(!reserved("DHH fan"));
    }
}
