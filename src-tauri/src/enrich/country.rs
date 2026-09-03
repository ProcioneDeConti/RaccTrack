//! ICAO 24-bit address -> country of registration.
//! Ranges from ICAO Annex 10 Volume III allocations (subset covering the bulk
//! of global traffic). Returns `None` for unallocated / unknown blocks.

struct Block {
    start: u32,
    end: u32,
    country: &'static str,
}

// Ordered, non-overlapping enough for a linear scan. Not exhaustive.
const BLOCKS: &[Block] = &[
    Block { start: 0x004000, end: 0x0043FF, country: "Zimbabwe" },
    Block { start: 0x006000, end: 0x006FFF, country: "Mozambique" },
    Block { start: 0x008000, end: 0x00FFFF, country: "South Africa" },
    Block { start: 0x010000, end: 0x017FFF, country: "Egypt" },
    Block { start: 0x018000, end: 0x01FFFF, country: "Libya" },
    Block { start: 0x020000, end: 0x027FFF, country: "Morocco" },
    Block { start: 0x028000, end: 0x02FFFF, country: "Tunisia" },
    Block { start: 0x030000, end: 0x0303FF, country: "Botswana" },
    Block { start: 0x032000, end: 0x032FFF, country: "Burundi" },
    Block { start: 0x034000, end: 0x034FFF, country: "Cameroon" },
    Block { start: 0x038000, end: 0x038FFF, country: "Congo" },
    Block { start: 0x03E000, end: 0x03EFFF, country: "Gabon" },
    Block { start: 0x040000, end: 0x040FFF, country: "Ethiopia" },
    Block { start: 0x044000, end: 0x044FFF, country: "Ghana" },
    Block { start: 0x048000, end: 0x048FFF, country: "Guinea" },
    Block { start: 0x04C000, end: 0x04CFFF, country: "Ivory Coast" },
    Block { start: 0x050000, end: 0x050FFF, country: "Kenya" },
    Block { start: 0x058000, end: 0x058FFF, country: "Liberia" },
    Block { start: 0x060000, end: 0x060FFF, country: "Madagascar" },
    Block { start: 0x068000, end: 0x068FFF, country: "Mali" },
    Block { start: 0x070000, end: 0x070FFF, country: "Mauritania" },
    Block { start: 0x074000, end: 0x074FFF, country: "Mauritius" },
    Block { start: 0x078000, end: 0x078FFF, country: "Niger" },
    Block { start: 0x07C000, end: 0x07CFFF, country: "Nigeria" },
    Block { start: 0x084000, end: 0x084FFF, country: "Rwanda" },
    Block { start: 0x088000, end: 0x088FFF, country: "Senegal" },
    Block { start: 0x094000, end: 0x094FFF, country: "Sudan" },
    Block { start: 0x098000, end: 0x098FFF, country: "Tanzania" },
    Block { start: 0x09C000, end: 0x09CFFF, country: "Chad" },
    Block { start: 0x0A0000, end: 0x0A7FFF, country: "Algeria" },
    Block { start: 0x0A8000, end: 0x0A8FFF, country: "Uganda" },
    Block { start: 0x0AC000, end: 0x0ACFFF, country: "DR Congo" },
    Block { start: 0x0B0000, end: 0x0B0FFF, country: "Zambia" },
    Block { start: 0x0C0000, end: 0x0C3FFF, country: "Angola" },
    Block { start: 0x0D0000, end: 0x0D7FFF, country: "Mexico" },
    Block { start: 0x100000, end: 0x1FFFFF, country: "Russia" },
    Block { start: 0x201000, end: 0x2013FF, country: "Namibia" },
    Block { start: 0x300000, end: 0x33FFFF, country: "Italy" },
    Block { start: 0x340000, end: 0x37FFFF, country: "Spain" },
    Block { start: 0x380000, end: 0x3BFFFF, country: "France" },
    Block { start: 0x3C0000, end: 0x3FFFFF, country: "Germany" },
    Block { start: 0x400000, end: 0x43FFFF, country: "United Kingdom" },
    Block { start: 0x440000, end: 0x447FFF, country: "Austria" },
    Block { start: 0x448000, end: 0x44FFFF, country: "Belgium" },
    Block { start: 0x450000, end: 0x457FFF, country: "Bulgaria" },
    Block { start: 0x458000, end: 0x45FFFF, country: "Denmark" },
    Block { start: 0x460000, end: 0x467FFF, country: "Finland" },
    Block { start: 0x468000, end: 0x46FFFF, country: "Greece" },
    Block { start: 0x470000, end: 0x477FFF, country: "Hungary" },
    Block { start: 0x478000, end: 0x47FFFF, country: "Norway" },
    Block { start: 0x480000, end: 0x487FFF, country: "Netherlands" },
    Block { start: 0x488000, end: 0x48FFFF, country: "Poland" },
    Block { start: 0x490000, end: 0x497FFF, country: "Portugal" },
    Block { start: 0x498000, end: 0x49FFFF, country: "Czechia" },
    Block { start: 0x4A0000, end: 0x4A7FFF, country: "Romania" },
    Block { start: 0x4A8000, end: 0x4AFFFF, country: "Sweden" },
    Block { start: 0x4B0000, end: 0x4B7FFF, country: "Switzerland" },
    Block { start: 0x4B8000, end: 0x4BFFFF, country: "Turkey" },
    Block { start: 0x4C0000, end: 0x4C7FFF, country: "Yugoslavia/Serbia" },
    Block { start: 0x4CA000, end: 0x4CAFFF, country: "Ireland" },
    Block { start: 0x4CC000, end: 0x4CCFFF, country: "Iceland" },
    Block { start: 0x500000, end: 0x5003FF, country: "Slovenia" },
    Block { start: 0x501000, end: 0x5013FF, country: "Croatia" },
    Block { start: 0x501C00, end: 0x501FFF, country: "Latvia" },
    Block { start: 0x502C00, end: 0x502FFF, country: "Lithuania" },
    Block { start: 0x503C00, end: 0x503FFF, country: "Armenia" },
    Block { start: 0x506C00, end: 0x506FFF, country: "Estonia" },
    Block { start: 0x508000, end: 0x50FFFF, country: "Ukraine" },
    Block { start: 0x510000, end: 0x5103FF, country: "Belarus" },
    Block { start: 0x511C00, end: 0x511FFF, country: "Moldova" },
    Block { start: 0x513C00, end: 0x513FFF, country: "Cyprus" },
    Block { start: 0x515C00, end: 0x515FFF, country: "Georgia" },
    Block { start: 0x516C00, end: 0x516FFF, country: "Azerbaijan" },
    Block { start: 0x600000, end: 0x6003FF, country: "Armenia" },
    Block { start: 0x600800, end: 0x600BFF, country: "Azerbaijan" },
    Block { start: 0x601000, end: 0x6013FF, country: "Kyrgyzstan" },
    Block { start: 0x680000, end: 0x6800FF, country: "Turkmenistan" },
    Block { start: 0x6A0000, end: 0x6A0FFF, country: "Uzbekistan" },
    Block { start: 0x700000, end: 0x700FFF, country: "Afghanistan" },
    Block { start: 0x702000, end: 0x702FFF, country: "Bangladesh" },
    Block { start: 0x708000, end: 0x708FFF, country: "Bahrain" },
    Block { start: 0x710000, end: 0x717FFF, country: "Saudi Arabia" },
    Block { start: 0x718000, end: 0x71FFFF, country: "Iran" },
    Block { start: 0x720000, end: 0x727FFF, country: "Lebanon" },
    Block { start: 0x728000, end: 0x72FFFF, country: "Jordan" },
    Block { start: 0x730000, end: 0x737FFF, country: "Kuwait" },
    Block { start: 0x738000, end: 0x73FFFF, country: "Oman" },
    Block { start: 0x740000, end: 0x747FFF, country: "Qatar" },
    Block { start: 0x748000, end: 0x74FFFF, country: "United Arab Emirates" },
    Block { start: 0x750000, end: 0x757FFF, country: "Yemen" },
    Block { start: 0x758000, end: 0x75FFFF, country: "Pakistan" },
    Block { start: 0x760000, end: 0x767FFF, country: "Iraq" },
    Block { start: 0x768000, end: 0x76FFFF, country: "Israel" },
    Block { start: 0x770000, end: 0x777FFF, country: "Syria" },
    Block { start: 0x780000, end: 0x7BFFFF, country: "China" },
    Block { start: 0x7C0000, end: 0x7FFFFF, country: "Australia" },
    Block { start: 0x800000, end: 0x83FFFF, country: "India" },
    Block { start: 0x840000, end: 0x87FFFF, country: "Japan" },
    Block { start: 0x880000, end: 0x887FFF, country: "Thailand" },
    Block { start: 0x888000, end: 0x88FFFF, country: "Viet Nam" },
    Block { start: 0x890000, end: 0x890FFF, country: "China (Hong Kong/Macau)" },
    Block { start: 0x894000, end: 0x894FFF, country: "Cambodia" },
    Block { start: 0x8A0000, end: 0x8A7FFF, country: "Indonesia" },
    Block { start: 0x8B0000, end: 0x8B7FFF, country: "South Korea" },
    Block { start: 0x8C0000, end: 0x8C7FFF, country: "Chinese Taipei" },
    Block { start: 0x900000, end: 0x9003FF, country: "Marshall Islands" },
    Block { start: 0x901000, end: 0x9013FF, country: "Cook Islands" },
    Block { start: 0xA00000, end: 0xAFFFFF, country: "United States" },
    Block { start: 0xC00000, end: 0xC3FFFF, country: "Canada" },
    Block { start: 0xC80000, end: 0xC87FFF, country: "New Zealand" },
    Block { start: 0xC88000, end: 0xC88FFF, country: "Fiji" },
    Block { start: 0xC8A000, end: 0xC8A3FF, country: "Nauru" },
    Block { start: 0xC8C000, end: 0xC8C3FF, country: "Papua New Guinea" },
    Block { start: 0xD00000, end: 0xD3FFFF, country: "Germany" },
    Block { start: 0xE00000, end: 0xE3FFFF, country: "Argentina" },
    Block { start: 0xE40000, end: 0xE7FFFF, country: "Brazil" },
    Block { start: 0xE80000, end: 0xE80FFF, country: "Chile" },
    Block { start: 0xE84000, end: 0xE84FFF, country: "Colombia" },
    Block { start: 0xE88000, end: 0xE88FFF, country: "Costa Rica" },
    Block { start: 0xE8C000, end: 0xE8CFFF, country: "Cuba" },
    Block { start: 0xE90000, end: 0xE90FFF, country: "Ecuador" },
    Block { start: 0xE94000, end: 0xE94FFF, country: "Guatemala" },
    Block { start: 0xE98000, end: 0xE98FFF, country: "Guyana" },
    Block { start: 0xE9C000, end: 0xE9CFFF, country: "Panama" },
    Block { start: 0xEA0000, end: 0xEA0FFF, country: "Paraguay" },
    Block { start: 0xEA4000, end: 0xEA4FFF, country: "Peru" },
    Block { start: 0xEA8000, end: 0xEA8FFF, country: "El Salvador" },
    Block { start: 0xEAC000, end: 0xEACFFF, country: "Uruguay" },
    Block { start: 0xEB0000, end: 0xEB0FFF, country: "Venezuela" },
];

pub fn country_for_hex(hex: &str) -> Option<&'static str> {
    let addr = u32::from_str_radix(hex.trim(), 16).ok()?;
    BLOCKS
        .iter()
        .find(|b| addr >= b.start && addr <= b.end)
        .map(|b| b.country)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_countries() {
        assert_eq!(country_for_hex("a835af"), Some("United States"));
        assert_eq!(country_for_hex("C078A0"), Some("Canada"));
        assert_eq!(country_for_hex("3C6444"), Some("Germany"));
        assert_eq!(country_for_hex("406B1C"), Some("United Kingdom"));
        assert_eq!(country_for_hex("zzzz"), None);
    }
}
