use crate::ZoomError;
use crate::arguments::Arguments;

use std::fs;
use std::collections::HashMap;

use reqwest::{Client, header};

/// Fetch data, either from an URL or a path to a local file.
/// If uri doesnt start with "http(s)://", it is considered to be a path
/// to a local file
// TODO: return Bytes
pub async fn fetch_uri(uri: &str, http: &Client) -> Result<Vec<u8>, ZoomError> {
    if uri.starts_with("http://") || uri.starts_with("https://") {
        println!("Downloading {}...", uri);
        let response = http.get(uri).send().await?.error_for_status()?;
        let mut contents = Vec::new();
        contents.extend(response.bytes().await?);
        Ok(contents)
    } else {
        println!("Opening {}...", uri);
        Ok(fs::read(uri)?)
    }
}


pub fn client<'a, I: Iterator<Item=(&'a String, &'a String)>>(
    headers: I,
    args: &Arguments,
) -> Result<reqwest::Client, ZoomError> {
    let header_map: Result<header::HeaderMap, ZoomError> = default_headers()
        .iter()
        .chain(headers.map(|(k, v)| (k, v)))
        .map(|(name, value)| Ok((name.parse()?, value.parse()?)))
        .collect();
    let client = reqwest::Client::builder()
        .default_headers(header_map?)
        .max_idle_per_host(args.max_idle_per_host)
        .danger_accept_invalid_certs(args.accept_invalid_certs)
        .timeout(args.timeout)
        .build()?;
    Ok(client)
}

pub fn default_headers() -> HashMap<String, String> {
    serde_yaml::from_str(include_str!("default_headers.yaml")).unwrap()
}