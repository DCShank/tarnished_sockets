pub fn encode(bytes: Vec<u8>) -> String {
    let mut result = String::new();

    for chunk in bytes.chunks(3) {
        let first_byte = chunk.first().unwrap(); // A chunk will always have at least one value
        let second_byte = chunk.get(1);
        let third_byte = chunk.get(2);

        let first = translate_char(first_byte >> 2);
        let second =
            translate_char((0x30 as u8 & (first_byte << 4)) | second_byte.unwrap_or(&0) >> 4);
        let third = match (second_byte, third_byte) {
            (Some(second_byte), Some(third_byte)) => {
                translate_char(0x3F as u8 & (second_byte << 2 | third_byte >> 6))
            }
            (Some(second_byte), None) => {
                translate_char(0x3F as u8 & (second_byte << 2 | 0x00 as u8))
            }
            (None, _) => '=',
        };
        let fourth = third_byte.map_or('=', |byte| translate_char(0x3F as u8 & byte));

        result.push(first);
        result.push(second);
        result.push(third);
        result.push(fourth);
    }
    result
}

fn translate_char(input: u8) -> char {
    match input {
        0..=25 => (input + 65) as char,
        26..=51 => (97 - 26 + input) as char,
        52..=61 => (input - 4) as char,
        62 => '+',
        63 => '/',
        64.. => panic!("Received {}", input),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encoding_works() {
        let input: Vec<u8> = "Many hands make light work.".as_bytes().into();
        let encoded = encode(input);
        let expected = "TWFueSBoYW5kcyBtYWtlIGxpZ2h0IHdvcmsu";

        assert_eq!(encoded, expected);
    }

    #[test]
    fn encoding_padding_works() {
        let input: Vec<u8> = "light work.".as_bytes().into();
        let encoded = encode(input);
        let expected = "bGlnaHQgd29yay4=";

        assert_eq!(encoded, expected);
    }

    #[test]
    fn encoding_padding_two_bytes_works() {
        let input: Vec<u8> = "light work".as_bytes().into();
        let encoded = encode(input);
        let expected = "bGlnaHQgd29yaw==";

        assert_eq!(encoded, expected);
    }
}
