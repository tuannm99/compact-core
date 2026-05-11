use crate::CompactError;

///
/// Layouts: [count][byte][count][byte]...
///
/// AAABBBCCC -> 3A 3B 3C
/// In case more than 255 (u8 max) then split it -> 255A 45A 3B 3C
pub fn encode_delta(data: &[u8]) -> Vec<u8> {
    let mut result = Vec::with_capacity(data.len() * 2);

    result
}

pub fn decode_delta(data: &[u8]) -> Result<Vec<u8>, CompactError> {
    let mut result = Vec::new();

    Ok(result)
}

#[cfg(test)]
mod tests {}
