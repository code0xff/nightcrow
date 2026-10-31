use super::*;
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;

fn b64(s: &str) -> Vec<u8> {
    URL_SAFE_NO_PAD.decode(s).unwrap()
}

/// RFC 8291 Appendix A, the worked example.
#[test]
fn the_rfc_8291_example_encrypts_byte_for_byte() {
    let receiver_public = b64(
        "BCVxsr7N_eNgVRqvHtD0zTZsEc6-VV-JvLexhqUzORcxaOzi6-AYWXvTBHm4bjyPjs7Vd8pZGH6SRpkNtoIAiw4",
    );
    let auth = b64("BTBZMqHH6r4Tts7J_aSIgg");
    let sender =
        SecretKey::from_slice(&b64("yfWPiYE-n46HLnH0KqZOF1fJJU3MYrct3AELtAQ-oRw")).unwrap();
    let salt: [u8; 16] = b64("DGv6ra1nlYgDCS1FRnbzlw").try_into().unwrap();
    let recipient = Recipient {
        public_key: &receiver_public,
        auth_secret: &auth,
    };

    let body = encrypt_with(
        &recipient,
        b"When I grow up, I want to be a watermelon",
        &sender,
        &salt,
    )
    .unwrap();

    assert_eq!(
        URL_SAFE_NO_PAD.encode(body),
        "DGv6ra1nlYgDCS1FRnbzlwAAEABBBP4z9KsN6nGRTbVYI_c7VJSPQTBtkgcy27mlmlMoZIIgDll6e3vCYLocInmYWAmS6TlzAC8wEqKK6PBru3jl7A_yl95bQpu6cVPTpK4Mqgkf1CXztLVBSt2Ks3oZwbuwXPXLWyouBWLVWGNWQexSgSxsj_Qulcy4a-fN"
    );
}

#[test]
fn every_message_gets_a_fresh_key_and_salt() {
    let browser = random_secret_key().unwrap();
    let public = uncompressed(&browser.public_key());
    let recipient = Recipient {
        public_key: &public,
        auth_secret: &[7u8; 16],
    };
    let one = encrypt(&recipient, b"hi").unwrap();
    let two = encrypt(&recipient, b"hi").unwrap();
    assert_eq!(one.len(), 16 + 4 + 1 + 65 + 2 + 1 + 16);
    assert_ne!(one[..16], two[..16], "salt reused");
    assert_ne!(one[21..86], two[21..86], "sender key reused");
}

#[test]
fn a_malformed_subscription_key_is_refused() {
    let short = Recipient {
        public_key: &[4u8; 33],
        auth_secret: &[0u8; 16],
    };
    assert!(encrypt(&short, b"x").is_err());
    let off_curve = Recipient {
        public_key: &[4u8; 65],
        auth_secret: &[0u8; 16],
    };
    assert!(encrypt(&off_curve, b"x").is_err());
    let browser = random_secret_key().unwrap();
    let public = uncompressed(&browser.public_key());
    let bad_auth = Recipient {
        public_key: &public,
        auth_secret: &[0u8; 8],
    };
    assert!(encrypt(&bad_auth, b"x").is_err());
}
