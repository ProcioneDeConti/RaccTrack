//! International Morse for navaid idents — the shared table for encoding
//! (frontend popup / tests) and decoding the keyed 1020 Hz VOR ident tone.

const TABLE: &[(char, &str)] = &[
    ('A', ".-"), ('B', "-..."), ('C', "-.-."), ('D', "-.."), ('E', "."),
    ('F', "..-."), ('G', "--."), ('H', "...."), ('I', ".."), ('J', ".---"),
    ('K', "-.-"), ('L', ".-.."), ('M', "--"), ('N', "-."), ('O', "---"),
    ('P', ".--."), ('Q', "--.-"), ('R', ".-."), ('S', "..."), ('T', "-"),
    ('U', "..-"), ('V', "...-"), ('W', ".--"), ('X', "-..-"), ('Y', "-.--"),
    ('Z', "--.."), ('0', "-----"), ('1', ".----"), ('2', "..---"),
    ('3', "...--"), ('4', "....-"), ('5', "....."), ('6', "-...."),
    ('7', "--..."), ('8', "---.."), ('9', "----."),
];

/// Morse for a single character (`.`/`-`), or `None` if unsupported. Only the
/// decode direction (`letter_from`) is needed at runtime; this is for tests
/// and any future backend-side rendering.
#[cfg(test)]
pub fn code_for(ch: char) -> Option<&'static str> {
    let up = ch.to_ascii_uppercase();
    TABLE.iter().find(|(c, _)| *c == up).map(|(_, code)| *code)
}

/// The character for a `.`/`-` string, or `None` if it isn't a valid symbol.
pub fn letter_from(code: &str) -> Option<char> {
    TABLE.iter().find(|(_, c)| *c == code).map(|(l, _)| *l)
}

/// Whole-string Morse, characters space-separated — used by tests and any
/// backend-side rendering.
#[cfg(test)]
pub fn encode(text: &str) -> String {
    text.chars()
        .filter_map(code_for)
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips() {
        assert_eq!(encode("PDZ"), ".--. -.. --..");
        assert_eq!(letter_from("-..."), Some('B'));
        assert_eq!(letter_from(".-.-"), None);
    }
}
