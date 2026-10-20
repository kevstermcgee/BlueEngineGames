//! Verify immutable launcher bytes and the signed version with the source's Tauri key.
use base64::{engine::general_purpose::STANDARD, Engine};
use minisign_verify::{PublicKey, Signature};
use std::{env, fs, process::ExitCode};

fn verify(
    data: &[u8],
    encoded_signature: &str,
    encoded_key: &str,
    version: &str,
) -> Result<(), String> {
    let decode = |value: &str| -> Result<String, String> {
        String::from_utf8(
            STANDARD
                .decode(value.trim())
                .map_err(|_| "Invalid signature encoding")?,
        )
        .map_err(|_| "Invalid signature text".into())
    };
    let key = PublicKey::decode(&decode(encoded_key)?).map_err(|_| "Invalid update key")?;
    let signature =
        Signature::decode(&decode(encoded_signature)?).map_err(|_| "Invalid update signature")?;
    key.verify(data, &signature, true)
        .map_err(|_| "Installer signature verification failed")?;
    let versions: Vec<_> = signature
        .trusted_comment()
        .split('\t')
        .filter_map(|part| part.strip_prefix("version:"))
        .collect();
    if versions != [version] {
        return Err("The signature does not certify this exact launcher version".into());
    }
    Ok(())
}

fn run() -> Result<(), String> {
    let args: Vec<_> = env::args_os().skip(1).collect();
    if args.len() != 4 {
        return Err(
            "Usage: verify-update-signature <tauri.conf.json> <installer> <signature> <version>"
                .into(),
        );
    }
    let config: serde_json::Value =
        serde_json::from_slice(&fs::read(&args[0]).map_err(|_| "Cannot read signing config")?)
            .map_err(|_| "Cannot parse signing config")?;
    let key = config["plugins"]["updater"]["pubkey"]
        .as_str()
        .ok_or("Missing update public key")?;
    let data = fs::read(&args[1]).map_err(|_| "Cannot read installer")?;
    let signature = fs::read_to_string(&args[2]).map_err(|_| "Cannot read signature")?;
    let version = args[3].to_str().ok_or("Invalid version")?;
    verify(&data, &signature, key, version)
}
fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // Public Tauri updater test vector: payload `test`, signed for 2.0.0 with a throwaway key.
    const KEY: &str = "untrusted comment: minisign public key: A49EF4746FA6E6D7\nRWTX5qZvdPSepDGWg2bz8n6ArQnBMwgQK1geVosorcbLzcunD+KJ+cYe";
    const SIG: &str = "untrusted comment: signature from tauri secret key\nRUTX5qZvdPSepPF2zsjvP/L/TQtBLRnWRvaq+4omK1UEugtQp/USrGiI1v762sDy3id3aor7bRC9ETyeot6lMHHrWoZ5hjUM1Qc=\ntrusted comment: timestamp:1791649051\tfile:payload\tversion:2.0.0\nDDq2CLOEl5cQmC2eJRHlnZPVViOZsu1CjpPKQoei4A/XBfkDqGO0B+QMrcrHgEQ5qV3mne75tdFWhJpqUXh9AA==";
    #[test]
    fn retained_installer_must_match_signature_key_and_version() {
        let key = STANDARD.encode(KEY);
        let signature = STANDARD.encode(SIG);
        assert!(verify(b"test", &signature, &key, "2.0.0").is_ok());
        assert!(verify(b"changed", &signature, &key, "2.0.0").is_err());
        assert!(verify(b"test", &signature, &key, "2.0.1").is_err());
        assert!(verify(
            b"test",
            &STANDARD.encode(SIG.replace("file:payload", "file:other")),
            &key,
            "2.0.0"
        )
        .is_err());
        assert!(verify(b"test", "invalid", &key, "2.0.0").is_err());
    }
}
