// ========< CONVERSION UTILITIES >========
/// Converts unsigned integer to base32 string.
pub fn to_base32(value: u64) -> String {
    let mut value = value;
    let mut result = String::new();
    let alphabet = "0123456789abcdefghijklmnopqrstuv";
    let base = alphabet.len() as u64;

    while value > 0 {
        let index = (value % base) as usize;
        result.push(alphabet.chars().nth(index).unwrap());
        value /= base;
    }

    if result == "" {
        result.push('0');
    }

    result.chars().rev().collect()
}

/// Converts base32 string to unsigned integer.
pub fn from_base32(value: &str) -> u64 {
    let mut result = 0;
    let alphabet = "0123456789abcdefghijklmnopqrstuv";
    let base = alphabet.len() as u64;

    for (i, c) in value.chars().rev().enumerate() {
        let index = alphabet.find(c).unwrap();
        result += index as u64 * base.pow(i as u32);
    }

    result
}

/// Allows for easy conversion into base32
pub trait ToBase32 {
    fn to_base32(&self) -> String;
    fn from_base32(value: &str) -> Self;
}

impl ToBase32 for u64 {
    fn to_base32(&self) -> String {
        to_base32(*self)
    }

    fn from_base32(value: &str) -> Self {
        from_base32(value)
    }
}

impl ToBase32 for u128 {
    fn to_base32(&self) -> String {
        format!("{}.{}",
            ((self >> 64) as u64).to_base32(),
            ((self & 0xFFFFFFFFFFFFFFFF) as u64).to_base32()
        )
    }

    fn from_base32(value: &str) -> Self {
        let mut split = value.split('.');
        let first = u64::from_base32(split.next().unwrap());
        let second = u64::from_base32(split.next().unwrap());

        (first as u128) << 64 | (second as u128)
    }
}

/// Alphabet used for base255 conversion.
/// It was chosen to be as readable as possible.
const BASE_255_ALPHABET: &str = "0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz!\"#$%&'()+,-./:;<=>?@[]^{}~‰£¤¥¦§«¬²³µÀÁÂÃÄÅÆÇÈÉÊËÌÍÎÏÐÑÒØßàáâãäåæçèéêëìþÿǷǾǿɅɆɄɃȽȾȺȸȹɎʘʗʖʕʔʓʒʑʊʇʆʁʂϠϡϢϭϱϺϻϿϾϼ◔◍◎◐◑◒◓◚◛◳◲◱◰◯◿◜◝◞◟◠◡◉◊▣▤▥▦▧▨▩▚▙▜▛▝▞▟▂▃▄▅▆▇█▉▊▋▌▍░▒▓①②③④⑤⑥⑦⑧⑨⑩⑪⑫⑬⑭⑮ⒶⒷⒸⒹⒺⒻⒼⒽⒾⒿ⑴⑵⑶⑷⑸⑹‹";

pub fn byte_to_base_255(byte: u8) -> char {
    BASE_255_ALPHABET.chars().nth(byte as usize).unwrap()
}

pub fn base_255_to_byte(c: char) -> u8 {
    for (i, b) in BASE_255_ALPHABET.chars().enumerate() {
        if b == c {
            return i as u8;
        }
    }
    return 0;
}

#[cfg(test)]
mod test_utils {
    #[test]
    fn to_base32() {
        let value = super::to_base32(1234567890);
        assert_eq!(value, "14pc0mi");
    }

    #[test]
    fn from_base32() {
        let value = super::from_base32("14pc0mi");
        assert_eq!(value, 1234567890);
    }

    #[test]
    fn base32_zero() {
        let value = super::to_base32(0);
        assert_eq!(value, "0");

        let value = super::from_base32("0");
        assert_eq!(value, 0);
    }

    #[test]
    fn to_base256() {
        let value = super::byte_to_base_255(255);
        assert_eq!(value, '‹');
    }

    #[test]
    fn random_conversions_b256() {
        for i in 0..255 {
            let c = super::byte_to_base_255(i);
            println!("{} -> {}", i, c);
            let b = super::base_255_to_byte(c);
            println!("{} <- {}", b, c);
            assert_eq!(i, b);
        }
    }
}


// ========< MASK >========
#[derive(Clone, Debug)]
pub struct BitMask<const S: usize> {
    mask: [u8; S]
}

impl<const S: usize> BitMask<S> {
    pub const fn new() -> Self {
        Self {
            mask: [0; S]
        }
    }

    pub fn from_bytes(bytes: &[u8]) -> Self {
        let mut mask = Self::new();

        for (i, byte) in bytes.iter().enumerate() {
            mask.mask[i] = *byte;
        }

        mask
    }

    pub fn set(&mut self, index: usize, value: bool) {
        let byte = index / 8;
        let bit = index % 8;

        if value {
            self.mask[byte] |= 1 << bit;
        } else {
            self.mask[byte] &= !(1 << bit);
        }
    }

    pub fn get(&self, index: usize) -> bool {
        let byte = index / 8;
        let bit = index % 8;

        self.mask[byte] & (1 << bit) != 0
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.mask
    }

    pub fn from_hex(value: &str) -> Self {
        let mut mask = Self::new();

        for m in (0..value.len()).step_by(2) {
            let byte = u8::from_str_radix(&value[m..m + 2], 16).unwrap();
            mask.mask[m / 2] = byte;
        }

        mask
    }
}

#[cfg(test)]
mod test_bitmask {
    #[test]
    fn test_bitmask() {
        let mut mask = super::BitMask::<1>::new();
        mask.set(0, true);
        mask.set(1, true);
        mask.set(2, true);
        mask.set(3, true);
        mask.set(4, true);
        mask.set(5, true);
        mask.set(6, true);
        mask.set(7, true);

        assert_eq!(mask.mask[0], 0b11111111);

        mask.set(0, false);
        mask.set(1, false);
        mask.set(2, false);
        mask.set(3, false);
        mask.set(4, false);
        mask.set(5, false);
        mask.set(6, false);
        mask.set(7, false);

        assert_eq!(mask.mask[0], 0b00000000);

        mask.set(0, true);
        mask.set(1, true);
        mask.set(2, true);
        mask.set(3, true);
        mask.set(4, true);
        mask.set(5, true);
        mask.set(6, true);
        mask.set(7, true);

        assert_eq!(mask.mask[0], 0b11111111);

        mask.set(0, false);
        mask.set(1, false);
        mask.set(2, false);
        mask.set(3, false);
        mask.set(4, false);
        mask.set(5, false);
        mask.set(6, false);
        mask.set(7, false);

        assert_eq!(mask.mask[0], 0b00000000);
    }
}