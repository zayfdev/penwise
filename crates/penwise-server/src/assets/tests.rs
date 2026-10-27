use super::*;

fn png(width: u32, height: u32) -> Vec<u8> {
    let mut bytes = PNG_SIGNATURE.to_vec();
    bytes.extend_from_slice(&13u32.to_be_bytes());
    bytes.extend_from_slice(b"IHDR");
    bytes.extend_from_slice(&width.to_be_bytes());
    bytes.extend_from_slice(&height.to_be_bytes());
    bytes.extend_from_slice(&[8, 6, 0, 0, 0]);
    bytes.extend_from_slice(&[0, 0, 0, 0]);
    bytes
}

#[test]
fn a_png_is_stored_under_its_digest_and_reads_back() {
    let dir = std::env::temp_dir().join(format!("penwise-assets-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let assets = Assets::load(dir.clone());
    let stored = assets.put(&png(64, 32)).expect("stores");
    assert_eq!((64, 32), (stored.width, stored.height));
    assert_eq!(64, stored.asset.len());
    assert_eq!(
        png(64, 32),
        assets.bytes(&stored.asset).expect("reads back")
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_same_picture_twice_is_one_file() {
    let dir = std::env::temp_dir().join(format!("penwise-assets-dup-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let assets = Assets::load(dir.clone());
    let first = assets.put(&png(8, 8)).expect("stores");
    let second = assets.put(&png(8, 8)).expect("stores");
    assert_eq!(first.asset, second.asset);
    let files = std::fs::read_dir(&dir).expect("lists").count();
    assert_eq!(1, files);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_non_png_is_refused() {
    let dir = std::env::temp_dir().join(format!("penwise-assets-bad-{}", std::process::id()));
    let assets = Assets::load(dir.clone());
    assert!(assets.put(b"GIF89a not a png at all").is_err());
    assert!(
        assets.path("../../etc/passwd").is_none(),
        "no path traversal"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn an_asset_id_round_trips_through_its_digest() {
    let id = "00ff10ab".repeat(8);
    let digest = digest_of(&id).expect("64 hex digits");
    assert_eq!(id, id_of(&digest));
    assert!(digest_of("zz").is_none());
    assert!(digest_of(&"g".repeat(64)).is_none());
    assert!(digest_of(&"é".repeat(32)).is_none(), "64 bytes, not hex");
}
