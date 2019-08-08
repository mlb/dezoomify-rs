use std::io::Write;

use hmac::{Hmac, Mac};
use sha1::Sha1;

use super::PageInfo;

type HmacSha1 = Hmac<Sha1>;

pub fn compute_url(page: &PageInfo, x: u32, y: u32, z: usize) -> String {
    let mut url = format!("{}=x{}-y{}-z{}-t", page.base_url, x, y, z);

    let mut sign_path: Vec<u8> = Vec::new();
    sign_path.extend_from_slice(page.path().as_bytes());
    write!(sign_path, "=x{}-y{}-z{}-t", x, y, z).unwrap();
    sign_path.extend_from_slice(page.token.as_bytes());

    let digest = mac_digest(&sign_path);
    url.push_str(&custom_base64(&digest));
    url
}

fn custom_base64(digest: &[u8]) -> String {
    base64::encode_config(digest, base64::URL_SAFE_NO_PAD).replace('-', "_")
}

fn mac_digest(b: &[u8]) -> Vec<u8> {
    let key = &[123, 43, 78, 35, 222, 44, 197, 197];
    let mut mac = HmacSha1::new_varkey(key).expect("HMac keys can have any length");
    mac.input(b);
    mac.result().code().to_vec()
}


#[test]
fn test_compute_url() {
    let path = "https://lh3.googleusercontent.com/wGcDNN8L-2COcm9toX5BTp6HPxpMPPPuxrMU-ZL-W-nDHW8I_L4R5vlBJ6ITtlmONQ".into();
    let token = "KwCgJ1QIfgprHn0a93x7Q-HhJ04".into();
    assert_eq!(
        compute_url(&PageInfo { base_url: path, token, name: "".into() }, 0, 0, 7),
        "https://lh3.googleusercontent.com/wGcDNN8L-2COcm9toX5BTp6HPxpMPPPuxrMU-ZL-W-nDHW8I_L4R5vlBJ6ITtlmONQ=x0-y0-z7-tHeJ3xylnSyyHPGwMZimI4EV3JP8"
    );
}