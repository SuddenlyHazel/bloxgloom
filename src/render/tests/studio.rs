//! Controlled illumination for material and animation fixtures, independent of outdoor art.

/// A constant incoming diffuse term exercises the production SH consumer while
/// keeping palette/pose tests independent of sun angle, atmospheric color and GI.
/// `directional` is white directional irradiance; receivers still apply 1/pi.
pub(in crate::render) fn light(packet: &mut [f32], diffuse: f32, directional: f32) {
    assert!(
        packet.len() >= 80,
        "studio fixture needs the SH camera packet"
    );
    packet[36..39].fill(directional);
    packet[44..47].fill(diffuse);
    packet[48..51].fill(diffuse);
    packet[56..80].fill(0.0);
    packet[56..59].fill(diffuse);
    packet[59] = 1.0;
}
