use base64::{engine::general_purpose::STANDARD, Engine as _};
use minisign_verify::{PublicKey, Signature};
use std::{env, ffi::OsString, fs, process::ExitCode};

fn decode_envelope(encoded: &str) -> Result<String, ()> {
    let bytes = STANDARD.decode(encoded.trim()).map_err(|_| ())?;
    String::from_utf8(bytes).map_err(|_| ())
}

fn verify(data: &[u8], encoded_signature: &str, encoded_public_key: &str) -> Result<(), ()> {
    let public_key = PublicKey::decode(&decode_envelope(encoded_public_key)?).map_err(|_| ())?;
    let signature = Signature::decode(&decode_envelope(encoded_signature)?).map_err(|_| ())?;
    // Match tauri-plugin-updater's verification, including legacy minisign support.
    public_key.verify(data, &signature, true).map_err(|_| ())
}

fn run(args: &[OsString]) -> Result<(), ()> {
    if args.len() != 3 {
        return Err(());
    }
    let data = fs::read(&args[0]).map_err(|_| ())?;
    let signature = fs::read_to_string(&args[1]).map_err(|_| ())?;
    let public_key = fs::read_to_string(&args[2]).map_err(|_| ())?;
    verify(&data, &signature, &public_key)
}

fn main() -> ExitCode {
    if run(&env::args_os().skip(1).collect::<Vec<_>>()).is_ok() {
        println!("passed");
        ExitCode::SUCCESS
    } else {
        // Never include paths, key material, comments or third-party error text.
        println!("failed");
        ExitCode::FAILURE
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Public test vectors from minisign-verify 0.2.5 src/lib.rs (MIT).
    // https://github.com/jedisct1/rust-minisign-verify
    const PUBLIC_KEY: &str = "untrusted comment: minisign public key E7620F1842B4E81F\n\
RWQf6LRCGA9i53mlYecO4IzT51TGPpvWucNSCh1CBM0QTaLn73Y7GFO3";
    const PREHASHED_SIGNATURE: &str = "untrusted comment: signature from minisign secret key\n\
RUQf6LRCGA9i559r3g7V1qNyJDApGip8MfqcadIgT9CuhV3EMhHoN1mGTkUidF/\
z7SrlQgXdy8ofjb7bNJJylDOocrCo8KLzZwo=\n\
trusted comment: timestamp:1556193335\tfile:test\n\
y/rUw2y8/hOUYjZU71eHp/Wo1KZ40fGy2VJEDl34XMJM+TX48Ss/17u3IvIfbVR1FkZZSNCisQbuQY+bHwhEBg==";
    const LEGACY_SIGNATURE: &str = "untrusted comment: signature from minisign secret key\n\
RWQf6LRCGA9i59SLOFxz6NxvASXDJeRtuZykwQepbDEGt87ig1BNpWaVWuNrm73YiIiJbq71Wi+dP9eKL8OC351vwIasSSbXxwA=\n\
trusted comment: timestamp:1555779966\tfile:test\n\
QtKMXWyYcwdpZAlPF7tE2ENJkRd1ujvKjlj1m9RtHTBnZPa5WKU5uWRs5GoP5M/VqE81QFuMKI5k/SfNQUaOAA==";

    fn check(data: &[u8], signature: &str, key: &str) -> bool {
        verify(data, &STANDARD.encode(signature), &STANDARD.encode(key)).is_ok()
    }

    #[test]
    fn accepts_tauri_wrapped_prehashed_signature() {
        assert!(check(b"test", PREHASHED_SIGNATURE, PUBLIC_KEY));
    }

    #[test]
    fn accepts_legacy_signature_like_tauri() {
        assert!(check(b"test", LEGACY_SIGNATURE, PUBLIC_KEY));
    }

    #[test]
    fn rejects_changed_truncated_and_extended_payloads() {
        for data in [b"Test".as_slice(), b"tes", b"test\0", b""] {
            assert!(!check(data, PREHASHED_SIGNATURE, PUBLIC_KEY));
        }
    }

    #[test]
    fn rejects_changed_trusted_comment_and_signature() {
        let comment = PREHASHED_SIGNATURE.replace("1556193335", "1556193336");
        assert!(!check(b"test", &comment, PUBLIC_KEY));
        let signature = PREHASHED_SIGNATURE.replace("559r3g7V", "559r3g7W");
        assert!(!check(b"test", &signature, PUBLIC_KEY));
    }

    #[test]
    fn rejects_wrong_public_key() {
        let key = PUBLIC_KEY.replace("73Y7GFO3", "73Y7GFO4");
        assert!(!check(b"test", PREHASHED_SIGNATURE, &key));
    }

    #[test]
    fn rejects_invalid_base64_utf8_and_minisign_text() {
        let signature = STANDARD.encode(PREHASHED_SIGNATURE);
        let key = STANDARD.encode(PUBLIC_KEY);
        for invalid in [
            "%%%".to_owned(),
            STANDARD.encode([0xff]),
            STANDARD.encode("invalid"),
        ] {
            assert!(verify(b"test", &invalid, &key).is_err());
            assert!(verify(b"test", &signature, &invalid).is_err());
        }
    }

    #[test]
    fn accepts_outer_file_line_endings() {
        assert!(verify(
            b"test",
            &format!("{}\r\n", STANDARD.encode(PREHASHED_SIGNATURE)),
            &format!("{}\n", STANDARD.encode(PUBLIC_KEY)),
        )
        .is_ok());
    }

    #[test]
    fn rejects_wrong_argument_count_and_missing_files() {
        assert!(run(&[]).is_err());
        assert!(run(&[OsString::new(); 0]).is_err());
        assert!(run(&["a".into(), "b".into(), "c".into(), "d".into()]).is_err());
        assert!(run(&[OsString::new(), OsString::new(), OsString::new()]).is_err());
    }
}
