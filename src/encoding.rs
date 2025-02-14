use std::io;

/// Defines all supported encoding types.
#[derive(Clone, Debug)]
pub enum EncodingType {
    NRZ,
    NRZI,
    FM0,
    FM1,
    Manchester,
}

/// Trait for encoding and decoding bitstreams.
pub trait Encoding {
    fn get_clock_ratio(&self) -> u32;
    fn encode(&self, input: &[u8]) -> Vec<u8>;
    fn decode(&self, input: &[u8]) -> io::Result<Vec<u8>>;
}

/// NRZ Encoding (Raw mode, no changes).
struct NRZEncoding;
impl Encoding for NRZEncoding {
    fn get_clock_ratio(&self) -> u32 {
        1
    }

    fn encode(&self, input: &[u8]) -> Vec<u8> {
        input.to_vec() // No encoding, return input directly
    }

    fn decode(&self, encoded: &[u8]) -> io::Result<Vec<u8>> {
        Ok(encoded.to_vec()) // No decoding, return encoded directly
    }
}

/// NRZI Encoding.
struct NRZIEncoding;
impl Encoding for NRZIEncoding {
    fn get_clock_ratio(&self) -> u32 {
        1
    }

    fn encode(&self, input: &[u8]) -> Vec<u8> {
        let mut encoded = vec![0u8; input.len()];
        let mut last_state = 1; // Assume line starts high

        for (i, &byte) in input.iter().enumerate() {
            for bit in (0..8).rev() {
                let data_bit = (byte >> bit) & 1;
                if data_bit == 1 {
                    last_state ^= 1; // Toggle state
                }
                if last_state == 1 {
                    encoded[i] |= 1 << bit;
                }
            }
        }
        encoded
    }

    fn decode(&self, encoded: &[u8]) -> io::Result<Vec<u8>> {
        let mut decoded = vec![0u8; encoded.len()];
        let mut last_state = 1;

        for (i, &byte) in encoded.iter().enumerate() {
            for bit in (0..8).rev() {
                let current_state = (byte >> bit) & 1;
                let decoded_bit = if current_state == last_state { 0 } else { 1 };
                decoded[i] |= decoded_bit << bit;
                last_state = current_state;
            }
        }
        Ok(decoded)
    }
}

/// FM0 Encoding.
struct FM0Encoding;
impl Encoding for FM0Encoding {
    fn get_clock_ratio(&self) -> u32 {
        2
    }

    fn encode(&self, input: &[u8]) -> Vec<u8> {
        let mut last_state = 1;
        let input_bits = input.len() * 8;
        let mut encoded = vec![0u8; (input_bits * 2 + 7) / 8]; // Allocate output buffer
        let mut bit_idx = 0;

        for i in 0..input_bits {
            let bit = (input[i / 8] >> (7 - (i % 8))) & 1;

            if bit == 1 {
                encoded[bit_idx / 8] |= (last_state ^ 1) << (7 - (bit_idx % 8));
                bit_idx += 1;
                encoded[bit_idx / 8] |= last_state << (7 - (bit_idx % 8));
            } else {
                last_state ^= 1; // Toggle last state
                encoded[bit_idx / 8] |= last_state << (7 - (bit_idx % 8));
                bit_idx += 1;
                encoded[bit_idx / 8] |= last_state << (7 - (bit_idx % 8));
            }
            bit_idx += 1;
        }

        encoded
    }

    fn decode(&self, encoded: &[u8]) -> io::Result<Vec<u8>> {
        let mut decoded = vec![0u8; encoded.len() / 2];

        for (bit_idx, i) in (0..(encoded.len() * 8)).step_by(2).enumerate() {
            let first_bit = (encoded[i / 8] >> (7 - (i % 8))) & 1;
            let second_bit = (encoded[(i + 1) / 8] >> (7 - ((i + 1) % 8))) & 1;

            if first_bit == second_bit {
                decoded[bit_idx / 8] &= !(1 << (7 - (bit_idx % 8))); // Original bit = 0
            } else {
                decoded[bit_idx / 8] |= 1 << (7 - (bit_idx % 8)); // Original bit = 1
            }
        }

        Ok(decoded)
    }
}

/// FM1 Encoding.
struct FM1Encoding;
impl Encoding for FM1Encoding {
    fn get_clock_ratio(&self) -> u32 {
        2
    }

    fn encode(&self, input: &[u8]) -> Vec<u8> {
        let mut last_state = 1;
        let input_bits = input.len() * 8;
        let mut encoded = vec![0u8; (input_bits * 2 + 7) / 8];
        let mut bit_idx = 0;

        for i in 0..input_bits {
            let bit = (input[i / 8] >> (7 - (i % 8))) & 1;

            if bit == 1 {
                last_state ^= 1;
                encoded[bit_idx / 8] |= last_state << (7 - (bit_idx % 8));
            } else {
                encoded[bit_idx / 8] |= last_state << (7 - (bit_idx % 8));
            }
            bit_idx += 1;
            encoded[bit_idx / 8] |= last_state << (7 - (bit_idx % 8));
            bit_idx += 1;
        }

        encoded
    }

    fn decode(&self, encoded: &[u8]) -> io::Result<Vec<u8>> {
        let mut last_state = 1;
        let mut decoded = vec![0u8; encoded.len() / 2];

        for (bit_idx, i) in (0..(encoded.len() * 8)).step_by(2).enumerate() {
            let first_bit = (encoded[i / 8] >> (7 - (i % 8))) & 1;
            let second_bit = (encoded[(i + 1) / 8] >> (7 - ((i + 1) % 8))) & 1;

            if first_bit != second_bit {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("❌ FM1 decoding error at bit position {}", i),
                ));
            } else if first_bit != last_state {
                // Transition at the start → Decoded bit = 1
                decoded[bit_idx / 8] |= 1 << (7 - (bit_idx % 8));
            } else {
                // No transition → Decoded bit = 0
                decoded[bit_idx / 8] &= !(1 << (7 - (bit_idx % 8)));
            }
            last_state = second_bit;
        }

        Ok(decoded)
    }
}

/// Manchester Encoding.
struct ManchesterEncoding;
impl Encoding for ManchesterEncoding {
    fn get_clock_ratio(&self) -> u32 {
        2
    }

    fn encode(&self, input: &[u8]) -> Vec<u8> {
        let input_bits = input.len() * 8;
        let mut encoded = vec![0u8; (input_bits * 2 + 7) / 8];
        let mut bit_idx = 0;

        for i in 0..input_bits {
            let bit = (input[i / 8] >> (7 - (i % 8))) & 1;

            if bit == 1 {
                encoded[bit_idx / 8] |= 0 << (7 - (bit_idx % 8)); // LOW
                bit_idx += 1;
                encoded[bit_idx / 8] |= 1 << (7 - (bit_idx % 8)); // HIGH
            } else {
                encoded[bit_idx / 8] |= 1 << (7 - (bit_idx % 8)); // HIGH
                bit_idx += 1;
                encoded[bit_idx / 8] |= 0 << (7 - (bit_idx % 8)); // LOW
            }
            bit_idx += 1;
        }

        encoded
    }

    fn decode(&self, encoded: &[u8]) -> io::Result<Vec<u8>> {
        let mut decoded = vec![0u8; encoded.len() / 2];

        for (bit_idx, i) in (0..encoded.len() * 8).step_by(2).enumerate() {
            let first_bit = (encoded[i / 8] >> (7 - (i % 8))) & 1;
            let second_bit = (encoded[(i + 1) / 8] >> (7 - ((i + 1) % 8))) & 1;

            if first_bit == 0 && second_bit == 1 {
                // Manchester encoding: LOW → HIGH transition means original bit = 1
                decoded[bit_idx / 8] |= 1 << (7 - (bit_idx % 8));
            } else if first_bit == 1 && second_bit == 0 {
                // Manchester encoding: HIGH → LOW transition means original bit = 0
                decoded[bit_idx / 8] &= !(1 << (7 - (bit_idx % 8)));
            } else {
                // Invalid Manchester sequence (00 or 11), return an error
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("❌ Manchester decoding error at bit position {}", i),
                ));
            }
        }

        Ok(decoded)
    }
}

/// Retrieves the correct encoder/decoder.
impl EncodingType {
    pub fn get_encoder(&self) -> Box<dyn Encoding> {
        match self {
            EncodingType::NRZ => Box::new(NRZEncoding),
            EncodingType::NRZI => Box::new(NRZIEncoding),
            EncodingType::FM0 => Box::new(FM0Encoding),
            EncodingType::FM1 => Box::new(FM1Encoding),
            EncodingType::Manchester => Box::new(ManchesterEncoding),
        }
    }
}

/// Unit Tests for Encoding & RS485.
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encoding_nrz() {
        let encoding_type = EncodingType::NRZ;
        let encoder = encoding_type.get_encoder();
        let data = vec![0xA5, 0x5A];
        let encoded_data = encoder.encode(&data);
        let received_data = encoder
            .decode(&encoded_data)
            .expect("Failed to decode NRZ data");
        assert_eq!(received_data, data, "Decoded data does not match original");
        println!("✅ test_encoding_nrz() (NRZ) passed!");
    }

    #[test]
    fn test_encoding_nrzi() {
        let encoding_type = EncodingType::NRZI;
        let encoder = encoding_type.get_encoder();
        let data = vec![0xA5, 0x5A];
        let encoded_data = encoder.encode(&data);
        let received_data = encoder
            .decode(&encoded_data)
            .expect("Failed to decode NRZI data");
        assert_eq!(received_data, data, "Decoded data does not match original");
        println!("✅ test_encoding_nrzi() (NRZI) passed!");
    }

    #[test]
    fn test_encoding_fm0() {
        let encoding_type = EncodingType::FM0;
        let encoder = encoding_type.get_encoder();
        let data = vec![0xA5, 0x5A];
        let encoded_data = encoder.encode(&data);
        let received_data = encoder
            .decode(&encoded_data)
            .expect("Failed to decode FM0 data");
        assert_eq!(received_data, data, "Decoded data does not match original");
        println!("✅ test_encoding_fm0() (FM0) passed!");
    }

    #[test]
    fn test_encoding_fm1() {
        let encoding_type = EncodingType::FM1;
        let encoder = encoding_type.get_encoder();
        let data = vec![0xA5, 0x5A];
        let encoded_data = encoder.encode(&data);
        let received_data = encoder
            .decode(&encoded_data)
            .expect("Failed to decode FM1 data");
        assert_eq!(received_data, data, "Decoded data does not match original");
        println!("✅ test_encoding_fm1() (FM1) passed!");
    }

    #[test]
    fn test_encoding_manchester() {
        let encoding_type = EncodingType::Manchester;
        let encoder = encoding_type.get_encoder();
        let data = vec![0xA5, 0x5A];
        let encoded_data = encoder.encode(&data);
        let received_data = encoder
            .decode(&encoded_data)
            .expect("Failed to decode Manchester data");
        assert_eq!(received_data, data, "Decoded data does not match original");
        println!("✅ test_encoding_manchester() (Manchester) passed!");
    }
}
