

// initialize variables
const HASH_0_INIT: u32 = 0x67452301;
const HASH_1_INIT: u32 = 0xEFCDAB89;
const HASH_2_INIT: u32 = 0x98BADCFE;
const HASH_3_INIT: u32 = 0x10325476;
const HASH_4_INIT: u32 = 0xC3D2E1F0;

pub fn calculate_sha1(message: &str) -> Vec<u8> {
    let mut hash_0 = HASH_0_INIT;
    let mut hash_1 = HASH_1_INIT;
    let mut hash_2 = HASH_2_INIT;
    let mut hash_3 = HASH_3_INIT;
    let mut hash_4 = HASH_4_INIT;

    let mut message : Vec<u8> = message.into();
    // message length in bits
    let original_message_length: u64 = TryInto::<u64>::try_into(message.len()).unwrap() * 8;

    // Pre-processing
    // we append a '1'. Because this is only operating on strings of bytes, and in the next step we
    // need to get it to some number of bytes that is 448 (mod 512), we actually append 0x80
    message.push(0x80);

    // append 0's until we hit a length that is 448 mod 512
    while message.len() % (512 / 8) != (448 / 8) {
        message.push(0x00);
    }

    // original_message_length has a length of 64 bits, so this extends messages len to 0 mod 512
    message.extend(original_message_length.to_be_bytes());

    // loop over the modified message in chunks of 512 bits
    for chunk in message.chunks(512 / 8) {

        // Do the message schedule stuff
        let mut words: Vec<u32> = chunk
            .chunks(4)
            .map(|word_chunk| u32::from_be_bytes(word_chunk.try_into().unwrap()))
            .collect();

        // add the remaining words from 16 to 79
        for i in 16..=79 {
            unsafe {
            let word = (words.get_unchecked(i - 3)
                    ^ words.get_unchecked(i - 8)
                    ^ words.get_unchecked(i - 14)
                    ^ words.get_unchecked(i - 16)).rotate_left(1);
            words.push(word);
            }
        }

        let (mut a, mut b, mut c, mut d, mut e) = (hash_0, hash_1, hash_2, hash_3, hash_4);

        for i in 0..=79 {
            //let (mut f, mut k) = (0, 0);
            let (f, k) = match i {
                0..=19 => { ((b & c) | ((!b) & d), 0x5A827999) }
                20..=39 => { (b ^ c ^ d, 0x6ED9EBA1) }
                40..=59 => { ((b & c) | (b & d) | (c & d), 0x8F1BBCDC) }
                60..=79 => { (b ^ c ^ d, 0xCA62C1D6) }
                _ => { panic!() }
            };

            let temp = a.rotate_left(5)
                        .wrapping_add(f)
                        .wrapping_add(e)
                        .wrapping_add(k)
                        .wrapping_add(*words.get(i).unwrap());
            e = d;
            d = c;
            c = b.rotate_left(30);
            b = a;
            a = temp;
        }

        hash_0 = hash_0.wrapping_add(a);
        hash_1 = hash_1.wrapping_add(b);
        hash_2 = hash_2.wrapping_add(c);
        hash_3 = hash_3.wrapping_add(d);
        hash_4 = hash_4.wrapping_add(e);
    }

    let mut hash: Vec<u8> = Vec::new();
    hash.extend(hash_0.to_be_bytes());
    hash.extend(hash_1.to_be_bytes());
    hash.extend(hash_2.to_be_bytes());
    hash.extend(hash_3.to_be_bytes());
    hash.extend(hash_4.to_be_bytes());


    hash
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha1_works() {
        let input = "The quick brown fox jumps over the lazy cog";
        let sha1_output = calculate_sha1(Into::into(input));

        let expected_output: Vec<u8> = [
            0xde,0x9f,0x2c,0x7f,0xd2,
            0x5e,0x1b,0x3a,0xfa,0xd3,
            0xe8,0x5a,0x0b,0xd1,0x7d,
            0x9b,0x10,0x0d,0xb4,0xb3].to_vec();

        assert_eq!(sha1_output, expected_output);
    }

    #[test]
    fn sha1_empty() {
        let input = "";
        let sha1_output = calculate_sha1(Into::into(input));

        let expected_output: Vec<u8> = [
            0xda,0x39,0xa3,0xee,0x5e,
            0x6b,0x4b,0x0d,0x32,0x55,
            0xbf,0xef,0x95,0x60,0x18,
            0x90,0xaf,0xd8,0x07,0x09].to_vec();

        assert_eq!(sha1_output, expected_output);
    }

    #[test]
    fn bytes_into_u32() {
        let bytes: Vec<u8> = vec![0x01, 0x02, 0x03, 0x04];
        let chunk = bytes.chunks(4);
        let value: Vec<u32> = chunk
            .map(|word_chunk| u32::from_be_bytes(word_chunk.try_into().unwrap())).collect();
        assert_eq!(*value.get(0).unwrap(), 0x01020304 as u32);
    }


}
