use super::*;

#[test]
fn rfc4648_vectors_encode() {
    for (plain, encoded) in [
        ("", ""),
        ("f", "Zg=="),
        ("fo", "Zm8="),
        ("foo", "Zm9v"),
        ("foob", "Zm9vYg=="),
        ("fooba", "Zm9vYmE="),
        ("foobar", "Zm9vYmFy"),
    ] {
        assert_eq!(encode(plain.as_bytes()), encoded);
        assert_eq!(decode(encoded.as_bytes()).unwrap(), plain.as_bytes());
    }
}

#[test]
fn all_byte_values_round_trip() {
    let all: Vec<u8> = (0..=255).collect();
    assert_eq!(decode(encode(&all).as_bytes()).unwrap(), all);
}

#[test]
fn wrapped_output_decodes() {
    // GNU base64 wraps at 76, openssl at 64; both put a newline every N columns.
    let data: Vec<u8> = (0..200u8).collect();
    let enc = encode(&data);
    let wrapped: String = enc
        .as_bytes()
        .chunks(64)
        .map(|c| format!("{}\n", std::str::from_utf8(c).unwrap()))
        .collect();
    assert_eq!(decode(wrapped.as_bytes()).unwrap(), data);
}

#[test]
fn unpadded_input_is_accepted() {
    assert_eq!(decode(b"Zg").unwrap(), b"f");
    assert_eq!(decode(b"Zm8").unwrap(), b"fo");
}

#[test]
fn malformed_input_is_rejected() {
    assert_eq!(decode(b"Zm9v!"), Err(DecodeError::InvalidByte(b'!')));
    assert_eq!(decode(b"Z"), Err(DecodeError::DanglingGroup));
    assert_eq!(decode(b"Zg==Zg"), Err(DecodeError::BadPadding));
}

#[test]
fn od_hex_decodes() {
    // `od -An -v -tx1` output: leading space, 16 bytes per line.
    let out = b" 68 65 6c 6c 6f 0a\n";
    assert_eq!(decode_hex(out).unwrap(), b"hello\n");
    assert_eq!(Base64Tool::Od.decode(out).unwrap(), b"hello\n");
    assert_eq!(decode_hex(b" 6 8").unwrap_err(), DecodeError::OddHex);
    assert_eq!(decode_hex(b"zz"), Err(DecodeError::InvalidByte(b'z')));
}

#[test]
fn probe_output_maps_to_tools() {
    assert_eq!(
        Base64Tool::from_probe_output("gnu-w0\n"),
        Some(Base64Tool::GnuW0)
    );
    assert_eq!(
        Base64Tool::from_probe_output("plain"),
        Some(Base64Tool::Plain)
    );
    assert_eq!(
        Base64Tool::from_probe_output("openssl\n"),
        Some(Base64Tool::Openssl)
    );
    assert_eq!(Base64Tool::from_probe_output(" od "), Some(Base64Tool::Od));
    assert_eq!(Base64Tool::from_probe_output("none\n"), None);
    assert_eq!(Base64Tool::from_probe_output("garbage"), None);
}

#[test]
fn encode_commands_are_golden() {
    assert_eq!(
        Base64Tool::GnuW0.encode_file_command("'/a b'"),
        "base64 -w0 < '/a b'"
    );
    assert_eq!(
        Base64Tool::Plain.encode_file_command("'/x'"),
        "base64 < '/x'"
    );
    assert_eq!(
        Base64Tool::Openssl.encode_file_command("'/x'"),
        "openssl base64 < '/x'"
    );
    assert_eq!(
        Base64Tool::Od.encode_file_command("'/x'"),
        "od -An -v -tx1 < '/x'"
    );
}

#[test]
fn the_probe_command_tries_the_tools_in_order() {
    let p = probe_command();
    let gnu = p.find("base64 -w0").unwrap();
    let plain = p.find("elif base64 </dev/null").unwrap();
    let openssl = p.find("openssl base64").unwrap();
    let od = p.find("command -v od").unwrap();
    assert!(gnu < plain && plain < openssl && openssl < od);
    assert!(p.ends_with("fi"));
}

#[test]
fn the_decode_probe_output_maps_to_a_decoder_and_unknown_is_none() {
    assert_eq!(
        DecodeTool::from_probe_output("d\n"),
        Some(DecodeTool::Lower)
    );
    assert_eq!(DecodeTool::from_probe_output("D"), Some(DecodeTool::Upper));
    assert_eq!(
        DecodeTool::from_probe_output("openssl"),
        Some(DecodeTool::Openssl)
    );
    assert_eq!(DecodeTool::from_probe_output("none"), None);
    assert_eq!(DecodeTool::Lower.filter(), "base64 -d");
    assert_eq!(DecodeTool::Upper.filter(), "base64 -D");
    assert_eq!(DecodeTool::Openssl.filter(), "openssl base64 -d");
    // Each branch decodes a known string rather than trusting that the flag exists.
    let probe = decode_probe_command();
    assert!(probe.contains("aGk=") && probe.contains("= hi"));
}
