use libftd2xx::{Ft2232h, FtdiCommon, FtdiMpsse, MpsseSettings};
use std::io;
use std::time::Duration;

pub mod encoding;

/// Opens and configures FT2232H for RS485 communication.
fn init_ftdi_rs485(
    target_baud_rate: u32,
    encoding_type: encoding::EncodingType,
) -> io::Result<Ft2232h> {
    let mut ftdi = Ft2232h::with_description("Dual RS232-HS A")
        .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("FTDI error: {:?}", e)))?;

    // Adjust baud rate for encoding
    let encoder = encoding_type.get_encoder();
    let adjusted_baud_rate = target_baud_rate * encoder.get_clock_ratio();

    if !(300..=12_000_000).contains(&adjusted_baud_rate) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Baud rate out of range",
        ));
    }

    // Configure MPSSE mode for RS485
    let settings = MpsseSettings {
        clock_frequency: Some(adjusted_baud_rate),
        latency_timer: Duration::from_millis(1),
        ..Default::default()
    };

    ftdi.initialize_mpsse(&settings).map_err(|e| {
        io::Error::new(
            io::ErrorKind::Other,
            format!("Failed to initialize MPSSE: {:?}", e),
        )
    })?;

    println!(
        "✅ FT2232H initialized in RS485 mode with adjusted baud rate: {}",
        adjusted_baud_rate
    );

    Ok(ftdi)
}

/// Sends encoded data via RS485.
fn rs485_send<T: FtdiCommon>(
    ftdi: &mut T,
    data: &[u8],
    encoding_type: encoding::EncodingType,
) -> io::Result<()> {
    let encoder = encoding_type.get_encoder();
    let encoded_data = encoder.encode(data);

    // Prepare MPSSE write packet (0x19 command)
    let mut packet = vec![0x19];
    packet.push(((encoded_data.len() - 1) & 0xFF) as u8);
    packet.push(((encoded_data.len() - 1) >> 8) as u8);
    packet.extend_from_slice(&encoded_data);

    // Send via FTDI
    ftdi.write(&packet)
        .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("FTDI write failed: {:?}", e)))?;

    Ok(())
}

/// Receives data from RS485 via FTDI.
fn rs485_receive<T: FtdiCommon>(
    ftdi: &mut T,
    max_len: usize,
    encoding_type: encoding::EncodingType,
) -> io::Result<Vec<u8>> {
    let decoder = encoding_type.get_encoder();
    let mut encoded_data = vec![0u8; max_len * decoder.get_clock_ratio() as usize];

    // Read data from FTDI
    let bytes_read = ftdi
        .read(&mut encoded_data)
        .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("FTDI read failed: {:?}", e)))?;

    if bytes_read == 0 {
        return Err(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            "No data received",
        ));
    }

    // Decode data
    let decoded = decoder.decode(&encoded_data).map_err(|e| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("Decoding failed: {:?}", e),
        )
    })?;

    Ok(decoded)
}

fn main() {
    let target_baud_rate = 1_000_000; // 1 Mbps
    let encoding = encoding::EncodingType::FM0;

    let mut ftdi = init_ftdi_rs485(target_baud_rate, encoding.clone())
        .expect("❌ Failed to open FTDI device by description");
    println!("✅ Successfully opened FTDI device");

    let tx_data = vec![0xA5, 0x5A]; // Test data

    println!("🚀 Sending RS485 data...");
    rs485_send(&mut ftdi, &tx_data, encoding.clone()).expect("Failed to send RS485 data");

    println!("📡 Receiving RS485 data...");
    match rs485_receive(&mut ftdi, tx_data.len(), encoding.clone()) {
        Ok(received_data) => {
            println!("✅ Received Data: {:?}", received_data);
            if received_data == tx_data {
                println!("🎉 RS485 Communication Successful!");
            } else {
                println!("❌ Data mismatch!");
            }
        }
        Err(err) => println!("⚠️ Receive error: {}", err),
    }
}

/// Unit Tests for Encoding & RS485.
#[cfg(test)]
mod tests {
    use super::*;
    use libftd2xx::TimeoutError;
    use libftd2xx::{DeviceType, FtStatus, FtdiCommon, FtdiMpsse, MpsseSettings};
    use std::cell::RefCell;
    use std::ffi::c_void;
    use std::ptr;

    #[derive(Default)]
    pub struct MockFt2232h {
        buffer: RefCell<Vec<u8>>, // Simulated internal device buffer
    }

    impl MockFt2232h {
        pub fn new() -> Self {
            Self {
                buffer: RefCell::new(Vec::new()),
            }
        }
    }

    impl FtdiCommon for MockFt2232h {
        const DEVICE_TYPE: DeviceType = DeviceType::FT2232H; // Corrected uppercase variant

        fn handle(&mut self) -> *mut c_void {
            ptr::null_mut() // Return a null pointer since it's a mock
        }

        fn write(&mut self, data: &[u8]) -> Result<usize, FtStatus> {
            if data.len() < 3 {
                return Err(FtStatus::INVALID_PARAMETER); // Ensure minimum packet size
            }

            // Store only the actual encoded data (skip first 3 bytes)
            self.buffer.borrow_mut().extend_from_slice(&data[3..]);

            Ok(data.len()) // Simulate successful write
        }

        fn read(&mut self, data: &mut [u8]) -> Result<usize, FtStatus> {
            let mut buffer = self.buffer.borrow_mut();

            let len = buffer.len().min(data.len());
            if len == 0 {
                return Err(FtStatus::DEVICE_NOT_FOUND);
            }

            data[..len].copy_from_slice(&buffer[..len]);
            buffer.drain(..len); // Properly remove read bytes

            // Debug: Check if we are accidentally adding padding bytes
            if data.len() > len {
                println!("⚠️ Unexpected padding detected: {:?}", &data[len..]);
            }

            Ok(len)
        }
    }

    impl FtdiMpsse for MockFt2232h {
        fn initialize_mpsse(&mut self, _settings: &MpsseSettings) -> Result<(), TimeoutError> {
            Ok(()) // Assume successful initialization
        }
    }

    #[test]
    fn test_rs485_send_receive_nrz() {
        let mut ftdi = MockFt2232h::new();
        let data = vec![0xA5, 0x5A];

        rs485_send(&mut ftdi, &data, encoding::EncodingType::NRZ).expect("Failed to send NRZ data");

        let received_data = rs485_receive(&mut ftdi, data.len(), encoding::EncodingType::NRZ)
            .expect("Failed to receive NRZ data");

        assert_eq!(received_data, data, "Decoded data does not match original");
        println!("✅ rs485_send_receive() (NRZ) passed!");
    }

    #[test]
    fn test_rs485_send_receive_nrzi() {
        let mut ftdi = MockFt2232h::new();
        let data = vec![0xA5, 0x5A];

        rs485_send(&mut ftdi, &data, encoding::EncodingType::NRZI)
            .expect("Failed to send NRZI data");

        let received_data = rs485_receive(&mut ftdi, data.len(), encoding::EncodingType::NRZI)
            .expect("Failed to receive NRZI data");

        assert_eq!(received_data, data, "Decoded data does not match original");
        println!("✅ rs485_send_receive() (NRZI) passed!");
    }

    #[test]
    fn test_rs485_send_receive_fm0() {
        let mut ftdi = MockFt2232h::new();
        let data = vec![0xA5, 0x5A];

        rs485_send(&mut ftdi, &data, encoding::EncodingType::FM0).expect("Failed to send FM0 data");

        let received_data = rs485_receive(&mut ftdi, data.len(), encoding::EncodingType::FM0)
            .expect("Failed to receive FM0 data");

        assert_eq!(received_data, data, "Decoded data does not match original");
        println!("✅ rs485_send_receive() (FM0) passed!");
    }

    #[test]
    fn test_rs485_send_receive_fm1() {
        let mut ftdi = MockFt2232h::new();
        let data = vec![0xA5, 0x5A];

        rs485_send(&mut ftdi, &data, encoding::EncodingType::FM1).expect("Failed to send FM1 data");

        let received_data = rs485_receive(&mut ftdi, data.len(), encoding::EncodingType::FM1)
            .expect("Failed to receive FM1 data");

        assert_eq!(received_data, data, "Decoded data does not match original");
        println!("✅ rs485_send_receive() (FM1) passed!");
    }

    #[test]
    fn test_rs485_send_receive_manchester() {
        let mut ftdi = MockFt2232h::new();
        let data = vec![0xA5, 0x5A];

        rs485_send(&mut ftdi, &data, encoding::EncodingType::Manchester)
            .expect("Failed to send Manchester data");

        let received_data =
            rs485_receive(&mut ftdi, data.len(), encoding::EncodingType::Manchester)
                .expect("Failed to receive Manchester data");

        assert_eq!(received_data, data, "Decoded data does not match original");
        println!("✅ rs485_send_receive() (Manchester) passed!");
    }
}
