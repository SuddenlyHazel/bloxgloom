use super::types::EntityError;

pub const ENTITY_RECORD_MAGIC: &[u8; 4] = b"BGER";
pub const ENTITY_RECORD_VERSION: u16 = 1;
pub const ENTITY_CHUNK_PAGE_MAGIC: &[u8; 4] = b"BGCI";
pub const ENTITY_CHUNK_PAGE_VERSION: u16 = 1;
pub const ENTITY_ALLOCATOR_MAGIC: &[u8; 4] = b"BGEA";
pub const ENTITY_ALLOCATOR_VERSION: u16 = 1;
pub const ENTITY_CELL_VALUE_MAGIC: &[u8; 4] = b"BGEC";
pub const ENTITY_CELL_VALUE_VERSION: u16 = 1;

pub struct Encoder {
    bytes: Vec<u8>,
}

impl Encoder {
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            bytes: Vec::with_capacity(capacity),
        }
    }

    pub fn raw(&mut self, value: &[u8]) {
        self.bytes.extend_from_slice(value);
    }

    pub fn len(&self) -> usize {
        self.bytes.len()
    }

    pub fn u8(&mut self, value: u8) {
        self.bytes.push(value);
    }

    pub fn u16(&mut self, value: u16) {
        self.raw(&value.to_le_bytes());
    }

    pub fn u32(&mut self, value: u32) {
        self.raw(&value.to_le_bytes());
    }

    pub fn u64(&mut self, value: u64) {
        self.raw(&value.to_le_bytes());
    }

    pub fn i32(&mut self, value: i32) {
        self.raw(&value.to_le_bytes());
    }

    pub fn f32(&mut self, value: f32) {
        self.u32(value.to_bits());
    }

    pub fn length_bytes(&mut self, value: &[u8]) -> Result<(), EntityError> {
        let length = u32::try_from(value.len()).map_err(|_| EntityError::PayloadTooLarge)?;
        self.u32(length);
        self.raw(value);
        Ok(())
    }

    pub fn finish_crc(mut self) -> Result<Vec<u8>, EntityError> {
        let checksum = crc32(&self.bytes);
        self.bytes.extend_from_slice(&checksum.to_le_bytes());
        Ok(self.bytes)
    }
}

pub struct Decoder<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> Decoder<'a> {
    pub fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    pub fn raw(&mut self, length: usize) -> Result<&'a [u8], EntityError> {
        let end = self
            .offset
            .checked_add(length)
            .ok_or(EntityError::CorruptCheckpoint)?;
        let value = self
            .bytes
            .get(self.offset..end)
            .ok_or(EntityError::CorruptCheckpoint)?;
        self.offset = end;
        Ok(value)
    }

    pub fn u8(&mut self) -> Result<u8, EntityError> {
        Ok(self.raw(1)?[0])
    }

    pub fn u16(&mut self) -> Result<u16, EntityError> {
        Ok(u16::from_le_bytes(
            self.raw(2)?
                .try_into()
                .map_err(|_| EntityError::CorruptCheckpoint)?,
        ))
    }

    pub fn u32(&mut self) -> Result<u32, EntityError> {
        Ok(u32::from_le_bytes(
            self.raw(4)?
                .try_into()
                .map_err(|_| EntityError::CorruptCheckpoint)?,
        ))
    }

    pub fn u64(&mut self) -> Result<u64, EntityError> {
        Ok(u64::from_le_bytes(
            self.raw(8)?
                .try_into()
                .map_err(|_| EntityError::CorruptCheckpoint)?,
        ))
    }

    pub fn i32(&mut self) -> Result<i32, EntityError> {
        Ok(i32::from_le_bytes(
            self.raw(4)?
                .try_into()
                .map_err(|_| EntityError::CorruptCheckpoint)?,
        ))
    }

    pub fn f32(&mut self) -> Result<f32, EntityError> {
        Ok(f32::from_bits(self.u32()?))
    }

    pub fn length_bytes(&mut self, maximum: usize) -> Result<&'a [u8], EntityError> {
        let length = usize::try_from(self.u32()?).map_err(|_| EntityError::CorruptCheckpoint)?;
        if length > maximum {
            return Err(EntityError::CorruptCheckpoint);
        }
        self.raw(length)
    }

    pub fn finish(self) -> Result<(), EntityError> {
        if self.offset == self.bytes.len() {
            Ok(())
        } else {
            Err(EntityError::CorruptCheckpoint)
        }
    }
}

pub fn checked_body(bytes: &[u8], maximum: usize) -> Result<&[u8], EntityError> {
    if bytes.len() < 4 || bytes.len() > maximum {
        return Err(EntityError::CorruptCheckpoint);
    }
    let body_len = bytes.len() - 4;
    let expected = u32::from_le_bytes(
        bytes[body_len..]
            .try_into()
            .map_err(|_| EntityError::CorruptCheckpoint)?,
    );
    if crc32(&bytes[..body_len]) != expected {
        return Err(EntityError::CorruptCheckpoint);
    }
    Ok(&bytes[..body_len])
}

pub fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = !0u32;
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            let mask = 0u32.wrapping_sub(crc & 1);
            crc = (crc >> 1) ^ (0xedb8_8320 & mask);
        }
    }
    !crc
}
